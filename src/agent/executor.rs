use crate::agent::classifier::{analyze_execution_result, ClassificationDisposition};
use crate::agent::state::{ExplorationState, TestResult};
use crate::agent::tools::{execute_test_script, execute_test_in_sandbox};
use crate::sandbox::manager::Sandbox;
use regex::Regex;
use std::collections::HashSet;
use tracing::info;

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
    "count",
    "upsert",
    "delete",
    "scroll",
    "recommend",
];

pub struct FAExecutor {
    pub state: ExplorationState,
    pub error_state: ErrorStateMachine,
    pub last_test_code: Option<String>,
    assertions_tested: HashSet<String>,
    active_sandbox: Option<Sandbox>,
    active_db_url: Option<String>,
}

impl FAExecutor {
    pub fn new() -> Self {
        FAExecutor {
            state: ExplorationState::new(),
            error_state: ErrorStateMachine::new(),
            last_test_code: None,
            assertions_tested: HashSet::new(),
            active_sandbox: None,
            active_db_url: None,
        }
    }

    pub async fn execute_test(
        &mut self,
        code: &str,
        fresh_sandbox: bool,
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

        if !fresh_sandbox {
            if self.active_sandbox.is_some() {
                info!("Reusing existing sandbox (fresh_sandbox=false)...");
                let sandbox = self.active_sandbox.take().unwrap();
                let result = self.execute_in_existing_sandbox_internal(code, &sandbox, db_port).await;
                if result.is_ok() {
                    self.active_sandbox = Some(sandbox);
                }
                return result;
            } else {
                info!("No existing sandbox to reuse. Creating fresh sandbox...");
            }
        }

        info!("Creating fresh sandbox (fresh_sandbox={})...", fresh_sandbox);
        match execute_test_script(code, db_image, pip_packages, db_port).await {
            Ok((output, sandbox, db_url)) => {
                self.active_sandbox = Some(sandbox);
                self.active_db_url = Some(db_url.clone());
                let mut result = self.process_result(output, db_url)?;
                result.sandbox = self.active_sandbox.take();
                info!("Sandbox placed in ExecutionResult, active_sandbox now={}", self.active_sandbox.is_some());
                Ok(result)
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
                        sub_type: None,
                    },
                    sandbox: None,
                })
            }
        }
    }

    async fn execute_in_existing_sandbox_internal(
        &mut self,
        code: &str,
        sandbox: &Sandbox,
        db_port: u16,
    ) -> anyhow::Result<ExecutionResult> {
        match execute_test_in_sandbox(code, sandbox, db_port).await {
            Ok((output, db_url)) => {
                self.active_db_url = Some(db_url.clone());
                let mut result = self.process_result(output, db_url);
                if let Ok(ref mut r) = result {
                    r.sandbox = None;
                }
                result
            }
            Err(e) => {
                self.state.record_script_error();
                Ok(ExecutionResult {
                    output: format!("Sandbox reuse execution failed: {}", e),
                    db_url: self.active_db_url.clone().unwrap_or_default(),
                    found_defect: false,
                    classification: crate::agent::classifier::ClassificationResult {
                        disposition: ClassificationDisposition::RetryableScriptError,
                        defect_type: None,
                        reason: format!("Sandbox reuse execution failed: {}", e),
                        evidence_excerpt: String::new(),
                        sub_type: None,
                    },
                    sandbox: None,
                })
            }
        }
    }

    fn process_result(
        &mut self,
        output: String,
        db_url: String,
    ) -> anyhow::Result<ExecutionResult> {
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
            sandbox: None,
        })
    }

    pub fn has_active_sandbox(&self) -> bool {
        self.active_sandbox.is_some()
    }

    pub fn unique_assertions_count(&self) -> usize {
        self.assertions_tested.len()
    }

    pub fn take_sandbox(&mut self) -> Option<Sandbox> {
        self.active_sandbox.take()
    }

    pub fn put_sandbox(&mut self, sandbox: Sandbox) {
        self.active_sandbox = Some(sandbox);
    }
}
