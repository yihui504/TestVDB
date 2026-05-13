use crate::agent::classifier::{analyze_execution_result, ClassificationDisposition};
use crate::agent::state::{ExplorationState, TestResult};
use crate::agent::tools::execute_test_script;
use crate::sandbox::manager::Sandbox;
use regex::Regex;
use std::collections::HashSet;

pub struct ExecutionResult {
    pub output: String,
    pub db_url: String,
    pub found_defect: bool,
    pub classification: crate::agent::classifier::ClassificationResult,
    pub sandbox: Option<Sandbox>,
}

pub struct ErrorStateMachine {
    pub consecutive_same_errors: usize,
    last_error_type: String,
    error_regex: Regex,
}

impl ErrorStateMachine {
    pub fn new() -> Self {
        ErrorStateMachine {
            consecutive_same_errors: 0,
            last_error_type: String::new(),
            error_regex: Regex::new(r"([A-Z][a-zA-Z0-9]+Error):").unwrap(),
        }
    }

    pub fn update(&mut self, output: &str) {
        if output.contains("STDERR:\n") {
            let stderr = output.split("STDERR:\n").last().unwrap_or("");
            if let Some(captures) = self.error_regex.captures(stderr) {
                let error_type = captures.get(1).unwrap().as_str().to_string();
                if error_type == self.last_error_type {
                    self.consecutive_same_errors += 1;
                } else {
                    self.consecutive_same_errors = 1;
                    self.last_error_type = error_type;
                }
            } else {
                self.consecutive_same_errors = 0;
                self.last_error_type.clear();
            }
        }
    }

    pub fn should_intervene(&self) -> bool {
        self.consecutive_same_errors >= 3
    }

    pub fn reset(&mut self) {
        self.consecutive_same_errors = 0;
        self.last_error_type.clear();
    }
}

const ASSERTION_KEYWORDS: &[&str] = &[
    "limit",
    "offset",
    "hnsw_ef",
    "vector",
    "score_threshold",
    "exact",
    "NaN",
    "Infinity",
    "collection_name",
    "size",
    "distance",
    "shard",
    "replication_factor",
    "write_consistency_factor",
    "oversampling",
    "ef_construct",
    "quantization",
];

pub struct FAExecutor {
    pub state: ExplorationState,
    pub error_state: ErrorStateMachine,
    pub last_test_code: Option<String>,
    assertions_tested: HashSet<String>,
}

impl FAExecutor {
    pub fn new() -> Self {
        FAExecutor {
            state: ExplorationState::new(),
            error_state: ErrorStateMachine::new(),
            last_test_code: None,
            assertions_tested: HashSet::new(),
        }
    }

    pub async fn execute_test(
        &mut self,
        code: &str,
        db_image: &str,
        pip_packages: &[String],
        db_port: u16,
    ) -> anyhow::Result<ExecutionResult> {
        self.last_test_code = Some(code.to_string());

        for keyword in ASSERTION_KEYWORDS {
            if code.to_lowercase().contains(&keyword.to_lowercase()) {
                self.assertions_tested.insert(keyword.to_string());
            }
        }

        match execute_test_script(code, db_image, pip_packages, db_port).await {
            Ok((output, sandbox, db_url)) => {
                let found_defect = output.contains("[DEFECT:")
                    || output.contains("ILLEGAL_SUCCESS")
                    || output.contains("RANGE_VIOLATION")
                    || output.contains("TYPE_VIOLATION")
                    || output.contains("STATE_VIOLATION");

                let normalized_stdout = output
                    .split("STDERR:\n")
                    .next()
                    .unwrap_or(&output)
                    .replace("STDOUT:\n", "")
                    .trim()
                    .to_string();
                let normalized_stderr = output
                    .split("STDERR:\n")
                    .last()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let classification =
                    analyze_execution_result(&normalized_stdout, &normalized_stderr);

                self.error_state.update(&output);

                if found_defect {
                    self.state.record_test(
                        "last_test",
                        "unknown",
                        TestResult::Defect,
                        classification.defect_type.clone(),
                    );
                } else if classification.disposition
                    == ClassificationDisposition::RetryableScriptError
                {
                    self.state.record_script_error();
                } else {
                    self.state.record_test(
                        "last_test",
                        "unknown",
                        TestResult::Rejected,
                        None,
                    );
                }

                Ok(ExecutionResult {
                    output,
                    db_url,
                    found_defect,
                    classification,
                    sandbox: Some(sandbox),
                })
            }
            Err(e) => {
                self.state.record_script_error();
                Ok(ExecutionResult {
                    output: format!("Sandbox execution failed: {}", e),
                    db_url: String::new(),
                    found_defect: false,
                    classification: crate::agent::classifier::ClassificationResult {
                        disposition: ClassificationDisposition::RetryableScriptError,
                        defect_type: None,
                        reason: format!("Sandbox execution failed: {}", e),
                        evidence_excerpt: String::new(),
                    },
                    sandbox: None,
                })
            }
        }
    }

    pub async fn execute_safety_net(
        &mut self,
        _net_name: &str,
        net_script: &str,
        db_image: &str,
        pip_packages: &[String],
        db_port: u16,
    ) -> anyhow::Result<Option<ExecutionResult>> {
        match execute_test_script(net_script, db_image, pip_packages, db_port).await {
            Ok((output, _sandbox, db_url)) => {
                let normalized_stdout = output
                    .split("STDERR:\n")
                    .next()
                    .unwrap_or(&output)
                    .replace("STDOUT:\n", "")
                    .trim()
                    .to_string();
                let normalized_stderr = output
                    .split("STDERR:\n")
                    .last()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let classification =
                    analyze_execution_result(&normalized_stdout, &normalized_stderr);

                if classification.disposition != ClassificationDisposition::Pass
                    && classification.disposition != ClassificationDisposition::CoverageDetected
                {
                    Ok(Some(ExecutionResult {
                        output,
                        db_url,
                        found_defect: true,
                        classification,
                        sandbox: None,
                    }))
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    pub fn unique_assertions_count(&self) -> usize {
        self.assertions_tested.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_state_machine_same_error() {
        let mut esm = ErrorStateMachine::new();
        esm.update("STDOUT:\nok\nSTDERR:\nSyntaxError: invalid syntax");
        assert_eq!(esm.consecutive_same_errors, 1);
        esm.update("STDOUT:\nok\nSTDERR:\nSyntaxError: another error");
        assert_eq!(esm.consecutive_same_errors, 2);
        assert!(!esm.should_intervene());
        esm.update("STDOUT:\nok\nSTDERR:\nSyntaxError: third");
        assert_eq!(esm.consecutive_same_errors, 3);
        assert!(esm.should_intervene());
    }

    #[test]
    fn test_error_state_machine_different_errors() {
        let mut esm = ErrorStateMachine::new();
        esm.update("STDOUT:\nok\nSTDERR:\nSyntaxError: invalid");
        assert_eq!(esm.consecutive_same_errors, 1);
        esm.update("STDOUT:\nok\nSTDERR:\nImportError: no module");
        assert_eq!(esm.consecutive_same_errors, 1);
        assert!(!esm.should_intervene());
    }

    #[test]
    fn test_error_state_machine_reset() {
        let mut esm = ErrorStateMachine::new();
        esm.update("STDOUT:\nok\nSTDERR:\nSyntaxError: x");
        esm.update("STDOUT:\nok\nSTDERR:\nSyntaxError: y");
        esm.update("STDOUT:\nok\nSTDERR:\nSyntaxError: z");
        assert!(esm.should_intervene());
        esm.reset();
        assert_eq!(esm.consecutive_same_errors, 0);
        assert!(!esm.should_intervene());
    }

    #[test]
    fn test_error_state_machine_no_stderr() {
        let mut esm = ErrorStateMachine::new();
        esm.update("STDOUT:\nall good");
        assert_eq!(esm.consecutive_same_errors, 0);
    }

    #[test]
    fn test_assertion_tracking() {
        let mut exec = FAExecutor::new();
        assert_eq!(exec.unique_assertions_count(), 0);
        for keyword in ASSERTION_KEYWORDS {
            if "limit" == *keyword {
                let code = format!("test with {} = 0", keyword);
                for kw in ASSERTION_KEYWORDS {
                    if code.to_lowercase().contains(&kw.to_lowercase()) {
                        exec.assertions_tested.insert(kw.to_string());
                    }
                }
                break;
            }
        }
        assert!(exec.assertions_tested.contains("limit"));
        assert_eq!(exec.unique_assertions_count(), 1);
    }
}
