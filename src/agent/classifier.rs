use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ── Shared signal constants ──

/// Keywords used to deduplicate defect/test lines by extracting a canonical issue key.
/// Maps signal lines like `[DEFECT: POOR_DIAGNOSTICS] limit must be > 0` → `[defect: poor_diagnostics]|limit`.
const CANONICAL_KEYWORDS: &[&str] = &[
    "offset", "limit", "vector", "dimension", "size", "payload",
    "collection", "insert", "create", "parse", "address", "connect",
];

/// Python exception names used to detect script errors (LLM-generated code failures).
/// When any of these appear in stderr, the run is classified as a retryable script error.
const SCRIPT_ERROR_SIGNALS: &[&str] = &[
    "syntaxerror", "indentationerror", "importerror", "modulenotfounderror",
    "nameerror", "typeerror", "jsondecodeerror", "attributeerror",
    "keyerror", "valueerror", "runtimeerror", "oserror",
    "ioerror", "zerodivisionerror", "overflowerror", "indexerror",
    "filenotfounderror", "permissionerror", "traceback (most recent call last)",
];

/// Infra/dns error signals that indicate environment issues, not database defects.
const INFRA_ERROR_SIGNALS: &[&str] = &[
    "idna", "label too long", "name or service not known",
    "temporary failure in name resolution", "nodename nor servname provided",
    "failed to resolve", "no address associated with hostname",
];

// ── Signal → DefectType mapping ──

/// Unified mapping from a defect signal tag to its `DefectType`.
/// Both `detect_defect_type` and `analyze_execution_result` use this single source of truth.
fn defect_signal_to_type(signal: &str) -> Option<DefectType> {
    match signal {
        "illegal_success" => Some(DefectType::IllegalSuccess),
        "param_ignored" => Some(DefectType::ParamIgnored),
        "poor_diagnostics" => Some(DefectType::PoorDiagnostics),
        "data_corruption" => Some(DefectType::DataCorruption),
        "performance_regression" => Some(DefectType::PerformanceRegression),
        "state_logic_violation" => Some(DefectType::StateLogicViolation),
        "silent_failure" => Some(DefectType::StateLogicViolation),
        "async_inconsistency" => Some(DefectType::PoorDiagnostics),
        "metamorphic_violation" => Some(DefectType::MetamorphicViolation),
        "differential_mismatch" => Some(DefectType::DifferentialMismatch),
        "sequence_violation" => Some(DefectType::SequenceViolation),
        _ => None,
    }
}

// ── Public API ──

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

    let issue_key = CANONICAL_KEYWORDS
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
    IllegalSuccess,
    ParamIgnored,
    PoorDiagnostics,
    RuntimeFailure,
    StateLogicViolation,
    DataCorruption,
    PerformanceRegression,
    MetamorphicViolation,
    DifferentialMismatch,
    SequenceViolation,
    ScriptError,
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
    pub sub_type: Option<String>,
}

pub fn detect_defect_type(stdout: &str, stderr: &str) -> Option<DefectType> {
    let lower_stdout = stdout.to_lowercase();
    let lower_stderr = stderr.to_lowercase();
    let combined = format!("{}\n{}", lower_stdout, lower_stderr);

    // Check [defect: <type>] tags via the shared signal→type mapping.
    let known_signals = [
        "illegal_success", "param_ignored", "poor_diagnostics", "data_corruption",
        "performance_regression", "state_logic_violation", "silent_failure",
        "async_inconsistency", "metamorphic_violation", "differential_mismatch",
        "sequence_violation",
    ];
    for signal in &known_signals {
        let tag = format!("[defect: {signal}]");
        if combined.contains(&tag) {
            return defect_signal_to_type(signal);
        }
    }

    // Fallback: assertion messages in stderr.
    if lower_stderr.contains("assertionerror: illegal success") {
        return Some(DefectType::IllegalSuccess);
    }
    if lower_stderr.contains("assertionerror: poor diagnostics") {
        return Some(DefectType::PoorDiagnostics);
    }
    if lower_stderr.contains("assertionerror: state violation") {
        return Some(DefectType::StateLogicViolation);
    }
    if lower_stderr.contains("assertionerror") {
        return Some(DefectType::StateLogicViolation);
    }
    None
}

pub fn is_script_error(stdout: &str, stderr: &str) -> bool {
    let lower_stderr = stderr.to_lowercase();
    let lower_stdout = stdout.to_lowercase();
    SCRIPT_ERROR_SIGNALS.iter().any(|sig| lower_stderr.contains(sig))
        || lower_stdout.contains("[test_infra]")
}

/// Analyzes the stdout and stderr from the sandbox to classify the execution result.
pub fn analyze_execution_result(stdout: &str, stderr: &str) -> ClassificationResult {
    let lower_stderr = stderr.to_lowercase();
    let lower_stdout = stdout.to_lowercase();
    let combined_output = format!("{}\n{}", lower_stdout, lower_stderr);

    if INFRA_ERROR_SIGNALS.iter().any(|sig| combined_output.contains(sig)) {
        return ClassificationResult {
            disposition: ClassificationDisposition::NonDefectInfraError,
            defect_type: None,
            reason: "Execution failed before a valid database interaction due to addressing or environment issues.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
            sub_type: None,
        };
    }

    if combined_output.contains("[defect: permissive_parsing]") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CoverageDetected,
            defect_type: Some(DefectType::PoorDiagnostics),
            reason: "Server accepted request with unknown/extra parameters (permissive parsing).".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
            sub_type: Some("permissive_parsing".to_string()),
        };
    }
    if combined_output.contains("[defect: idempotent_success]") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CoverageDetected,
            defect_type: Some(DefectType::PoorDiagnostics),
            reason: "Server returned success for idempotent operation on nonexistent resource.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
            sub_type: Some("idempotent_success".to_string()),
        };
    }

    if let Some(defect_type) = detect_defect_type(stdout, stderr) {
        let sub_type = if defect_type == DefectType::StateLogicViolation && lower_stdout.contains("cross_step") {
            Some("cross_step".to_string())
        } else {
            None
        };
        return ClassificationResult {
            disposition: ClassificationDisposition::CandidateDefect,
            defect_type: Some(defect_type),
            reason: "Observed explicit defect marker.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
            sub_type,
        };
    }

    if is_script_error(stdout, stderr) {
        return ClassificationResult {
            disposition: ClassificationDisposition::RetryableScriptError,
            defect_type: Some(DefectType::ScriptError),
            reason: "Generated script failed due to Python/runtime authoring errors or test-infrastructure uncertainty.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
            sub_type: None,
        };
    }

    if lower_stderr.contains("connection refused")
        || lower_stderr.contains("500 internal server error")
        || lower_stderr.contains("segmentation fault")
        || lower_stdout.contains("panic") {
        return ClassificationResult {
            disposition: ClassificationDisposition::CandidateDefect,
            defect_type: Some(DefectType::RuntimeFailure),
            reason: "Observed a runtime failure after reaching the execution target.".to_string(),
            evidence_excerpt: combined_output.chars().take(300).collect(),
            sub_type: None,
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
            sub_type: None,
        };
    }

    if !stderr.trim().is_empty() {
        return ClassificationResult {
            disposition: ClassificationDisposition::Pass,
            defect_type: Some(DefectType::Pass),
            reason: "Execution completed without defect markers. (Non-error stderr output present but no recognized defect or error pattern.)".to_string(),
            evidence_excerpt: stdout.chars().take(300).collect(),
            sub_type: None,
        };
    }

    ClassificationResult {
        disposition: ClassificationDisposition::Pass,
        defect_type: Some(DefectType::Pass),
        reason: "Execution completed without defect markers.".to_string(),
        evidence_excerpt: stdout.chars().take(300).collect(),
        sub_type: None,
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

    #[test]
    fn test_metamorphic_violation() {
        assert_eq!(
            detect_defect_type("some [DEFECT: METAMORPHIC_VIOLATION] text", ""),
            Some(DefectType::MetamorphicViolation)
        );
        let result = analyze_execution_result("[DEFECT: METAMORPHIC_VIOLATION] nprobe subset failed", "");
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert_eq!(result.defect_type, Some(DefectType::MetamorphicViolation));
    }

    #[test]
    fn test_differential_mismatch() {
        assert_eq!(
            detect_defect_type("some [DEFECT: DIFFERENTIAL_MISMATCH] text", ""),
            Some(DefectType::DifferentialMismatch)
        );
        let result = analyze_execution_result("[DEFECT: DIFFERENTIAL_MISMATCH] REST vs SDK", "");
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert_eq!(result.defect_type, Some(DefectType::DifferentialMismatch));
    }

    #[test]
    fn test_sequence_violation() {
        assert_eq!(
            detect_defect_type("some [DEFECT: SEQUENCE_VIOLATION] text", ""),
            Some(DefectType::SequenceViolation)
        );
        let result = analyze_execution_result("[DEFECT: SEQUENCE_VIOLATION] delete-then-search", "");
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert_eq!(result.defect_type, Some(DefectType::SequenceViolation));
    }

    #[test]
    fn test_new_defect_types_from_stderr() {
        assert_eq!(
            detect_defect_type("", "error: [DEFECT: METAMORPHIC_VIOLATION] detected"),
            Some(DefectType::MetamorphicViolation)
        );
        assert_eq!(
            detect_defect_type("", "[DEFECT: DIFFERENTIAL_MISMATCH] mismatch"),
            Some(DefectType::DifferentialMismatch)
        );
        assert_eq!(
            detect_defect_type("", "[DEFECT: SEQUENCE_VIOLATION] bad order"),
            Some(DefectType::SequenceViolation)
        );
    }

    #[test]
    fn test_permissive_parsing_not_candidate_defect() {
        let result = analyze_execution_result("[DEFECT: PERMISSIVE_PARSING] unknown_param accepted", "");
        assert_eq!(result.disposition, ClassificationDisposition::CoverageDetected);
        assert_eq!(result.defect_type, Some(DefectType::PoorDiagnostics));
        assert_eq!(result.sub_type, Some("permissive_parsing".to_string()));
    }

    #[test]
    fn test_idempotent_success_not_candidate_defect() {
        let result = analyze_execution_result("[DEFECT: IDEMPOTENT_SUCCESS] drop nonexistent accepted", "");
        assert_eq!(result.disposition, ClassificationDisposition::CoverageDetected);
        assert_eq!(result.defect_type, Some(DefectType::PoorDiagnostics));
        assert_eq!(result.sub_type, Some("idempotent_success".to_string()));
    }

    #[test]
    fn dedupes_same_issue_with_different_wording() {
        let output = "\
[DEFECT: POOR_DIAGNOSTICS] Error message does not mention limit must be > 0\n\
[DEFECT: POOR_DIAGNOSTICS] Limit constraint is missing from the error text\n\
[DEFECT: POOR_DIAGNOSTICS] Error message does not mention offset must be >= 0";
        let normalized = normalize_observed_output(output);
        let lines: Vec<&str> = normalized.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("limit"));
        assert!(lines[1].contains("offset"));
    }
}
