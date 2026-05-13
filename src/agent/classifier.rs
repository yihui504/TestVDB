use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub fn canonicalize_signal_line(trimmed: &str) -> String {
    let lower = trimmed.to_lowercase();
    let signal = if lower.starts_with("[defect:") {
        lower
            .split(']')
            .next()
            .map(|prefix| format!("{prefix}]"))
            .unwrap_or(lower.clone())
    } else if lower.starts_with("[test_") {
        lower
            .split(']')
            .next()
            .map(|prefix| format!("{prefix}]"))
            .unwrap_or(lower.clone())
    } else {
        return lower;
    };

    let issue_key = [
        "offset",
        "limit",
        "vector",
        "dimension",
        "size",
        "payload",
        "collection",
        "insert",
        "create",
        "parse",
        "address",
        "connect",
    ]
    .iter()
    .find(|token| lower.contains(**token))
    .copied();

    match issue_key {
        Some(issue) => format!("{signal}|{issue}"),
        None => lower,
    }
}

pub fn normalize_observed_output(text: &str) -> String {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        let should_dedupe = trimmed.starts_with("[DEFECT:") || trimmed.starts_with("[TEST_");
        if should_dedupe {
            let canonical = canonicalize_signal_line(trimmed);
            if seen.insert(canonical) {
                normalized.push(line.to_string());
            }
        } else {
            normalized.push(line.to_string());
        }
    }

    normalized.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefectType {
    /// Type-1: Contract violated but database returned HTTP 200 / Success
    IllegalSuccess,
    /// Type-2: Database failed but the error message is unhelpful or incorrect
    PoorDiagnostics,
    /// Type-3: Database process crashed or returned HTTP 500
    RuntimeFailure,
    /// Type-4: Business logic state violated (e.g., inserted 10 records, but count is 0)
    StateLogicViolation,
    /// Not a database bug, just an error in the generated test script (needs retry)
    ScriptError,
    /// The test executed perfectly and the database behaved exactly as contracted
    Pass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassificationDisposition {
    Pass,
    CoverageDetected,
    RetryableScriptError,
    NonDefectInfraError,
    CandidateDefect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassificationResult {
    pub disposition: ClassificationDisposition,
    pub defect_type: Option<DefectType>,
    pub reason: String,
    pub evidence_excerpt: String,
}

/// Analyzes the stdout and stderr from the sandbox to classify the execution result.
pub fn analyze_execution_result(stdout: &str, stderr: &str) -> ClassificationResult {
    let lower_stderr = stderr.to_lowercase();
    let lower_stdout = stdout.to_lowercase();
    let combined_output = format!("{}\n{}", lower_stdout, lower_stderr);

    // 1. Check for Script Errors (LLM hallucinations or environment issues)
    // These should trigger a zero-shot retry
    if lower_stderr.contains("syntaxerror") 
        || lower_stderr.contains("importerror") 
        || lower_stderr.contains("modulenotfounderror")
        || lower_stderr.contains("nameerror") 
        || lower_stderr.contains("typeerror")
        || lower_stdout.contains("[test_infra]") {
        return ClassificationResult {
            disposition: ClassificationDisposition::RetryableScriptError,
            defect_type: Some(DefectType::ScriptError),
            reason: "Generated script failed due to Python/runtime authoring errors or test-infrastructure uncertainty.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
        };
    }

    // Environment or addressing failures must not be upgraded into database defects.
    if combined_output.contains("idna")
        || combined_output.contains("label too long")
        || combined_output.contains("name or service not known")
        || combined_output.contains("temporary failure in name resolution")
        || combined_output.contains("nodename nor servname provided")
        || combined_output.contains("failed to resolve")
        || combined_output.contains("no address associated with hostname") {
        return ClassificationResult {
            disposition: ClassificationDisposition::NonDefectInfraError,
            defect_type: None,
            reason: "Execution failed before a valid database interaction due to addressing or environment issues.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
        };
    }

    // 2. Check for Runtime Failures (Type-3)
    if lower_stderr.contains("connection refused") 
        || lower_stderr.contains("500 internal server error") 
        || lower_stderr.contains("segmentation fault")
        || lower_stdout.contains("panic") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CandidateDefect,
            defect_type: Some(DefectType::RuntimeFailure),
            reason: "Observed a runtime failure after reaching the execution target.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
        };
    }

    // 3. Check for explicit Assertion Failures injected by the LLM test script
    // We expect the script to print specific markers when a contract assertion fails
    if lower_stdout.contains("[defect: illegal_success]") || lower_stderr.contains("assertionerror: illegal success") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CandidateDefect,
            defect_type: Some(DefectType::IllegalSuccess),
            reason: "Observed explicit illegal success marker.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
        };
    }

    if lower_stdout.contains("[defect: poor_diagnostics]") || lower_stderr.contains("assertionerror: poor diagnostics") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CandidateDefect,
            defect_type: Some(DefectType::PoorDiagnostics),
            reason: "Observed explicit poor diagnostics marker.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
        };
    }

    if lower_stdout.contains("[defect: state_logic_violation]") || lower_stderr.contains("assertionerror: state violation") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CandidateDefect,
            defect_type: Some(DefectType::StateLogicViolation),
            reason: "Observed explicit state or logic violation marker.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
        };
    }

    // 4. Generic Assertion Error without a specific marker falls back to StateLogicViolation
    if lower_stderr.contains("assertionerror") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CandidateDefect,
            defect_type: Some(DefectType::StateLogicViolation),
            reason: "Generic assertion failure treated as candidate state/logic violation.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
        };
    }

    if combined_output.contains("[coverage]") || combined_output.contains("[coverage:type") {
        let evidence = combined_output.lines()
            .filter(|l| l.contains("[coverage"))
            .collect::<Vec<_>>()
            .join("\n");
        return ClassificationResult {
            disposition: ClassificationDisposition::CoverageDetected,
            defect_type: None,
            reason: format!("Coverage report detected:\n{}", evidence),
            evidence_excerpt: evidence.chars().take(300).collect(),
        };
    }

    // If there is any unclassified error left in stderr, treat it as a script error
    if !stderr.trim().is_empty() {
        return ClassificationResult {
            disposition: ClassificationDisposition::RetryableScriptError,
            defect_type: Some(DefectType::ScriptError),
            reason: "Unclassified stderr output treated as retryable script error.".to_string(),
            evidence_excerpt: stderr.chars().take(300).collect(),
        };
    }

    ClassificationResult {
        disposition: ClassificationDisposition::Pass,
        defect_type: Some(DefectType::Pass),
        reason: "Execution completed without defect markers.".to_string(),
        evidence_excerpt: stdout.chars().take(300).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_error_module_not_found() {
        let stderr = "Traceback (most recent call last):\n  File \"test.py\", line 2, in <module>\n    import pymilvus_unknown\nModuleNotFoundError: No module named 'pymilvus_unknown'";
        assert_eq!(analyze_execution_result("", stderr).disposition, ClassificationDisposition::RetryableScriptError);
    }

    #[test]
    fn test_script_error_syntax() {
        let stderr = "  File \"test.py\", line 4\n    if True\n          ^\nSyntaxError: expected ':'";
        assert_eq!(analyze_execution_result("", stderr).disposition, ClassificationDisposition::RetryableScriptError);
    }

    #[test]
    fn test_script_error_import() {
        let stderr = "ImportError: cannot import name 'Collection' from 'pymilvus'";
        assert_eq!(analyze_execution_result("", stderr).disposition, ClassificationDisposition::RetryableScriptError);
    }

    #[test]
    fn test_poor_diagnostics() {
        let stderr = "AssertionError: poor diagnostics: expected error message 'Invalid Dimension', got 'Unknown 500 Error'";
        let result = analyze_execution_result("", stderr);
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert_eq!(result.defect_type, Some(DefectType::PoorDiagnostics));
    }

    #[test]
    fn test_runtime_failure() {
        let stderr = "urllib3.exceptions.ConnectionError: HTTPConnectionPool(host='localhost', port=19530): Max retries exceeded with url: /api/v1/collection (Caused by NewConnectionError('<urllib3.connection.HTTPConnection object at 0x7f>: Failed to establish a new connection: [Errno 111] Connection refused'))";
        let result = analyze_execution_result("", stderr);
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert_eq!(result.defect_type, Some(DefectType::RuntimeFailure));
    }

    #[test]
    fn test_illegal_success() {
        let stdout = "Testing create_collection with dimension=-1...\n[DEFECT: ILLEGAL_SUCCESS] Expected error but got 200 OK.";
        let result = analyze_execution_result(stdout, "");
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert_eq!(result.defect_type, Some(DefectType::IllegalSuccess));
    }

    #[test]
    fn test_state_logic_violation() {
        let stderr = "AssertionError: State violation: expected count 10, got 0";
        let result = analyze_execution_result("", stderr);
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert_eq!(result.defect_type, Some(DefectType::StateLogicViolation));
    }

    #[test]
    fn test_pass() {
        let stdout = "All tests passed successfully.";
        let result = analyze_execution_result(stdout, "");
        assert_eq!(result.disposition, ClassificationDisposition::Pass);
        assert_eq!(result.defect_type, Some(DefectType::Pass));
    }

    #[test]
    fn test_environment_addressing_error_does_not_become_poor_diagnostics() {
        let stdout = "[DEFECT: POOR_DIAGNOSTICS] Failed to create collection: {'error': \"encoding with 'idna' codec failed (UnicodeError: label too long)\"}";
        assert_eq!(analyze_execution_result(stdout, "").disposition, ClassificationDisposition::NonDefectInfraError);
    }
}
