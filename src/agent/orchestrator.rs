use crate::agent::classifier::ClassificationDisposition;
use crate::agent::executor::FAExecutor;
use crate::agent::llm::{DeepSeekClient, Message};
use crate::agent::oracle::{build_oracle_findings_message, Oracle};
use crate::agent::tools::{get_execute_test_script_tool, get_submit_mre_tool};
use crate::contract::schema::StructuredContract;
use crate::report::generator;
use crate::target::TargetPlugin;
use tracing::{info, warn};

fn build_system_prompt(contract_content: &str) -> String {
    format!(
        "You are an expert security researcher performing Agentic Fuzzing.\n\
        Your goal is to find REAL vulnerabilities where the server silently accepts invalid input.\n\
        You must use the `execute_test_script` tool to test your code in an isolated sandbox.\n\
        \n\
        TEST TEMPLATES (choose based on the constraint type):\n\
        \n\
        TYPE VIOLATION: Send wrong JSON type for a parameter.\n\
        Example: limit expects integer → send string 'abc' or float 3.14.\n\
        Print [DEFECT: TYPE_VIOLATION] if the server accepts the wrong type.\n\
        \n\
        RANGE VIOLATION: Send correct type but out-of-range value.\n\
        Example: limit=0 (should be >0), offset=-1, hnsw_ef=0 (should be >=1).\n\
        Test boundary values: min-1, min, max, max+1.\n\
        Print [DEFECT: RANGE_VIOLATION] or [DEFECT: ILLEGAL_SUCCESS] if the server silently accepts.\n\
        \n\
        STATE VIOLATION: Violate preconditions before calling the endpoint.\n\
        Example: search on non-existent collection, upsert without creating collection first.\n\
        Print [DEFECT: STATE_VIOLATION] if the server returns 200 or unclear error.\n\
        \n\
        COMBINATION VIOLATION: Combine multiple boundary values in one request.\n\
        Some servers validate each field independently but miss interactions.\n\
        Print [DEFECT: ILLEGAL_SUCCESS] if the server accepts the combination.\n\
        \n\
        ZERO-VALUE PROBE: Test parameters with value 0 where positive is expected.\n\
        Many APIs forget to validate that certain uint fields must be >= 1.\n\
        Print [DEFECT: ILLEGAL_SUCCESS] if the server accepts 0 for a must-be-positive field.\n\
        \n\
        CRITICAL RULES:\n\
        1. You MUST test at least 3 DIFFERENT assertions from the contract before calling submit_mre.\n\
        2. Each test script should target ONE assertion and print the appropriate [DEFECT: ...] marker.\n\
        3. When you find a potential defect, FOCUS on it: refine until it cleanly demonstrates the violation.\n\
        4. The script you submit MUST print a [DEFECT: ...] marker — otherwise worthless.\n\
        5. If the server correctly REJECTS (400/422), that is normal — move on to a different parameter or approach.\n\
        6. After testing 3+ assertions with at least one confirmed defect, call submit_mre.\n\
        7. TEST RESULT REPORTING: If you test 3+ assertions and ALL are correctly rejected (no defects), print coverage markers and call submit_mre:\n\
           [COVERAGE:TYPE] N type assertions tested\n\
           [COVERAGE:RANGE] M range assertions tested\n\
           [COVERAGE:STATE] K state assertions tested\n\
           Then call submit_mre with that coverage script. This is a legitimate result — not a failure.\n\
        8. ADAPTIVE STRATEGY: You will receive an exploration state summary each turn. Use it to:\n\
           - Identify which parameters have NOT been tested yet\n\
           - Notice patterns: if many parameters are correctly rejected, try a different attack approach\n\
           - If consecutive tests find no defect, consider switching to a completely different strategy\n\
        \n\
        Contract:\n{}\n",
        contract_content
    )
}

fn build_state_message(state_json: &str) -> String {
    format!(
        "=== EXPLORATION STATE ===\n{}\n=== END STATE ===\n\n\
        Based on the exploration state above, decide your next action. \
        Focus on untested parameters or try a different approach if recent tests found no defects.",
        state_json
    )
}

pub struct FAOrchestrator<'a> {
    llm_client: &'a DeepSeekClient,
    plugin: &'a dyn TargetPlugin,
    contract_content: String,
    contract: StructuredContract,
    db_image: String,
    pip_packages: Vec<String>,
    db_port: u16,
    max_turns: usize,
}

impl<'a> FAOrchestrator<'a> {
    pub fn new(
        llm_client: &'a DeepSeekClient,
        plugin: &'a dyn TargetPlugin,
        contract_content: String,
        version: &str,
        max_turns: usize,
    ) -> Self {
        let db_image = plugin.target_image(version);
        let pip_packages = plugin.pip_packages();
        let db_port = plugin.db_port();
        let contract: StructuredContract = serde_json::from_str(&contract_content)
            .unwrap_or(StructuredContract {
                api_endpoint: String::new(),
                doc_url: String::new(),
                assertions: Vec::new(),
                type_constraints: Vec::new(),
                range_constraints: Vec::new(),
                state_constraints: Vec::new(),
                state_invariants: Vec::new(),
            });
        FAOrchestrator {
            llm_client,
            plugin,
            contract_content,
            contract,
            db_image,
            pip_packages,
            db_port,
            max_turns,
        }
    }

    pub async fn run(
        &self,
    ) -> anyhow::Result<(
        String,
        generator::RunEvidence,
        crate::agent::classifier::ClassificationResult,
    )> {
        let tools = vec![get_execute_test_script_tool(), get_submit_mre_tool()];
        let system_prompt = build_system_prompt(&self.contract_content);

        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(
                "Begin exploration. Write a script and use execute_test_script to test it.",
            ),
        ];

        let mut executor = FAExecutor::new();
        let oracle_checks = self.plugin.derive_oracle_checks(&self.contract);
        let mut oracle = Oracle::new(oracle_checks);
        let oracle_batch_size = 2;

        for turn in 0..self.max_turns {
            info!(
                "Agentic Exploration Turn {}/{}",
                turn + 1,
                self.max_turns
            );

            if turn > 0 {
                let state_json = executor.state.to_prompt_json();
                let state_msg = build_state_message(&state_json);
                messages.push(Message::user(state_msg));
            }

            if turn == 8 && executor.unique_assertions_count() < 2 {
                messages.push(Message::user(format!(
                    "[URGENT] You only tested {} assertion(s). Test more parameters from the contract.",
                    executor.unique_assertions_count()
                )));
            } else if turn == 8 {
                messages.push(Message::user(format!(
                    "[URGENT] Tested {} assertions. If you found ANY defect, FOCUS on it and prepare a script with [DEFECT: ...] marker. Then call submit_mre.",
                    executor.unique_assertions_count()
                )));
            }

            if turn == 10 {
                messages.push(Message::user(format!(
                    "[B2 SYSTEM] You have tested {} assertions. This is your last test turn.\n\
                    After this turn, the system will automatically submit your last test script as the MRE.\n\
                    Make sure your last script prints [DEFECT: ...] if a defect is found.",
                    executor.unique_assertions_count()
                )));
            }

            if turn == self.max_turns - 1 {
                messages.push(Message::user(format!(
                    "[FINAL TURN] You tested {} assertions. This is your last turn.\n\
                    If you found a defect, call submit_mre. If not, your last test script will be auto-submitted.",
                    executor.unique_assertions_count()
                )));
            }

            let response_msg = self
                .llm_client
                .send_chat_with_tools(messages.clone(), tools.clone())
                .await?;
            messages.push(response_msg.clone());

            let Some(tool_calls) = response_msg.tool_calls else {
                continue;
            };
            if tool_calls.is_empty() {
                continue;
            }
            if tool_calls.len() > 1 {
                warn!("Agent attempted parallel tool calls. Rejecting.");
                for tc in tool_calls {
                    messages.push(Message::tool_response(
                        &tc.id,
                        "Error: Parallel tool calls are not supported. Please call one tool at a time.",
                    ));
                }
                continue;
            }

            let tc = &tool_calls[0];
            match tc.function.name.as_str() {
                "execute_test_script" => {
                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");

                    let result = executor
                        .execute_test(code, &self.db_image, &self.pip_packages, self.db_port)
                        .await?;

                    messages.push(Message::tool_response(&tc.id, &result.output));

                    if oracle.has_pending() {
                        if let Some(ref sandbox) = result.sandbox {
                            if !result.db_url.is_empty() {
                                info!("Running Oracle checks in same sandbox (batch of {})...", oracle_batch_size);
                                let oracle_findings = oracle
                                    .run_next_batch(sandbox, &result.db_url, oracle_batch_size)
                                    .await;
                                if !oracle_findings.is_empty() {
                                    let violated_count = oracle_findings.iter().filter(|f| f.violated).count();
                                    if violated_count > 0 {
                                        info!("Oracle found {} violation(s)!", violated_count);
                                    }
                                    executor.state.record_oracle_findings(oracle_findings.clone());
                                    let oracle_msg = build_oracle_findings_message(&oracle_findings);
                                    if !oracle_msg.is_empty() {
                                        messages.push(Message::user(oracle_msg));
                                    }
                                }
                            }
                        }
                    }

                    if executor.error_state.should_intervene() {
                        warn!(
                            "Agent hit the same error {} times. Injecting SYSTEM INTERVENTION.",
                            executor.error_state.consecutive_same_errors
                        );
                        messages.push(Message::user(
                            "[SYSTEM INTERVENTION] You have failed with similar errors 3 times. You must change your approach entirely or stop.",
                        ));
                        executor.error_state.reset();
                    }
                }
                "submit_mre" => {
                    let oracle_violations = executor.state.oracle_violations();
                    let total_assertions = executor.unique_assertions_count() + oracle_violations.len();
                    if total_assertions < 3 {
                        messages.push(Message::tool_response(
                            &tc.id,
                            format!(
                                "REJECTED: You have only tested {} assertion(s) (FA: {}, Oracle: {}). You MUST test at least 3 DIFFERENT assertions before submitting. \
                                Review the contract and test untested parameters.",
                                total_assertions,
                                executor.unique_assertions_count(),
                                oracle_violations.len()
                            ),
                        ));
                        continue;
                    }

                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let code = args
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    info!("Agent submitted MRE. Running final validation...");
                    let result = executor
                        .execute_test(&code, &self.db_image, &self.pip_packages, self.db_port)
                        .await?;

                    let classification = result.classification.clone();

                    if classification.disposition == ClassificationDisposition::Pass
                        || classification.disposition == ClassificationDisposition::CoverageDetected
                    {
                        let oracle_violations = executor.state.oracle_violations();
                        if !oracle_violations.is_empty() {
                            info!(
                                "FA found no defect, but Oracle found {} violation(s). Using Oracle findings.",
                                oracle_violations.len()
                            );
                            let first_violation = &oracle_violations[0];
                            let oracle_defect_type = first_violation.defect_type.clone()
                                .unwrap_or(crate::agent::classifier::DefectType::StateLogicViolation);
                            let oracle_classification = crate::agent::classifier::ClassificationResult {
                                disposition: ClassificationDisposition::CandidateDefect,
                                defect_type: Some(oracle_defect_type),
                                reason: format!("Oracle detected: {}", first_violation.evidence),
                                evidence_excerpt: first_violation.evidence.chars().take(300).collect(),
                            };
                            let initial_run = generator::RunEvidence {
                                phase: "oracle".to_string(),
                                db_url: result.db_url.clone(),
                                stdout: String::new(),
                                stderr: String::new(),
                                classifier_reason: oracle_classification.reason.clone(),
                                classifier_evidence_excerpt: oracle_classification.evidence_excerpt.clone(),
                            };
                            return Ok((code, initial_run, oracle_classification));
                        }

                        warn!(
                            "FA found no defect ({}). Running safety net probes.",
                            if matches!(
                                classification.disposition,
                                ClassificationDisposition::CoverageDetected
                            ) {
                                "coverage report"
                            } else {
                                "non-defect MRE"
                            }
                        );
                        for net in self.plugin.safety_nets() {
                            info!("Safety net probe: {}", net.name);
                            if let Some(safety_result) = executor
                                .execute_safety_net(
                                    &net.name,
                                    &net.script,
                                    &self.db_image,
                                    &self.pip_packages,
                                    self.db_port,
                                )
                                .await?
                            {
                                let initial_run = generator::RunEvidence {
                                    phase: "initial".to_string(),
                                    db_url: safety_result.db_url.clone(),
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    classifier_reason: safety_result.classification.reason.clone(),
                                    classifier_evidence_excerpt: safety_result
                                        .classification
                                        .evidence_excerpt
                                        .clone(),
                                };
                                return Ok((net.script, initial_run, safety_result.classification));
                            }
                            warn!(
                                "Safety net '{}' did not trigger (properly rejected).",
                                net.name
                            );
                        }
                        warn!("All safety nets passed. No defect found.");
                    }

                    let initial_run = generator::RunEvidence {
                        phase: "initial".to_string(),
                        db_url: result.db_url.clone(),
                        stdout: String::new(),
                        stderr: String::new(),
                        classifier_reason: classification.reason.clone(),
                        classifier_evidence_excerpt: classification.evidence_excerpt.clone(),
                    };
                    return Ok((code, initial_run, classification));
                }
                _ => {
                    messages.push(Message::tool_response(&tc.id, "Unknown tool."));
                }
            }
        }

        if let Some(code) = executor.last_test_code.clone() {
            warn!("Agentic exploration exceeded max turns. B2: submitting last test script as MRE.");
            let result = executor
                .execute_test(&code, &self.db_image, &self.pip_packages, self.db_port)
                .await?;
            let initial_run = generator::RunEvidence {
                phase: "initial".to_string(),
                db_url: result.db_url.clone(),
                stdout: String::new(),
                stderr: String::new(),
                classifier_reason: result.classification.reason.clone(),
                classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
            };
            return Ok((code, initial_run, result.classification));
        }

        warn!("No FA test scripts executed. Running safety net probes.");
        for net in self.plugin.safety_nets() {
            info!("Safety net probe (fallback): {}", net.name);
            if let Some(safety_result) = executor
                .execute_safety_net(
                    &net.name,
                    &net.script,
                    &self.db_image,
                    &self.pip_packages,
                    self.db_port,
                )
                .await?
            {
                let initial_run = generator::RunEvidence {
                    phase: "initial".to_string(),
                    db_url: safety_result.db_url.clone(),
                    stdout: String::new(),
                    stderr: String::new(),
                    classifier_reason: safety_result.classification.reason.clone(),
                    classifier_evidence_excerpt: safety_result.classification.evidence_excerpt.clone(),
                };
                return Ok((net.script, initial_run, safety_result.classification));
            }
            warn!(
                "Safety net '{}' did not trigger (properly rejected).",
                net.name
            );
        }
        anyhow::bail!("No defect found by FA or any safety net probe");
    }
}
