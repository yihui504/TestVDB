use crate::agent::classifier::ClassificationDisposition;
use crate::agent::executor::FAExecutor;
use crate::agent::llm::{DeepSeekClient, Message};
use crate::agent::oracle::{build_oracle_findings_message, Oracle};
use crate::agent::tools::{get_execute_test_script_tool, get_submit_mre_tool, get_fuzz_boundary_values_tool, get_coverage_report_tool, get_fuzz_api_sequence_tool};
use crate::agent::vdbfuzz::boundary::BoundaryValueGenerator;
use crate::agent::vdbfuzz::coverage::{CoverageTracker, ApiEndpoint};
use crate::agent::vdbfuzz::sequence::APISequenceExplorer;
use crate::contract::schema::StructuredContract;
use crate::report::generator;
use crate::target::TargetPlugin;
use tracing::{info, warn};

fn build_system_prompt(contract_content: &str) -> String {
    format!(
        "You are a security researcher performing Agentic Fuzzing. Find REAL defects where the server violates contracts or silently accepts invalid input.\n\
        \n\
        === TOOLS ===\n\
        `execute_test_script(code, fresh_sandbox?)` — Run Python scripts. Auto-reuses DB across calls. Set fresh_sandbox=true ONLY for clean start.\n\
        `fuzz_boundary_values(focus_params?)` — Auto-generate boundary value tests from contract constraints.\n\
        `fuzz_api_sequence(sequence_type?)` — Auto-generate multi-step API sequence tests.\n\
        `get_coverage_report()` — Show tested vs untested parameters.\n\
        \n\
        === MANDATORY RULES ===\n\
        1. DO NOT submit MRE before turn 5. You MUST explore at least 5 turns first.\n\
        2. DO NOT repeat the same test pattern. Each turn MUST test a DIFFERENT parameter or endpoint.\n\
        3. If AUTO-GENERATED scripts are provided in the context, you MUST execute at least ONE of them before writing your own.\n\
        4. You MUST test at least 3 DIFFERENT parameters before submitting.\n\
        5. After finding a defect, test 2 MORE parameters to see if the same class of defect exists elsewhere.\n\
        \n\
        === EXPLORATION STRATEGY ===\n\
        Turn 1-2: Execute auto-generated boundary/sequence tests from the context above.\n\
        Turn 3-4: Test STATE consistency (upsert N -> count=N, delete K -> count=N-K).\n\
        Turn 5-6: Test DATA integrity (write -> read back -> verify match) and ASYNC behavior (wait=true vs wait=false).\n\
        Turn 7+: Test CROSS-STEP lifecycle (create -> delete -> recreate) and explore untested parameters from coverage report.\n\
        \n\
        === DEFECT TYPES TO LOOK FOR ===\n\
        - ILLEGAL_SUCCESS: Server accepts input that should be rejected (e.g., negative values, zero, out-of-range)\n\
        - POOR_DIAGNOSTICS: Server returns 200 but silently discards data (test wait=true vs wait=false)\n\
        - STATE_VIOLATION: Count mismatch, data inconsistency after operations\n\
        - DATA_CORRUPTION: Write vector -> read back -> values don't match\n\
        \n\
        === SCRIPT RULES ===\n\
        - Use {{{{TESTVDB_DB_URL}}}} as DB URL placeholder\n\
        - time.sleep(0.5) after create, 0.3 after upsert\n\
        - Print [DEFECT: ILLEGAL_SUCCESS|STATE_LOGIC_VIOLATION|DATA_CORRUPTION|POOR_DIAGNOSTICS] on defect\n\
        - sys.exit(1) on defect, sys.exit(0) on pass\n\
        - Unique collection name with uuid\n\
        - Submit with submit_mre when >= 3 surviving assertions found\n\
        \n\
        Contract:\n{}\n",
        contract_content
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
    multi_defect: bool,
}

pub struct CollectedDefect {
    pub script: String,
    pub evidence: generator::RunEvidence,
    pub classification: crate::agent::classifier::ClassificationResult,
}

impl<'a> FAOrchestrator<'a> {
    pub fn new(
        llm_client: &'a DeepSeekClient,
        plugin: &'a dyn TargetPlugin,
        contract_content: String,
        version: &str,
        max_turns: usize,
        multi_defect: bool,
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
                behavioral_contracts: Vec::new(),
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
            multi_defect,
        }
    }

    fn build_behavioral_section(&self) -> String {
        if self.contract.behavioral_contracts.is_empty() {
            return String::new();
        }

        let mut section = String::from("=== SPECIFIC BEHAVIORAL CONTRACTS TO TEST ===\n");
        section.push_str("The following behavioral contracts are defined for this endpoint.\n");
        section.push_str("Use these as EXAMPLES to write your own multi-step test scripts.\n");
        section.push_str("Each contract has a verification_script you can adapt.\n");
        section.push_str("PRIORITY: Test these BEFORE doing simple input validation.\n\n");

        let max_templates = 10;
        for (i, bc) in self.contract.behavioral_contracts.iter().take(max_templates).enumerate() {
            section.push_str(&format!("{}. [{:?}] {}\n", i + 1, bc.category, bc.name));
            section.push_str(&format!("   Endpoints: {}\n", bc.endpoints.join(", ")));
            section.push_str(&format!("   Expected: {}\n", bc.expected_outcome));
            let script_preview = if bc.verification_script.len() > 800 {
                format!("{}...", &bc.verification_script[..800])
            } else {
                bc.verification_script.clone()
            };
            section.push_str(&format!("   Script template:\n   {}\n\n", script_preview.replace('\n', "\n   ")));
        }

        if self.contract.behavioral_contracts.len() > max_templates {
            section.push_str(&format!(
                "... and {} more behavioral contracts. Follow the same pattern for those.\n",
                self.contract.behavioral_contracts.len() - max_templates
            ));
        }

        section.push_str("\nIMPORTANT: These scripts use {{TESTVDB_DB_URL}} as the database URL placeholder.\n");
        section.push_str("Adapt them to your test scenario. You can modify the data, parameters, or assertions.\n");
        section.push_str("Write a COMPLETE self-contained script that does setup + action + verification in one call.\n");

        section
    }

    pub async fn run(
        &self,
    ) -> anyhow::Result<(
        String,
        generator::RunEvidence,
        crate::agent::classifier::ClassificationResult,
        Vec<CollectedDefect>,
    )> {
        let tools = vec![get_execute_test_script_tool(), get_submit_mre_tool(), get_fuzz_boundary_values_tool(), get_fuzz_api_sequence_tool(), get_coverage_report_tool()];
        let system_prompt = build_system_prompt(&self.contract_content);
        let mut coverage_tracker = CoverageTracker::new();

        let mut param_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for tc in &self.contract.type_constraints {
            param_names.insert(tc.param_name.clone());
        }
        for rc in &self.contract.range_constraints {
            param_names.insert(rc.param_name.clone());
        }
        if !param_names.is_empty() {
            let params: Vec<String> = param_names.into_iter().collect();
            coverage_tracker.register_endpoint(ApiEndpoint {
                path: self.contract.api_endpoint.clone(),
                method: "POST".to_string(),
                params,
            });
            info!("Coverage tracker: registered {} params for endpoint '{}'", coverage_tracker.endpoint_count(), self.contract.api_endpoint);
        }

        let mut fuzz_context = String::new();

        let boundary_cases = BoundaryValueGenerator::from_contract(&self.contract);
        let high_value_cases: Vec<_> = boundary_cases.iter().filter(|case| case.expected_rejection).take(5).collect();
        if !high_value_cases.is_empty() {
            for case in &high_value_cases {
                if let Some((ep, param, val)) = &case.coverage_entry {
                    coverage_tracker.record_visit(ep, param, val);
                }
            }
            fuzz_context.push_str("=== PRE-BUILT DEFECT HUNT SCRIPTS ===\n");
            fuzz_context.push_str("CRITICAL: These scripts test parameters that are LIKELY to have defects.\n");
            fuzz_context.push_str("You MUST execute at least ONE of these scripts in your FIRST turn.\n");
            fuzz_context.push_str("Copy the script exactly and call execute_test_script(code=<script>).\n\n");
            for (i, case) in high_value_cases.iter().enumerate() {
                fuzz_context.push_str(&format!("{}. {} (expected_rejection={})\n", i + 1, case.name, case.expected_rejection));
                fuzz_context.push_str(&format!("   Script:\n   {}\n\n", case.script.replace('\n', "\n   ")));
            }
        }

        let sequence_cases = APISequenceExplorer::generate_sequences();
        let high_value_seqs: Vec<_> = sequence_cases.iter().filter(|case| case.expected_defect.is_some()).take(3).collect();
        if !high_value_seqs.is_empty() {
            fuzz_context.push_str("\n=== PRE-BUILT API SEQUENCE TESTS ===\n");
            fuzz_context.push_str("These multi-step tests check for state consistency defects.\n");
            fuzz_context.push_str("Execute at least ONE after the boundary tests.\n\n");
            for (i, case) in high_value_seqs.iter().enumerate() {
                fuzz_context.push_str(&format!("{}. {} [{}] (expected: {:?})\n", i + 1, case.name, case.sequence_type, case.expected_defect));
                fuzz_context.push_str(&format!("   Script:\n   {}\n\n", case.script.replace('\n', "\n   ")));
            }
        }

        let initial_msg = if fuzz_context.is_empty() {
            "Begin exploration. Write a script and use execute_test_script(fresh_sandbox=true) to test it.".to_string()
        } else {
            format!(
                "START by executing one of the PRE-BUILT scripts below. Do NOT write your own script first.\n\
                 Step 1: Copy a PRE-BUILT script and call execute_test_script(code=<copied_script>)\n\
                 Step 2: If it finds a defect, test similar parameters to find more defects of the same class\n\
                 Step 3: Only write your own scripts AFTER you have exhausted the pre-built ones\n\n{}",
                fuzz_context
            )
        };

        let mut messages = vec![
            Message::system(system_prompt),
            Message::user(initial_msg),
        ];

        let mut executor = FAExecutor::new();
        let mut oracle_checks = self.plugin.derive_oracle_checks(&self.contract);
        let mutation_checks = Oracle::from_mutation_rules(
            &self.contract.behavioral_contracts.iter().flat_map(|c| c.mutation_rules.iter()).cloned().collect::<Vec<_>>(),
        );
        let mut_count = mutation_checks.len();
        oracle_checks.splice(0..0, mutation_checks);
        info!("Oracle initialized with {} checks ({} mutation rules prioritized first)", oracle_checks.len(), mut_count);
        let mut oracle = Oracle::new(oracle_checks);
        let oracle_batch_size = 6;
        let mut collected_defects: Vec<CollectedDefect> = Vec::new();

        let all_safety_nets: Vec<_> = self.plugin.safety_nets()
            .into_iter()
            .filter(|net| !net.redundant_with_mutation)
            .collect();
        let sn_total = all_safety_nets.len();
        let sn_batch_size = (sn_total / 3).max(1);
        let mut sn_next_idx = 0;
        let mut sn_defect_names: Vec<String> = Vec::new();

        macro_rules! handle_defect {
            ($script:expr, $evidence:expr, $classif:expr, $tc_id:expr, $messages:expr) => {
                if self.multi_defect {
                    info!("multi_defect mode: collecting defect and continuing exploration");
                    collected_defects.push(CollectedDefect {
                        script: $script,
                        evidence: $evidence,
                        classification: $classif,
                    });
                    $messages.push(Message::tool_response($tc_id, "Defect collected. Continue exploring for more defects. Try a different endpoint or parameter."));
                    continue;
                } else {
                    return Ok(($script, $evidence, $classif, collected_defects));
                }
            };
        }

        for turn in 0..self.max_turns {
            info!(
                "Agentic Exploration Turn {}/{}",
                turn + 1,
                self.max_turns
            );

            if turn > 0 {
                let state_json = executor.state.to_prompt_json();
                let coverage_report = coverage_tracker.report();
                info!("Injecting coverage report for turn {}: {} entries tracked", turn, coverage_tracker.visited_count());
                let state_msg = format!(
                    "=== EXPLORATION STATE ===\n{}\n\n=== COVERAGE REPORT ===\n{}\n\n\
                    Based on the state and coverage above, focus on UNTESTED parameters or try a DIFFERENT approach.",
                    state_json, coverage_report
                );
                messages.push(Message::user(state_msg));
            }

            if turn == 1 && executor.has_active_sandbox() {
                messages.push(Message::user(
                    "[HINT] Your database from the previous turn is still active and will be AUTOMATICALLY reused. \
                     Just write a script that operates on the existing data — no need to create a new collection. \
                     For example: delete points and verify count, or search and verify scores."
                        .to_string(),
                ));
            }

            if turn == 5 && sn_next_idx < sn_total {
                let batch_end = (sn_next_idx + sn_batch_size).min(sn_total);
                info!("Safety Net incremental batch 1: probes {}-{} of {}", sn_next_idx, batch_end - 1, sn_total);
                let mut first_in_batch = !executor.has_active_sandbox();
                for net in &all_safety_nets[sn_next_idx..batch_end] {
                    let safety_result = executor
                        .execute_test(&net.script, first_in_batch, &self.db_image, &self.pip_packages, self.db_port)
                        .await?;
                    if let Some(s) = safety_result.sandbox {
                        executor.put_sandbox(s);
                    }
                    first_in_batch = false;
                    if safety_result.found_defect {
                        sn_defect_names.push(net.name.clone());
                        if self.multi_defect {
                            collected_defects.push(CollectedDefect {
                                script: net.script.clone(),
                                evidence: generator::RunEvidence {
                                    phase: "safety_net_batch1".to_string(),
                                    db_url: safety_result.db_url.clone(),
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    classifier_reason: safety_result.classification.reason.clone(),
                                    classifier_evidence_excerpt: safety_result.classification.evidence_excerpt.clone(),
                                },
                                classification: safety_result.classification,
                            });
                        }
                    } else {
                        info!("Safety net probe '{}' passed", net.name);
                    }
                }
                sn_next_idx = batch_end;
                if !sn_defect_names.is_empty() {
                    let sn_msg = format!(
                        "\n\n[SAFETY NET BATCH 1] Found {} defect(s): {}. These are confirmed defects. Continue exploring for more.",
                        sn_defect_names.len(),
                        sn_defect_names.join(", ")
                    );
                    if let Some(last) = messages.last_mut() {
                        last.append_content(&sn_msg);
                    } else {
                        messages.push(Message::user(sn_msg));
                    }
                }
            }

            if turn == 9 && sn_next_idx < sn_total {
                let batch_end = (sn_next_idx + sn_batch_size).min(sn_total);
                info!("Safety Net incremental batch 2: probes {}-{} of {}", sn_next_idx, batch_end - 1, sn_total);
                let mut first_in_batch = !executor.has_active_sandbox();
                let mut batch2_defects = Vec::new();
                for net in &all_safety_nets[sn_next_idx..batch_end] {
                    let safety_result = executor
                        .execute_test(&net.script, first_in_batch, &self.db_image, &self.pip_packages, self.db_port)
                        .await?;
                    if let Some(s) = safety_result.sandbox {
                        executor.put_sandbox(s);
                    }
                    first_in_batch = false;
                    if safety_result.found_defect {
                        batch2_defects.push(net.name.clone());
                        sn_defect_names.push(net.name.clone());
                        if self.multi_defect {
                            collected_defects.push(CollectedDefect {
                                script: net.script.clone(),
                                evidence: generator::RunEvidence {
                                    phase: "safety_net_batch2".to_string(),
                                    db_url: safety_result.db_url.clone(),
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    classifier_reason: safety_result.classification.reason.clone(),
                                    classifier_evidence_excerpt: safety_result.classification.evidence_excerpt.clone(),
                                },
                                classification: safety_result.classification,
                            });
                        }
                    } else {
                        info!("Safety net probe '{}' passed", net.name);
                    }
                }
                sn_next_idx = batch_end;
                if !batch2_defects.is_empty() {
                    let sn_msg = format!(
                        "\n\n[SAFETY NET BATCH 2] Found {} defect(s): {}. Total safety net defects: {}. Continue exploring.",
                        batch2_defects.len(),
                        batch2_defects.join(", "),
                        sn_defect_names.len()
                    );
                    if let Some(last) = messages.last_mut() {
                        last.append_content(&sn_msg);
                    } else {
                        messages.push(Message::user(sn_msg));
                    }
                }
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
                    let requested_fresh = args.get("fresh_sandbox").and_then(|v| v.as_bool()).unwrap_or(true);
                    let fresh_sandbox = if executor.has_active_sandbox() {
                        if requested_fresh {
                            info!("FA requested fresh_sandbox=true but active sandbox exists. Auto-reusing sandbox (forced).");
                        }
                        false
                    } else {
                        true
                    };

                    let result = executor
                        .execute_test(code, fresh_sandbox, &self.db_image, &self.pip_packages, self.db_port)
                        .await?;

                    messages.push(Message::tool_response(&tc.id, &result.output));

                    for param in &["limit", "offset", "hnsw_ef", "exact", "score_threshold", "vector", "dimension", "shard_number"] {
                        if code.contains(param) {
                            coverage_tracker.record_visit(&self.contract.api_endpoint, param, "executed");
                        }
                    }

                    if let Some(sandbox) = result.sandbox {
                        info!("Got sandbox from ExecutionResult (fresh), oracle_pending={}", oracle.has_pending());
                        if oracle.has_pending() && !result.db_url.is_empty() {
                            info!("Running Oracle checks in sandbox (batch of {})...", oracle_batch_size);
                            let oracle_findings = oracle
                                .run_next_batch(&sandbox, &result.db_url, oracle_batch_size)
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
                        executor.put_sandbox(sandbox);
                        info!("Sandbox returned to executor, has_active_sandbox={}", executor.has_active_sandbox());
                    } else if executor.has_active_sandbox() && oracle.has_pending() && !result.db_url.is_empty() {
                        info!("No sandbox in ExecutionResult (reused), but executor has active sandbox. Running Oracle...");
                        let sandbox = executor.take_sandbox().unwrap();
                        let oracle_findings = oracle
                            .run_next_batch(&sandbox, &result.db_url, oracle_batch_size)
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
                        executor.put_sandbox(sandbox);
                        info!("Sandbox returned to executor after Oracle, has_active_sandbox={}", executor.has_active_sandbox());
                    } else {
                        info!("No sandbox available for Oracle checks (sandbox=None, has_active={})", executor.has_active_sandbox());
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
                    if turn < 4 {
                        messages.push(Message::tool_response(
                            &tc.id,
                            format!("REJECTED: You must explore for at least 5 turns before submitting (currently turn {}). Continue testing different parameters.", turn + 1),
                        ));
                        continue;
                    }

                    info!("Agent submitted MRE. Running all remaining Oracle checks before final validation...");
                    while oracle.has_pending() {
                        if let Some(sandbox) = executor.take_sandbox() {
                            let db_url = format!("http://{}:{}", sandbox.db_host.as_deref().unwrap_or("localhost"), self.db_port);
                            let violations = oracle.run_next_batch(&sandbox, &db_url, oracle_batch_size).await;
                            for v in &violations {
                                warn!("Oracle violation found: {} — {}", v.invariant_name, v.evidence);
                            }
                            executor.state.record_oracle_findings(violations);
                            executor.put_sandbox(sandbox);
                        } else {
                            break;
                        }
                    }

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
                        .execute_test(&code, true, &self.db_image, &self.pip_packages, self.db_port)
                        .await?;

                    let classification = result.classification.clone();

                    let oracle_violations = executor.state.oracle_violations_owned();
                    if !oracle_violations.is_empty() {
                        info!(
                            "Oracle found {} violation(s). Using Oracle findings.",
                            oracle_violations.len()
                        );
                    }

                    info!("Running safety net probes (remaining batch 3: {}-{} of {})...", sn_next_idx, sn_total - 1, sn_total);
                    let mut first_probe = !executor.has_active_sandbox();
                    for net in &all_safety_nets[sn_next_idx..] {
                        info!("Safety net probe: {} (fresh={})", net.name, first_probe);
                        let safety_result = executor
                            .execute_test(&net.script, first_probe, &self.db_image, &self.pip_packages, self.db_port)
                            .await?;
                        if let Some(s) = safety_result.sandbox {
                            executor.put_sandbox(s);
                        }
                        first_probe = false;

                        if safety_result.found_defect {
                            let initial_run = generator::RunEvidence {
                                phase: "safety_net".to_string(),
                                db_url: safety_result.db_url.clone(),
                                stdout: String::new(),
                                stderr: String::new(),
                                classifier_reason: safety_result.classification.reason.clone(),
                                classifier_evidence_excerpt: safety_result
                                    .classification
                                    .evidence_excerpt
                                    .clone(),
                            };
                            if self.multi_defect {
                                collected_defects.push(CollectedDefect {
                                    script: net.script.clone(),
                                    evidence: initial_run,
                                    classification: safety_result.classification,
                                });
                            } else {
                                return Ok((net.script.clone(), initial_run, safety_result.classification, collected_defects));
                            }
                        } else {
                            info!("Safety net probe '{}' passed", net.name);
                        }
                    }

                    if !oracle_violations.is_empty() {
                        let first_violation = &oracle_violations[0];
                        let oracle_defect_type = first_violation.defect_type.clone()
                            .unwrap_or(crate::agent::classifier::DefectType::StateLogicViolation);
                        let oracle_classification = crate::agent::classifier::ClassificationResult {
                            disposition: ClassificationDisposition::CandidateDefect,
                            defect_type: Some(oracle_defect_type),
                            reason: format!("Oracle detected: {}", first_violation.evidence),
                            evidence_excerpt: first_violation.evidence.chars().take(300).collect(),
                            sub_type: None,
                        };
                        let initial_run = generator::RunEvidence {
                            phase: "oracle".to_string(),
                            db_url: result.db_url.clone(),
                            stdout: String::new(),
                            stderr: String::new(),
                            classifier_reason: oracle_classification.reason.clone(),
                            classifier_evidence_excerpt: oracle_classification.evidence_excerpt.clone(),
                        };
                        handle_defect!(code, initial_run, oracle_classification, &tc.id, &mut messages);
                    }

                    if classification.disposition == ClassificationDisposition::Pass
                        || classification.disposition == ClassificationDisposition::CoverageDetected
                    {
                        warn!(
                            "FA found no defect ({}).",
                            if matches!(
                                classification.disposition,
                                ClassificationDisposition::CoverageDetected
                            ) {
                                "coverage report"
                            } else {
                                "non-defect MRE"
                            }
                        );
                    }

                    let initial_run = generator::RunEvidence {
                        phase: "initial".to_string(),
                        db_url: result.db_url.clone(),
                        stdout: String::new(),
                        stderr: String::new(),
                        classifier_reason: classification.reason.clone(),
                        classifier_evidence_excerpt: classification.evidence_excerpt.clone(),
                    };
                    handle_defect!(code, initial_run, classification, &tc.id, &mut messages);
                }
                "fuzz_boundary_values" => {
                    let cases = BoundaryValueGenerator::from_contract(&self.contract);
                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let focus_params: Vec<String> = args.get("focus_params")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();

                    let filtered: Vec<_> = if focus_params.is_empty() {
                        cases
                    } else {
                        cases.into_iter().filter(|c| {
                            focus_params.iter().any(|fp| c.name.contains(fp))
                        }).collect()
                    };

                    let mut response = format!("Generated {} boundary value test cases:\n\n", filtered.len());
                    for (i, case) in filtered.iter().enumerate() {
                        response.push_str(&format!("{}. {} (expected_rejection={})\n", i + 1, case.name, case.expected_rejection));
                        if let Some((ep, param, val)) = &case.coverage_entry {
                            coverage_tracker.record_visit(ep, param, val);
                        }
                    }
                    response.push_str("\nTo execute a test, copy the script from any case above and run it with execute_test_script.");
                    response.push_str("\nAlternatively, I can provide the full script for any specific case - just ask by name.");

                    let mut scripts_summary = String::new();
                    for case in &filtered {
                        scripts_summary.push_str(&format!("--- {} ---\n{}\n\n", case.name, case.script));
                    }
                    response.push_str(&format!("\n\n=== FULL SCRIPTS ===\n{}", scripts_summary));

                    info!("fuzz_boundary_values generated {} cases", filtered.len());
                    messages.push(Message::tool_response(&tc.id, &response));
                    if !filtered.is_empty() {
                        messages.push(Message::user(
                            "[HINT] Boundary test scripts are provided above. Pick one with an interesting violation and run it with execute_test_script to confirm the defect. This will also run Oracle checks automatically."
                        ));
                    }
                }
                "get_coverage_report" => {
                    let report = coverage_tracker.report();
                    info!("get_coverage_report: {} entries tracked", coverage_tracker.visited_count());
                    messages.push(Message::tool_response(&tc.id, &report));
                }
                "fuzz_api_sequence" => {
                    let all_cases = APISequenceExplorer::generate_sequences();
                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let seq_type = args.get("sequence_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("all");

                    let filtered: Vec<_> = if seq_type == "all" {
                        all_cases
                    } else {
                        all_cases.into_iter().filter(|c| c.sequence_type == seq_type).collect()
                    };

                    let mut response = format!("Generated {} API sequence test cases:\n\n", filtered.len());
                    for (i, case) in filtered.iter().enumerate() {
                        response.push_str(&format!("{}. {} [{}] (expected: {:?})\n", i + 1, case.name, case.sequence_type, case.expected_defect));
                    }

                    let mut scripts_summary = String::new();
                    for case in &filtered {
                        scripts_summary.push_str(&format!("--- {} [{}] ---\n{}\n\n", case.name, case.sequence_type, case.script));
                    }
                    response.push_str(&format!("\n=== FULL SCRIPTS ===\n{}", scripts_summary));

                    info!("fuzz_api_sequence generated {} cases", filtered.len());
                    messages.push(Message::tool_response(&tc.id, &response));
                    if !filtered.is_empty() {
                        messages.push(Message::user(
                            "[HINT] Sequence test scripts are provided above. Run one with execute_test_script to confirm the defect and trigger Oracle state checks."
                        ));
                    }
                }
                _ => {
                    messages.push(Message::tool_response(&tc.id, "Unknown tool."));
                }
            }
        }

        if let Some(code) = executor.last_test_code.clone() {
            warn!("Agentic exploration exceeded max turns. B2: submitting last test script as MRE.");
            let result = executor
                .execute_test(&code, true, &self.db_image, &self.pip_packages, self.db_port)
                .await?;
            let initial_run = generator::RunEvidence {
                phase: "initial".to_string(),
                db_url: result.db_url.clone(),
                stdout: String::new(),
                stderr: String::new(),
                classifier_reason: result.classification.reason.clone(),
                classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
            };
            return Ok((code, initial_run, result.classification, collected_defects));
        }

        warn!("No FA test scripts executed. Running remaining safety net probes ({}-{} of {}).", sn_next_idx, sn_total - 1, sn_total);
        let mut first_probe = !executor.has_active_sandbox();
        for net in &all_safety_nets[sn_next_idx..] {
            info!("Safety net probe (fallback): {} (fresh={})", net.name, first_probe);
            let safety_result = executor
                .execute_test(&net.script, first_probe, &self.db_image, &self.pip_packages, self.db_port)
                .await?;
            if let Some(s) = safety_result.sandbox {
                executor.put_sandbox(s);
            }
            first_probe = false;

            if safety_result.found_defect {
                let initial_run = generator::RunEvidence {
                    phase: "initial".to_string(),
                    db_url: safety_result.db_url.clone(),
                    stdout: String::new(),
                    stderr: String::new(),
                    classifier_reason: safety_result.classification.reason.clone(),
                    classifier_evidence_excerpt: safety_result.classification.evidence_excerpt.clone(),
                };
                return Ok((net.script.clone(), initial_run, safety_result.classification, collected_defects));
            }
            warn!(
                "Safety net '{}' did not trigger (properly rejected).",
                net.name
            );
        }
        if !collected_defects.is_empty() {
            let first = collected_defects.remove(0);
            return Ok((first.script, first.evidence, first.classification, collected_defects));
        }
        anyhow::bail!("No defect found by FA or any safety net probe");
    }
}
