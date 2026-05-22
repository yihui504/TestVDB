use crate::agent::classifier::ClassificationDisposition;
use crate::agent::executor::FAExecutor;
use crate::agent::llm::{DeepSeekClient, Message};
use crate::agent::oracle::{build_oracle_findings_message, Oracle};
use crate::agent::tools::{get_execute_test_script_tool, get_submit_mre_tool, get_execute_stateful_test_tool, get_compare_endpoints_tool, get_coverage_report_tool};
use crate::agent::vdbfuzz::coverage::{CoverageTracker, ApiEndpoint};
use crate::contract::schema::StructuredContract;
use crate::report::generator;
use crate::sandbox::manager::SidecarSpec;
use crate::target::{SafetyNet, TargetPlugin};
use tracing::{info, warn};

fn build_system_prompt(contract_content: &str) -> String {
    format!(
        "You are a security researcher performing CREATIVE defect discovery. Find defects that deterministic generators CANNOT find.\n        \n        === YOUR CAPABILITIES ===\n        1. STATE CONSISTENCY — verify count, visibility, data integrity after operations\n        2. CONCURRENT RACE CONDITIONS — parallel ops on same resources, verify no corruption\n        3. TIMING SENSITIVITY — flush/load/delete with immediate=true, exposing stale-read bugs\n        4. SEMANTIC EQUIVALENCE — compare REST vs SDK, or two API paths that behave identically\n        5. BOUNDARY DEEPENING — take a known boundary violation and test RELATED parameters\n        6. CROSS-ENDPOINT CHAINS — test sequences: create -> insert -> index -> search -> drop\n        \n        === TOOLS ===\n        execute_stateful_test(test_name, pattern_category, steps, invariant)\n        execute_concurrent_test(test_name, pattern_category, setup_steps, concurrent_actions, state_check)\n        execute_timing_test(test_name, pattern_category, steps, invariant)\n        compare_endpoints(comparison_name, operation_a, operation_b, expected_equivalence)\n        execute_test_script(code, fresh_sandbox?) — Run custom Python scripts\n        get_coverage_report() — Show tested vs untested parameters and pattern diversity\n        \n        === EXPLORATION STRATEGY (adaptive) ===\n        - Turns 1-4: STATE CONSISTENCY (insert->count, delete->count, flush->search visibility)\n        - Turns 5-8: CONCURRENT + TIMING (parallel operations, immediate=true for async bugs)\n        - Turns 9+: DEEP exploration — cross-endpoint chains, boundary deepening, semantic equivalence\n        - Schedule is GUIDANCE, not prison — switch if you find a promising pattern\n        - After any defect, probe 2 MORE related parameters for systemic weakness\n        \n        === RULES ===\n        1. DO NOT submit MRE before turn 3. Explore at least 3 different angles first.\n        2. Test interactions BETWEEN parameters and STATE after sequences.\n        3. You CAN test boundary values as verification — go DEEPER when you find one.\n        4. Every step in execute_stateful_test/execute_timing_test MUST include a state_check.\n        \n        === SEMANTIC INVARIANTS (violation = BUG) ===\n        - rowCount = insertCount - deleteCount\n        - After flush, inserted data must be searchable\n        - After drop+recreate, previous data gone (rowCount=0)\n        - Concurrent inserts: final count = sum of all inserts\n        - Concurrent upserts same ID: no duplicates\n        - Search distance ordering: L2 ascending, COSINE/IP descending\n        - Delete then immediate query: no stale reads\n        - Search with filters: only matching data returned\n        - Pagination: offset changes = disjoint result sets\n        \n        === PATTERN CATEGORIES ===\n        count_consistency, data_visibility, state_residual, idempotency, search_correctness,\n        partition_isolation, alias_state, index_state, concurrent_insert_count,\n        concurrent_upsert_duplicate, concurrent_delete_stale, concurrent_create_conflict,\n        flush_visibility, load_search_failure, delete_stale_read, index_immediate_use,\n        cross_endpoint_chain, semantic_equivalence, boundary_deepening\n        \n        === DEFECT TYPES ===\n        STATE_LOGIC_VIOLATION | SEQUENCE_VIOLATION | ILLEGAL_SUCCESS | DIFFERENTIAL_MISMATCH | POOR_DIAGNOSTICS\n        \n        === TARGET-SPECIFIC NOTES ===\n        - Milvus: check r.json().get('code')==0, Bearer root:Milvus auth\n        - Qdrant: check status_code==200, no auth\n        - Weaviate: check status_code==200, /v1/schema and /v1/objects paths\n        - PgVector: use psycopg2, connect to postgresql://postgres:postgres@host:5432/testvdb\n        \n        Contract:\n{}\n",
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
    sidecars: Vec<SidecarSpec>,
    db_env: Vec<(String, String)>,
    db_command: Vec<String>,
    max_turns: usize,
    multi_defect: bool,
    custom_system_prompt: Option<String>,
    custom_initial_message: Option<String>,
    batch_defects_summary: String,
    skip_safety_nets: bool,
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
        let sidecars = plugin.db_sidecars();
        let db_env = plugin.db_env();
        let db_command = plugin.db_command();
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
            sidecars,
            db_env,
            db_command,
            max_turns,
            multi_defect,
            custom_system_prompt: None,
            custom_initial_message: None,
            skip_safety_nets: false,
            batch_defects_summary: String::new(),
        }
    }

    pub fn with_custom_prompt(mut self, system_prompt: String, initial_message: String) -> Self {
        self.custom_system_prompt = Some(system_prompt);
        self.custom_initial_message = Some(initial_message);
        self
    }

    pub fn with_batch_defects(mut self, summary: String) -> Self {
        self.batch_defects_summary = summary;
        self
    }

    pub fn with_skip_safety_nets(mut self, skip: bool) -> Self {
        self.skip_safety_nets = skip;
        self
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

    fn build_defect_context(&self) -> String {
        let target = self.plugin.name();
        let mut ctx = String::from("=== DATABASE-SPECIFIC WEAKNESS MAP ===\n");
        ctx.push_str("Focus on these KNOWN blind spots:\n\n");
        match target {
            "milvus" => ctx.push_str("MILVUS: REST proxy silently passes unknown params; TTL/schema poorly validated; create-drop-create state loss. ACCEPTED: param gaps, type confusion. AVOID: upsert semantics.\n"),
            "qdrant" => ctx.push_str("QDRANT: Async upsert silent discard (#9039); hnsw_ef=0 accepted; score_threshold edge cases. ACCEPTED: param boundaries, async gaps.\n"),
            "weaviate" => ctx.push_str("WEAVIATE: Schema validates some params but not others; dim mismatch->500; quantization silently normalized. ACCEPTED: validation gaps, silent normalization.\n"),
            "pgvector" => ctx.push_str("PGVECTOR: Param validation is PostgreSQL-grade. DON'T test single-param boundaries. Test: concurrent index builds, VACUUM+search, many indexes, iterative scans. ACCEPTED: race conditions, query plan issues.\n"),
            _ => {}
        }

        // ── Inject deterministic generator findings as DO-NOT-RETEST constraints ──
        if !self.batch_defects_summary.is_empty() {
            ctx.push_str("\n=== DETERMINISTIC GENERATORS ALREADY FOUND (DO NOT RETEST THESE) ===\n");
            ctx.push_str(&self.batch_defects_summary);
            ctx.push_str("\nCRITICAL: The parameters and endpoints above have ALREADY been tested by deterministic generators.\n");
            ctx.push_str("DO NOT write tests that only vary single parameter values — that's already covered.\n");
            ctx.push_str("INSTEAD, focus on: STATE after sequences, CONCURRENT races, SEMANTIC equivalence, CROSS-ENDPOINT chains.\n");
        }

        ctx.push_str("\nEach turn test ONE hypothesis from above. Find defect -> test 2 MORE related params.\n");
        ctx
    }

    /// Run a slice of safety net probes, returning (name, script, result) for each probe.
    async fn execute_safety_nets(
        &self,
        executor: &mut FAExecutor,
        nets: &[SafetyNet],
        first_probe: bool,
    ) -> anyhow::Result<Vec<(String, String, crate::agent::executor::ExecutionResult)>> {
        let mut results = Vec::new();
        let mut first = first_probe;
        for net in nets {
            let result = executor
                .execute_test(&net.script, first, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command)
                .await?;
            if let Some(ref s) = result.sandbox {
                executor.put_sandbox(s.clone());
            }
            first = false;
            results.push((net.name.clone(), net.script.clone(), result));
        }
        Ok(results)
    }

    pub async fn run(
        &self,
    ) -> anyhow::Result<(
        String,
        generator::RunEvidence,
        crate::agent::classifier::ClassificationResult,
        Vec<CollectedDefect>,
    )> {
        let tools = vec![get_execute_test_script_tool(), get_submit_mre_tool(), get_execute_stateful_test_tool(), get_compare_endpoints_tool(), get_coverage_report_tool()];
        let system_prompt = self.custom_system_prompt.clone()
            .unwrap_or_else(|| build_system_prompt(&self.contract_content));
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

        let initial_msg = if let Some(ref custom_msg) = self.custom_initial_message {
            custom_msg.clone()
        } else {
            "Begin creative exploration. Use execute_api_sequence to test multi-step API sequences, or compare_endpoints to test semantic equivalence between operations. Focus on finding defects that deterministic generators cannot find.".to_string()
        };

        let behavioral_section = self.build_behavioral_section();
        let initial_msg = if !behavioral_section.is_empty() {
            format!("{}\n\n{}", initial_msg, behavioral_section)
        } else {
            initial_msg
        };

        let initial_msg = if !self.batch_defects_summary.is_empty() {
            format!("{}\n\n=== DETERMINISTIC GENERATOR FINDINGS ===\n{}\n\nFocus on finding defects the deterministic generators MISSED — state sequences, semantic equivalence, and cross-endpoint inconsistencies.", initial_msg, self.batch_defects_summary)
        } else {
            initial_msg
        };

        // P1: Inject database-specific defect context
        let defect_context = self.build_defect_context();
        let initial_msg = format!("{}\n\n{}", initial_msg, defect_context);

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

        // Inline defect collection: multi_defect → collect & continue; single → return immediately.
        fn collect_or_return(
            multi_defect: bool,
            collected_defects: &mut Vec<CollectedDefect>,
            script: String,
            evidence: generator::RunEvidence,
            classification: crate::agent::classifier::ClassificationResult,
        ) -> Option<(String, generator::RunEvidence, crate::agent::classifier::ClassificationResult, Vec<CollectedDefect>)> {
            if multi_defect {
                info!("multi_defect mode: collecting defect and continuing exploration");
                collected_defects.push(CollectedDefect { script, evidence, classification });
                None
            } else {
                let remaining = std::mem::take(collected_defects);
                Some((script, evidence, classification, remaining))
            }
        }

        let mut last_assertion_count = 0usize;
        let mut no_progress_turns = 0usize;

        for turn in 0..self.max_turns {
            // P1: Truncate message history to prevent token overflow
            if messages.len() > 20 {
                let system_msg = messages.first().cloned();
                messages = messages.split_off(messages.len().saturating_sub(16));
                if let Some(sys) = system_msg {
                    messages.insert(0, sys);
                }
                info!("Truncated message history to {} messages (max 20)", messages.len());
            }

            // P2: Convergence detection — stop early if no new assertions for 3 turns
            let current_assertions = executor.unique_assertions_count();
            if turn > 0 && current_assertions == last_assertion_count {
                no_progress_turns += 1;
            } else {
                no_progress_turns = 0;
            }
            last_assertion_count = current_assertions;

            if no_progress_turns >= 3 && turn >= 5 {
                info!("Convergence: no new assertions for {} turns. Stopping early at turn {}.", no_progress_turns, turn + 1);
                break;
            }

            info!(
                "Agentic Exploration Turn {}/{}",
                turn + 1,
                self.max_turns
            );

            // ── Phase-guided exploration: first 3 turns → STATE/SEMANTIC, not ILLEGAL_SUCCESS ──
            if turn == 0 {
                messages.push(Message::user(
                    "[EXPLORATION PHASE 1] Turns 1-4: Focus EXCLUSIVELY on STATE CONSISTENCY and SEMANTIC CORRECTNESS.\n\
                     - Test: insert→count consistency, flush→search visibility, create→drop→recreate state\n\
                     - Test: concurrent operations on same resource, timing-sensitive flush/load patterns\n\
                     - Do NOT test single-parameter boundaries (e.g., limit=-1, offset=0) — those are already covered by deterministic generators.\n\
                     - Do NOT submit MRE before turn 4."
                ));
            }
            if turn == 4 {
                messages.push(Message::user(
                    "[EXPLORATION PHASE 2] Turns 5+: You may now explore CONCURRENT races, CROSS-ENDPOINT chains, and SEMANTIC EQUIVALENCE.\n\
                     - Test: compare REST vs SDK behavior, two API paths returning different results\n\
                     - Test: alias state consistency, partition isolation, TTL behavior\n\
                     - If you found a STATE or SEMANTIC defect, prepare a script with [DEFECT: ...] marker and call submit_mre."
                ));
            }

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
                let batch_nets: Vec<_> = all_safety_nets[sn_next_idx..batch_end].to_vec();
                let first_probe = !executor.has_active_sandbox();
                for (name, script, result) in self.execute_safety_nets(&mut executor, &batch_nets, first_probe).await? {
                    if result.found_defect {
                        sn_defect_names.push(name.clone());
                        if self.multi_defect {
                            collected_defects.push(CollectedDefect {
                                script,
                                evidence: generator::RunEvidence {
                                    phase: "safety_net_batch1".to_string(),
                                    db_url: result.db_url.clone(),
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    classifier_reason: result.classification.reason.clone(),
                                    classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
                                },
                                classification: result.classification,
                            });
                        }
                    } else {
                        info!("Safety net probe '{}' passed", name);
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
                let batch_nets: Vec<_> = all_safety_nets[sn_next_idx..batch_end].to_vec();
                let first_probe = !executor.has_active_sandbox();
                let mut batch2_defects = Vec::new();
                for (name, script, result) in self.execute_safety_nets(&mut executor, &batch_nets, first_probe).await? {
                    if result.found_defect {
                        batch2_defects.push(name.clone());
                        sn_defect_names.push(name.clone());
                        if self.multi_defect {
                            collected_defects.push(CollectedDefect {
                                script,
                                evidence: generator::RunEvidence {
                                    phase: "safety_net_batch2".to_string(),
                                    db_url: result.db_url.clone(),
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    classifier_reason: result.classification.reason.clone(),
                                    classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
                                },
                                classification: result.classification,
                            });
                        }
                    } else {
                        info!("Safety net probe '{}' passed", name);
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
                            info!("FA requested fresh_sandbox=true. Destroying old sandbox and creating fresh one.");
                            if let Some(old_sandbox) = executor.take_sandbox() {
                                let _ = old_sandbox.cleanup().await;
                            }
                            true
                        } else {
                            false
                        }
                    } else {
                        true
                    };

                    let result = executor
                        .execute_test(code, fresh_sandbox, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command)
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
                        .execute_test(&code, true, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command)
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
                    let batch_nets: Vec<_> = all_safety_nets[sn_next_idx..].to_vec();
                    let first_probe = !executor.has_active_sandbox();
                    for (name, script, result) in self.execute_safety_nets(&mut executor, &batch_nets, first_probe).await? {
                        if result.found_defect {
                            let initial_run = generator::RunEvidence {
                                phase: "safety_net".to_string(),
                                db_url: result.db_url.clone(),
                                stdout: String::new(),
                                stderr: String::new(),
                                classifier_reason: result.classification.reason.clone(),
                                classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
                            };
                            if self.multi_defect {
                                collected_defects.push(CollectedDefect {
                                    script,
                                    evidence: initial_run,
                                    classification: result.classification,
                                });
                            } else {
                                return Ok((script, initial_run, result.classification, collected_defects));
                            }
                        } else {
                            info!("Safety net probe '{}' passed", name);
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
                        if let Some(ret) = collect_or_return(self.multi_defect, &mut collected_defects, code, initial_run, oracle_classification) {
                            return Ok(ret);
                        }
                        messages.push(Message::tool_response(&tc.id, "Defect collected. Continue exploring for more defects. Try a different endpoint or parameter."));
                        continue;
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
                    if let Some(ret) = collect_or_return(self.multi_defect, &mut collected_defects, code, initial_run, classification) {
                        return Ok(ret);
                    }
                    messages.push(Message::tool_response(&tc.id, "Defect collected. Continue exploring for more defects. Try a different endpoint or parameter."));
                    continue;
                }
                "execute_api_sequence" => {
                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let sequence_name = args.get("sequence_name").and_then(|v| v.as_str()).unwrap_or("unnamed");
                    let steps = args.get("steps").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                    let invariant = args.get("invariant").and_then(|v| v.as_str()).unwrap_or("");

                    let mut script = format!("# API Sequence Test: {}\n", sequence_name);
                    script.push_str("import requests, sys, uuid, time, json\n");
                    script.push_str("BASE = '{{TESTVDB_DB_URL}}'\n");
                    script.push_str("HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}\n");
                    script.push_str("def api(path, body):\n");
                    script.push_str("    r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body)\n");
                    script.push_str("    return r.json()\n\n");

                    for (i, step) in steps.iter().enumerate() {
                        let endpoint = step.get("endpoint").and_then(|v| v.as_str()).unwrap_or("/");
                        let params = step.get("params").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        let expect = step.get("expect").and_then(|v| v.as_str()).unwrap_or("any");
                        let params_str = serde_json::to_string(&params).unwrap_or_default();

                        script.push_str(&format!("# Step {}: {} (expect: {})\n", i + 1, endpoint, expect));
                        script.push_str(&format!("r{} = api('{}', {})\n", i + 1, endpoint, params_str));
                        script.push_str(&format!("print('Step {} result:', json.dumps(r{}))\n", i + 1, i + 1));
                        if expect == "success" {
                            script.push_str(&format!("if r{}.get('code') != 0: print('[DEFECT: SEQUENCE_VIOLATION] Step {} expected success but got:', r{}); sys.exit(1)\n", i + 1, i + 1, i + 1));
                        } else if expect == "error" {
                            script.push_str(&format!("if r{}.get('code') == 0: print('[DEFECT: SEQUENCE_VIOLATION] Step {} expected error but succeeded'); sys.exit(1)\n", i + 1, i + 1));
                        }
                        script.push_str("time.sleep(0.3)\n\n");
                    }

                    if !invariant.is_empty() {
                        script.push_str(&format!("# Invariant check: {}\n", invariant));
                        script.push_str(&format!("print('Invariant: {}')\n", invariant));
                    }

                    script.push_str("print('All steps completed successfully')\nsys.exit(0)\n");

                    let fresh_sandbox = !executor.has_active_sandbox();
                    let result = executor
                        .execute_test(&script, fresh_sandbox, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command)
                        .await?;

                    messages.push(Message::tool_response(&tc.id, &result.output));

                    if let Some(sandbox) = result.sandbox {
                        executor.put_sandbox(sandbox);
                    }
                }
                "compare_endpoints" => {
                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let comparison_name = args.get("comparison_name").and_then(|v| v.as_str()).unwrap_or("unnamed");
                    let op_a = args.get("operation_a").cloned().unwrap_or_default();
                    let op_b = args.get("operation_b").cloned().unwrap_or_default();
                    let expected_eq = args.get("expected_equivalence").and_then(|v| v.as_str()).unwrap_or("");

                    let endpoint_a = op_a.get("endpoint").and_then(|v| v.as_str()).unwrap_or("/");
                    let params_a = op_a.get("params").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    let desc_a = op_a.get("description").and_then(|v| v.as_str()).unwrap_or("Operation A");

                    let endpoint_b = op_b.get("endpoint").and_then(|v| v.as_str()).unwrap_or("/");
                    let params_b = op_b.get("params").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    let desc_b = op_b.get("description").and_then(|v| v.as_str()).unwrap_or("Operation B");

                    let params_a_str = serde_json::to_string(&params_a).unwrap_or_default();
                    let params_b_str = serde_json::to_string(&params_b).unwrap_or_default();

                    let script = format!(
                        "# Semantic Equivalence Comparison: {}\n\
                        import requests, sys, uuid, time, json\n\
                        BASE = '{{{{TESTVDB_DB_URL}}}}'\n\
                        HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}\n\
                        def api(path, body):\n\
                            r = requests.post(f'{{BASE}}{{path}}', headers=HEADERS, json=body)\n\
                            return r.json()\n\n\
                        # Operation A: {}\n\
                        rA = api('{}', {})\n\
                        print('Operation A result:', json.dumps(rA))\n\
                        time.sleep(0.3)\n\n\
                        # Operation B: {}\n\
                        rB = api('{}', {})\n\
                        print('Operation B result:', json.dumps(rB))\n\n\
                        # Expected equivalence: {}\n\
                        code_a = rA.get('code')\n\
                        code_b = rB.get('code')\n\
                        if code_a != code_b:\n\
                            print(f'[DEFECT: DIFFERENTIAL_MISMATCH] Operation A code={{code_a}} vs Operation B code={{code_b}}')\n\
                            print(f'Expected equivalence: {}')\n\
                            sys.exit(1)\n\
                        print('Both operations returned same status code. No differential mismatch found.')\n\
                        sys.exit(0)\n",
                        comparison_name, desc_a, endpoint_a, params_a_str, desc_b, endpoint_b, params_b_str, expected_eq, expected_eq
                    );

                    let fresh_sandbox = !executor.has_active_sandbox();
                    let result = executor
                        .execute_test(&script, fresh_sandbox, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command)
                        .await?;

                    messages.push(Message::tool_response(&tc.id, &result.output));

                    if let Some(sandbox) = result.sandbox {
                        executor.put_sandbox(sandbox);
                    }
                }
                "get_coverage_report" => {
                    let report = coverage_tracker.report();
                    info!("get_coverage_report: {} entries tracked", coverage_tracker.visited_count());
                    messages.push(Message::tool_response(&tc.id, &report));
                }
                _ => {
                    messages.push(Message::tool_response(&tc.id, "Unknown tool."));
                }
            }
        }

        if let Some(code) = executor.last_test_code.clone() {
            warn!("Agentic exploration exceeded max turns. B2: submitting last test script as MRE.");
            let result = executor
                .execute_test(&code, true, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command)
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
        let batch_nets: Vec<_> = all_safety_nets[sn_next_idx..].to_vec();
        let first_probe = !executor.has_active_sandbox();
        for (name, script, result) in self.execute_safety_nets(&mut executor, &batch_nets, first_probe).await? {
            if result.found_defect {
                let initial_run = generator::RunEvidence {
                    phase: "initial".to_string(),
                    db_url: result.db_url.clone(),
                    stdout: String::new(),
                    stderr: String::new(),
                    classifier_reason: result.classification.reason.clone(),
                    classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
                };
                return Ok((script, initial_run, result.classification, collected_defects));
            }
            warn!("Safety net '{}' did not trigger (properly rejected).", name);
        }
        if !collected_defects.is_empty() {
            let first = collected_defects.remove(0);
            return Ok((first.script, first.evidence, first.classification, collected_defects));
        }
        anyhow::bail!("No defect found by FA or any safety net probe");
    }
}