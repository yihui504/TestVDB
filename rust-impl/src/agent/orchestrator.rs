use std::cell::RefCell;
use std::collections::HashSet;

use crate::agent::classifier::{ClassificationDisposition, LlmReview};
use crate::agent::executor::FAExecutor;
use crate::agent::harness::{Harness, ReviewOutcome};
use crate::agent::llm::{DeepSeekClient, Message};
use crate::agent::oracle::{build_oracle_findings_message, Oracle};
use crate::agent::tools::{get_execute_test_script_tool, get_submit_mre_tool, get_execute_stateful_test_tool, get_execute_differential_test_tool, get_coverage_report_tool};
use crate::agent::vdbfuzz::coverage::{CoverageTracker, PatternTracker, ApiEndpoint};
use crate::contract::schema::StructuredContract;
use crate::report::generator;
use crate::sandbox::manager::SidecarSpec;
use crate::target::{SafetyNet, TargetPlugin};
use tracing::{info, warn};

// Message history management
const MESSAGE_HISTORY_MAX: usize = 20;   // Max messages before truncation
const MESSAGE_HISTORY_KEEP: usize = 16;  // Messages to keep after truncation

fn build_system_prompt(contract_content: &str) -> String {
    format!(
        concat!(
            "Test STATE, CONCURRENT, TIMING, SEARCH_CORRECTNESS, CROSS_ENDPOINT bugs in a vector DB.\n",
            "\n",
            "=== MISSION ===\n",
            "1) State consistency (rowCount) 2) Concurrent races 3) Timing (immediate=true) 4) Search correctness (distances, top-k) 5) Cross-endpoint consistency.\n",
            "[MANDATORY TASK] assigns endpoint+pattern. Test assigned only.\n",
            "\n",
            "=== PATTERNS ===\n",
            "search_correctness: search SAME data with nprobe=1 then nprobe=100 — if top-1 distance identical, search quality broken → [DEFECT: SEARCH_CORRECTNESS]\n",
            "cross_endpoint_chain: insert N → compare search count vs query count — if different → [DEFECT: CROSS_ENDPOINT_INCONSISTENCY]\n",
            "concurrent: 5 threads create same collection — if >1 succeeds → [DEFECT: CONCURRENT_RACE]\n",
            "timing: insert+immediate get_stats — if rowCount=0 → [DEFECT: STATE_VIOLATION]\n",
            "count_consistency: insert N → get_stats rowCount — if ≠N → [DEFECT: STATE_VIOLATION]\n",
            "data_visibility: insert → search — if 0 results → [DEFECT: STATE_VIOLATION]\n",
            "\n",
            "=== RULES ===\n",
            "1. EVERY execute_stateful_test step needs state_check.\n",
            "2. No single-param boundary tests.\n",
            "3. Verify state after ops. Print [DEFECT:...] if found.\n",
            "4. pattern_category must match MANDATORY TASK.\n",
            "5. Use execute_test_script if structured tools don't fit.\n",
            "6. For search_correctness pattern: use execute_differential_test with test_type='search_correctness', comparison='should_differ'. Call A: search with searchParams ef=1, Call B: search with searchParams ef=100. Compare top-1 distance.\n",
            "7. For cross_endpoint_chain pattern: use execute_differential_test with test_type='cross_endpoint_consistency', comparison='should_match'. Call A: search with filter, count results. Call B: query with same filter, count results. Compare counts.\n",
            "\n",
            "=== DEFECT MARKERS (print the matching one when you find a defect) ===\n",
            "[DEFECT: ILLEGAL_SUCCESS] — API returns success(code=0) for invalid input (bad param, out-of-range, missing field)\n",
            "[DEFECT: STATE_VIOLATION] — State inconsistency: rowCount≠N after insert, entity exists after delete, index missing after create\n",
            "[DEFECT: SEARCH_CORRECTNESS] — Search returns wrong results: distances not sorted, top-k≠k, nprobe=1 same as nprobe=100\n",
            "[DEFECT: CROSS_ENDPOINT_INCONSISTENCY] — Same op via different paths gives different results: search count≠query count, create.ttl≠alter.ttl\n",
            "[DEFECT: CONCURRENT_RACE] — Concurrent ops cause data loss, duplicate creates succeed, count mismatch after parallel inserts\n",
            "[DEFECT: DATA_CORRUPTION] — Retrieved data≠inserted data: wrong vectors, missing fields, corrupted IDs\n",
            "[DEFECT: DIAGNOSTIC_FAILURE] — Error message doesn't identify the problem field, or undocumented required params\n",
            "\n",
            "=== CONTRACT ===\n",
            "{}",
        ),
        contract_content
    )
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TurnRecord {
    pub turn: usize,
    pub tool_name: String,
    pub state_check_present: bool,
    pub result_category: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConversationRecord {
    pub turn: usize,
    pub task_endpoint: String,
    pub task_pattern: String,
    pub tool_name: String,
    pub script_code: Option<String>,
    pub result_category: String,
    pub defect_type: Option<String>,
    pub boundary_rejected: bool,
    pub state_check_methods: Vec<String>,
    pub oracle_violations_count: usize,
    pub stdout_preview: Option<String>,
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
    baseline_output: Option<String>,
    harness: RefCell<Harness>,
    processed_defect_keys: RefCell<HashSet<String>>,
    early_mre_unlock: RefCell<bool>,
    baseline_records: RefCell<Vec<TurnRecord>>,
    conversation_log: RefCell<Vec<ConversationRecord>>,
    diagnostic_dir: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectedDefect {
    pub script: String,
    pub evidence: generator::RunEvidence,
    pub classification: crate::agent::classifier::ClassificationResult,
}

fn should_reject_boundary_test(code: &str, batch_defects_summary: &str) -> bool {
    let boundary_params = ["limit=-1", "limit=0", "offset=-1", "offset=0", "nprobe=-1", "nprobe=0", "shardsNum=null", "shardsNum=0", "shard_number=0", "replication_factor=-1", "ef=-1", "ef=0", "hnsw_ef=-1", "hnsw_ef=0", "Authorization=null"];
    let boundary_hits: Vec<_> = boundary_params.iter().filter(|p| code.contains(*p)).collect();
    let is_short_script = code.lines().count() < 12;
    let is_seed_expansion = !batch_defects_summary.is_empty()
        && boundary_hits.iter().any(|p| batch_defects_summary.contains(**p));
    !is_seed_expansion && (boundary_hits.len() >= 2 || (boundary_hits.len() == 1 && is_short_script))
}

fn classify_result(stdout: &str, exit_success: bool) -> String {
    if stdout.contains("[DEFECT:") || stdout.contains("[DEFECT ") {
        "defect_found".to_string()
    } else if stdout.contains("Error") || stdout.contains("error") || !exit_success {
        "error".to_string()
    } else {
        "success".to_string()
    }
}

#[derive(Debug, Clone)]
struct ExplorationTask {
    endpoint: String,
    pattern: String,
    description: String,
}

fn select_next_task(
    coverage_tracker: &CoverageTracker,
    pattern_tracker: &PatternTracker,
    skipped_tasks: &HashSet<String>,
) -> Option<ExplorationTask> {
    let unvisited = coverage_tracker.unvisited_params();
    let unexplored = pattern_tracker.unexplored_patterns();

    if unvisited.is_empty() && unexplored.is_empty() {
        return None;
    }

    let mut candidates: Vec<ExplorationTask> = Vec::new();

    for (endpoint, param) in &unvisited {
        for pattern in &unexplored {
            let task_key = format!("{}|{}|{}", endpoint, param, pattern);
            if skipped_tasks.contains(&task_key) {
                continue;
            }
            let description = build_task_description(endpoint, param, pattern);
            candidates.push(ExplorationTask {
                endpoint: endpoint.clone(),
                pattern: pattern.to_string(),
                description,
            });
        }
    }

    if candidates.is_empty() && !unexplored.is_empty() {
        for pattern in &unexplored {
            let task_key = format!("any|{}", pattern);
            if skipped_tasks.contains(&task_key) {
                continue;
            }
            let endpoint = unvisited.first().map(|(e, _)| e.clone()).unwrap_or_default();
            let description = build_task_description(&endpoint, "any", pattern);
            candidates.push(ExplorationTask {
                endpoint,
                pattern: pattern.to_string(),
                description,
            });
        }
    }

    if candidates.is_empty() && !unvisited.is_empty() {
        for (endpoint, param) in &unvisited {
            let task_key = format!("{}|{}|any", endpoint, param);
            if skipped_tasks.contains(&task_key) {
                continue;
            }
            let explored = pattern_tracker.explored_patterns();
            let pattern = explored.first().map(|p| p.to_string()).unwrap_or("count_consistency".to_string());
            let description = build_task_description(endpoint, param, &pattern);
            candidates.push(ExplorationTask {
                endpoint: endpoint.clone(),
                pattern,
                description,
            });
        }
    }

    candidates.into_iter().next()
}

fn build_task_description(endpoint: &str, param: &str, pattern: &str) -> String {
    match pattern {
        "count_consistency" => format!("Test count_consistency on {}: insert entities then verify rowCount matches. Check param '{}'.", endpoint, param),
        "data_visibility" => format!("Test data_visibility on {}: insert data, flush, then search — verify all data is visible. Check param '{}'.", endpoint, param),
        "state_residual" => format!("Test state_residual on {}: drop a resource then verify no residual state remains. Check param '{}'.", endpoint, param),
        "idempotency" => format!("Test idempotency on {}: call the same operation twice and verify identical results. Check param '{}'.", endpoint, param),
        "search_correctness" => format!("Test search_correctness on {}: insert known vectors, search, verify top-k results are correct. Check param '{}'.", endpoint, param),
        "partition_isolation" => format!("Test partition_isolation on {}: insert into partition A, query partition B, verify no cross-contamination. Check param '{}'.", endpoint, param),
        "alias_state" => format!("Test alias_state on {}: create alias, alter collection, verify alias reflects changes. Check param '{}'.", endpoint, param),
        "index_state" => format!("Test index_state on {}: create index, drop index, verify search behavior changes. Check param '{}'.", endpoint, param),
        "concurrent_insert_count" => format!("Test concurrent_insert_count on {}: parallel inserts on same collection, verify final rowCount. Check param '{}'.", endpoint, param),
        "concurrent_upsert_duplicate" => format!("Test concurrent_upsert_duplicate on {}: parallel upserts with same ID, verify no duplicates. Check param '{}'.", endpoint, param),
        "concurrent_delete_stale" => format!("Test concurrent_delete_stale on {}: concurrent delete + search, verify no stale reads. Check param '{}'.", endpoint, param),
        "concurrent_create_conflict" => format!("Test concurrent_create_conflict on {}: parallel create same collection, verify only one succeeds. Check param '{}'.", endpoint, param),
        "concurrent_mixed_ops" => format!("Test concurrent_mixed_ops on {}: parallel insert+delete+search, verify consistency. Check param '{}'.", endpoint, param),
        "flush_visibility" => format!("Test flush_visibility on {}: insert without flush, search, then flush and search again. Check param '{}'.", endpoint, param),
        "load_search_failure" => format!("Test load_search_failure on {}: search on unloaded collection, then load and retry. Check param '{}'.", endpoint, param),
        "delete_stale_read" => format!("Test delete_stale_read on {}: delete entities, search immediately, verify deleted entities not returned. Check param '{}'.", endpoint, param),
        "index_immediate_use" => format!("Test index_immediate_use on {}: create index, immediately search, verify index is used. Check param '{}'.", endpoint, param),
        "compact_immediate_effect" => format!("Test compact_immediate_effect on {}: compact collection, verify data integrity preserved. Check param '{}'.", endpoint, param),
        "cross_endpoint_chain" => format!("Test cross_endpoint_chain on {}: chain operations across different endpoints, verify state consistency. Check param '{}'.", endpoint, param),
        "semantic_equivalence" => format!("Test semantic_equivalence on {}: compare two semantically equivalent operations, verify same results. Check param '{}'.", endpoint, param),
        "boundary_deepening" => format!("Test boundary_deepening on {}: explore edge cases around parameter boundaries with multi-step verification. Check param '{}'.", endpoint, param),
        _ => format!("Test {} pattern on endpoint {} with param '{}'.", pattern, endpoint, param),
    }
}

fn coverage_complete(coverage_tracker: &CoverageTracker, pattern_tracker: &PatternTracker) -> bool {
    coverage_tracker.unvisited_params().is_empty() && pattern_tracker.unexplored_patterns().is_empty()
}

fn detect_script_coverage(script: &str, endpoints: &[ApiEndpoint]) -> Vec<(String, String)> {
    endpoints.iter()
        .filter(|ep| script.contains(&ep.path))
        .flat_map(|ep| {
            let path = ep.path.clone();
            ep.params.iter().filter(move |param| {
                let p1 = format!("'{}'", param);
                let p2 = format!("\"{}\"", param);
                script.contains(&p1) || script.contains(&p2) || script.contains(param.as_str())
            }).map(move |param| (path.clone(), param.clone()))
        })
        .collect()
}

fn build_rescue_message(unvisited: &[(String, String)], coverage_tracker: &CoverageTracker) -> String {
    let mut lines = vec![
        "[COVERAGE RESCUE] The following params have NEVER appeared in any test script:".to_string(),
        String::new(),
    ];
    let mut by_endpoint: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for (ep, param) in unvisited {
        by_endpoint.entry(ep.clone()).or_default().push(param.clone());
    }
    for (ep, params) in &by_endpoint {
        lines.push(format!("  {} → {}", ep, params.join(", ")));
    }
    lines.push(String::new());
    lines.push("You MUST write a test script that calls these endpoints with ALL missing params included as JSON keys.".to_string());
    lines.push(String::new());
    for (ep, params) in &by_endpoint {
        let all_params: Vec<String> = coverage_tracker.endpoints.iter()
            .filter(|e| e.path == *ep)
            .flat_map(|e| e.params.iter().cloned())
            .collect();
        let mut template_parts: Vec<String> = Vec::new();
        for p in &all_params {
            if params.contains(p) {
                template_parts.push(format!("    \"{}\": <PROVIDE_VALUE>", p));
            } else {
                template_parts.push(format!("    \"{}\": ...", p));
            }
        }
        lines.push(format!("Example for {}:", ep));
        lines.push("{".to_string());
        lines.push(template_parts.join(",\n"));
        lines.push("}".to_string());
        lines.push(String::new());
    }
    lines.push("Include these params EXACTLY as shown. Do NOT omit any of them.".to_string());
    lines.join("\n")
}

fn should_truncate_messages(msg_count: usize) -> bool {
    msg_count > MESSAGE_HISTORY_MAX
}

fn build_turn_tools(_turn: usize, early_unlock: bool) -> Vec<crate::agent::llm::Tool> {
    if early_unlock {
        vec![get_execute_test_script_tool(), get_submit_mre_tool(), get_execute_stateful_test_tool(), get_execute_differential_test_tool(), get_coverage_report_tool()]
    } else {
        vec![get_execute_test_script_tool(), get_execute_stateful_test_tool(), get_execute_differential_test_tool(), get_coverage_report_tool()]
    }
}

impl<'a> FAOrchestrator<'a> {
    fn script_headers(&self) -> &'static str {
        self.plugin.script_headers()
    }

    fn script_success_check(&self, var: &str) -> String {
        self.plugin.script_success_check(var)
    }

    fn script_success_code(&self) -> &'static str {
        self.plugin.script_success_code()
    }

    fn script_api_helper(&self) -> String {
        self.plugin.script_api_helper()
    }

    fn build_script_preamble(&self, extra_imports: &str) -> String {
        let mut script = String::new();
        script.push_str("import requests, sys, uuid, time, json");
        if !extra_imports.is_empty() {
            script.push_str(&format!(", {}", extra_imports));
        }
        script.push('\n');
        script.push_str("BASE = '{{TESTVDB_DB_URL}}'\n");
        script.push_str(self.script_headers());
        script.push('\n');
        script.push_str(&self.script_api_helper());
        script
    }

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
                rejection_policies: std::collections::HashMap::new(),
                nested_params: std::collections::HashMap::new(),
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
            harness: RefCell::new(Harness::new()),
            processed_defect_keys: RefCell::new(HashSet::new()),
            early_mre_unlock: RefCell::new(false),
            baseline_records: RefCell::new(Vec::new()),
            baseline_output: None,
            conversation_log: RefCell::new(Vec::new()),
            diagnostic_dir: None,
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

    pub fn with_baseline_output(mut self, path: String) -> Self {
        self.baseline_output = Some(path);
        self
    }

    pub fn with_diagnostic_dir(mut self, dir: String) -> Self {
        self.diagnostic_dir = Some(dir);
        self
    }

    fn save_diagnostics(&self) {
        if let Some(ref dir) = self.diagnostic_dir {
            let _ = std::fs::create_dir_all(dir);
            let baseline_records = self.baseline_records.borrow().clone();
            if let Ok(json) = serde_json::to_string_pretty(&baseline_records) {
                let path = format!("{}/baseline_telemetry.json", dir);
                let _ = std::fs::write(&path, json);
                info!("Baseline telemetry saved to {} ({} records)", path, baseline_records.len());
            }
            let conversation = self.conversation_log.borrow().clone();
            if let Ok(json) = serde_json::to_string_pretty(&conversation) {
                let path = format!("{}/llm_conversation.json", dir);
                let _ = std::fs::write(&path, json);
                info!("Conversation log saved to {} ({} records)", path, conversation.len());
            }
        }
        if let Some(ref path) = self.baseline_output {
            let records = self.baseline_records.borrow().clone();
            if let Ok(json) = serde_json::to_string_pretty(&records) {
                let _ = std::fs::write(path, json);
                info!("Baseline telemetry saved to {} ({} records)", path, records.len());
            }
        }
    }

    fn record_turn(
        &self,
        turn: usize,
        task_endpoint: &str,
        task_pattern: &str,
        tool_name: &str,
        result_category: &str,
        script_code: Option<&str>,
        defect_type: Option<&str>,
        boundary_rejected: bool,
        state_check_methods: Vec<String>,
        oracle_violations_count: usize,
        stdout_preview: Option<&str>,
    ) {
        self.baseline_records.borrow_mut().push(TurnRecord {
            turn,
            tool_name: tool_name.to_string(),
            state_check_present: !state_check_methods.is_empty(),
            result_category: result_category.to_string(),
        });
        let script_truncated = script_code.map(|s| {
            let chars: Vec<char> = s.chars().take(2000).collect();
            chars.into_iter().collect()
        });
        let stdout_truncated = stdout_preview.map(|s| {
            let chars: Vec<char> = s.chars().take(500).collect();
            chars.into_iter().collect()
        });
        self.conversation_log.borrow_mut().push(ConversationRecord {
            turn,
            task_endpoint: task_endpoint.to_string(),
            task_pattern: task_pattern.to_string(),
            tool_name: tool_name.to_string(),
            script_code: script_truncated,
            result_category: result_category.to_string(),
            defect_type: defect_type.map(|s| s.to_string()),
            boundary_rejected,
            state_check_methods,
            oracle_violations_count,
            stdout_preview: stdout_truncated,
        });
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
            "milvus" => ctx.push_str("MILVUS: REST proxy silently passes unknown params; TTL/schema poorly validated; create-drop-create state loss. ACCEPTED: param gaps, type confusion. AVOID: upsert semantics.\n\
            \n=== MILVUS V2 API CHEAT SHEET (USE EXACTLY THESE FORMATS) ===\n\
            Create collection WITH index: POST /v2/vectordb/collections/create {\"collectionName\":\"c\",\"schema\":{\"autoID\":false,\"enableDynamicField\":true,\"fields\":[{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":true},{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{\"dim\":4}}]},\"indexParams\":[{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"AUTOINDEX\"}]}\n\
            Create index separately: POST /v2/vectordb/indexes/create {\"collectionName\":\"c\",\"indexParams\":[{\"fieldName\":\"vector\",\"metricType\":\"L2\",\"indexName\":\"vector_idx\",\"params\":{\"nlist\":1024}}]}\n\
            Insert: POST /v2/vectordb/entities/insert {\"collectionName\":\"c\",\"data\":[{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}]}\n\
            Search: POST /v2/vectordb/entities/search {\"collectionName\":\"c\",\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":5,\"outputFields\":[\"id\"]}\n\
            Query: POST /v2/vectordb/entities/query {\"collectionName\":\"c\",\"filter\":\"id > 0\",\"limit\":10,\"outputFields\":[\"id\"]}\n\
            Get stats: POST /v2/vectordb/collections/get_stats {\"collectionName\":\"c\"} → rowCount\n\
            CRITICAL: Always use indexParams array format for index creation. NEVER use indexType as top-level key. NEVER use params.nlist without wrapping in indexParams.\n\
            \n\
            === SEARCH_CORRECTNESS TEST PROCEDURE ===\n\
            1. Create collection, insert 10+ entities, create HNSW index, load\n\
            2. Search with searchParams={\"ef\":1} — record top-1 distance as d1\n\
            3. Search with searchParams={\"ef\":100} — record top-1 distance as d2\n\
            4. If d1 == d2 (or very close), ef parameter is IGNORED → [DEFECT: SEARCH_CORRECTNESS]\n\
            Also test: limit=3 but get 0 results despite having data → [DEFECT: SEARCH_CORRECTNESS]\n\
            \n\
            === CROSS_ENDPOINT TEST PROCEDURE ===\n\
            1. Insert N entities, load collection\n\
            2. Search with filter=\"id > 0\", limit=N → count results as search_count\n\
            3. Query with filter=\"id > 0\", limit=N → count results as query_count\n\
            4. If search_count ≠ query_count → [DEFECT: CROSS_ENDPOINT_INCONSISTENCY]\n\
            Also: create with ttl=X → describe shows ttl≠X → [DEFECT: CROSS_ENDPOINT_INCONSISTENCY]\n\
            \n\
            STATE_VIOLATION: insert N entities → get_stats rowCount must equal N. Delete M → rowCount must equal N-M. If not → [DEFECT: STATE_VIOLATION]\n"),
            "qdrant" => ctx.push_str("QDRANT: Async upsert silent discard (#9039); hnsw_ef=0 accepted; score_threshold edge cases. ACCEPTED: param boundaries, async gaps.\n"),
            "weaviate" => ctx.push_str("WEAVIATE: Schema validates some params but not others; dim mismatch->500; quantization silently normalized. ACCEPTED: validation gaps, silent normalization.\n"),
            "pgvector" => ctx.push_str("PGVECTOR: SQL-based vector DB. Param validation is PostgreSQL-grade. DON'T test single-param boundaries. Test: concurrent index builds, VACUUM+search, many indexes, iterative scans. ACCEPTED: race conditions, query plan issues.\n\
            \n=== PGVECTOR SCRIPT REQUIREMENTS ===\n\
            ALWAYS include these at the start of EVERY script:\n\
            import psycopg2, sys, uuid, time, os, re, threading\n\
            DB = os.environ.get('TESTVDB_DB_URL', 'http://localhost:5432')\n\
            host = re.search(r'http://([^:]+)', DB).group(1) if 'http' in DB else DB\n\
            conn = psycopg2.connect(dbname='testvdb', user='postgres', password='postgres', host=host, port=5432)\n\
            cur = conn.cursor()\n\
            cur.execute('CREATE EXTENSION IF NOT EXISTS vector')\n\
            conn.commit()\n\
            \n\
            For concurrent tests, each thread creates its OWN connection.\n\
            NEVER use requests library — use psycopg2 for SQL.\n\
            NEVER use {{TESTVDB_DB_URL}} directly in SQL — extract host from it.\n\
            \n\
            === PGVECTOR STATE INCONSISTENCY PATTERNS ===\n\
            1. INSERT N rows → COUNT(*) must equal N (if not → [DEFECT: STATE_VIOLATION])\n\
            2. DELETE M of N rows → COUNT(*) must equal N-M (if not → [DEFECT: STATE_VIOLATION])\n\
            3. INSERT + immediate search → inserted vector must be visible (if not → [DEFECT: STATE_VIOLATION])\n\
            4. Concurrent INSERT + DELETE → final COUNT must be consistent (if not → [DEFECT: DATA_CORRUPTION])\n\
            5. CREATE INDEX + DROP INDEX + search → search must still work (if not → [DEFECT: STATE_VIOLATION])\n\
            6. UPDATE vector → COUNT must not change (if changed → [DEFECT: STATE_VIOLATION])\n"),
            _ => {}
        }

        // ── Inject deterministic generator findings as DO-NOT-RETEST constraints ──
        if !self.batch_defects_summary.is_empty() {
            ctx.push_str("\n=== DETERMINISTIC GENERATORS FOUND THESE DEFECTS — EXPAND FROM THEM ===\n");
            ctx.push_str(&self.batch_defects_summary);
            ctx.push_str("\nFor EACH defect above, explore:\n");
            ctx.push_str("1. SEMANTIC EQUIVALENTS: nprobe(IVF) ↔ ef(HNSW) ↔ search_list(SCANN) — if one accepts invalid values, the other likely does too\n");
            ctx.push_str("2. SAME PARAM on DIFFERENT ENDPOINTS: search.nprobe=0 → query.nprobe=0; create.ttl → alter.ttl\n");
            ctx.push_str("3. SEARCH QUALITY IMPACT: does the invalid param cause WRONG search results? Verify top-k sorting and distances\n");
            ctx.push_str("4. CROSS-ENDPOINT CONSISTENCY: same operation via different API paths should give same result\n");
        }

        ctx.push_str("\nDIFFERENTIAL TEST EXAMPLE: create collection with ttl=-100, then alter_properties with ttl=-100. If create accepts but alter rejects, that's [DEFECT: CROSS_ENDPOINT_INCONSISTENCY].\n");
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
                .execute_test(&net.script, first, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command, self.plugin.auth_header_value().unwrap_or(""))
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
        let system_prompt = self.custom_system_prompt.clone()
            .unwrap_or_else(|| build_system_prompt(&self.contract_content));
        let mut coverage_tracker = CoverageTracker::new();

        let all_endpoints = self.plugin.all_api_endpoints();
        if !all_endpoints.is_empty() {
            for ep in &all_endpoints {
                coverage_tracker.register_endpoint(ep.clone());
            }
            info!("Coverage tracker: registered {} endpoints with {} total params",
                all_endpoints.len(), coverage_tracker.endpoint_count());
        } else {
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
        }

        let initial_msg = if let Some(ref custom_msg) = self.custom_initial_message {
            custom_msg.clone()
        } else {
            "Begin creative exploration. Use execute_stateful_test to test multi-step state sequences, or execute_test_script for concurrent, timing, and comparison tests. Focus on finding defects that deterministic generators cannot find.".to_string()
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

        let mut executor = FAExecutor::new(self.contract.rejection_policies.clone());
        let mut oracle_checks = self.plugin.derive_oracle_checks(&self.contract);
        let mutation_checks = Oracle::from_mutation_rules(
            &self.contract.behavioral_contracts.iter().flat_map(|c| c.mutation_rules.iter()).cloned().collect::<Vec<_>>(),
        );
        let mut_count = mutation_checks.len();
        oracle_checks.extend(mutation_checks);
        info!("Oracle initialized with {} checks ({} mutation rules, sorted by priority in Oracle::new)", oracle_checks.len(), mut_count);
        let mut oracle = Oracle::new(oracle_checks);
        let oracle_batch_size = 20;
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

        let mut last_coverage_count = 0usize;
        let mut no_progress_turns = 0usize;
        let mut rescue_turns: usize = 0;
        let mut no_tool_call_streak: usize = 0;
        let mut pattern_tracker = PatternTracker::new();
        let skipped_tasks: HashSet<String> = HashSet::new();
        let mut turn: usize = 0;

        loop {
            if turn >= self.max_turns {
                info!("Max turns ({}) reached. Stopping at turn {}.", self.max_turns, turn);
                break;
            }

            if turn > 0 && turn % 3 == 0 {
                executor.ensure_sandbox_healthy().await;
            }

            if coverage_complete(&coverage_tracker, &pattern_tracker) {
                info!("Coverage complete: all params visited and all patterns explored. Stopping at turn {}.", turn);
                info!("Final param coverage: {:.1}%, pattern coverage: {}/21", coverage_tracker.coverage_ratio() * 100.0, pattern_tracker.explored_patterns().len());
                break;
            }

            let current_task = select_next_task(&coverage_tracker, &pattern_tracker, &skipped_tasks);
            if current_task.is_none() {
                info!("No more tasks to explore (all candidates skipped or no gaps). Stopping at turn {}.", turn);
                break;
            }
            let task = current_task.expect("current_task guaranteed non-None by is_none check above");
            info!("Turn {}: assigned task endpoint='{}' pattern='{}'", turn + 1, task.endpoint, task.pattern);

            // P1: Truncate message history to prevent token overflow
            if should_truncate_messages(messages.len()) {
                let system_msg = messages.first().cloned();
                messages = messages.split_off(messages.len().saturating_sub(MESSAGE_HISTORY_KEEP));
                while messages.first().map_or(false, |m| m.role == "tool") {
                    messages.remove(0);
                }
                if let Some(sys) = system_msg {
                    messages.insert(0, sys);
                }
                info!("Truncated message history to {} messages (max 20)", messages.len());
            }

            // P2: Convergence detection — stop early if no new coverage for many turns
            let current_coverage = coverage_tracker.visited_count();
            let last_result_cat = self.baseline_records.borrow().last().map(|r| r.result_category.clone());
            if turn > 0 && current_coverage == last_coverage_count {
                if last_result_cat.as_deref() != Some("error") {
                    no_progress_turns += 1;
                }
            } else {
                no_progress_turns = 0;
            }
            last_coverage_count = current_coverage;

            if no_progress_turns >= 15 {
                let unvisited = coverage_tracker.unvisited_params();
                if unvisited.is_empty() {
                    info!("Convergence: no new coverage for {} turns, all params covered. Stopping at turn {}.", no_progress_turns, turn + 1);
                    break;
                }
                if rescue_turns < 5 {
                    rescue_turns += 1;
                    info!("Convergence with {} unvisited params → RESCUE mode ({}/5) at turn {}.", unvisited.len(), rescue_turns, turn + 1);
                    for (ep, param) in unvisited.iter().take(30) {
                        info!("  UNVISITED: {} / {}", ep, param);
                    }
                    no_progress_turns = 0;
                    let rescue_msg = build_rescue_message(&unvisited, &coverage_tracker);
                    messages.push(Message::user(rescue_msg));
                } else {
                    info!("Convergence: rescue mode exhausted (5/5). Stopping at turn {}.", turn + 1);
                    if !unvisited.is_empty() {
                        info!("Unvisited params at convergence ({} total):", unvisited.len());
                        for (ep, param) in unvisited.iter().take(30) {
                            info!("  UNVISITED: {} / {}", ep, param);
                        }
                    }
                    break;
                }
            }

            info!(
                "Agentic Exploration Turn {} (param coverage: {:.1}%, pattern coverage: {}/21)",
                turn + 1,
                coverage_tracker.coverage_ratio() * 100.0,
                pattern_tracker.explored_patterns().len()
            );

            // ── Mandatory task injection ──
            let coverage_report = coverage_tracker.report();
            let pattern_report = pattern_tracker.pattern_diversity_report();
            let unvisited_for_ep = coverage_tracker.unvisited_params_for_endpoint(&task.endpoint);
            let params_hint = if unvisited_for_ep.is_empty() {
                String::new()
            } else {
                let all_ep_params: Vec<String> = coverage_tracker.endpoints.iter()
                    .filter(|e| e.path == task.endpoint)
                    .flat_map(|e| e.params.iter().cloned())
                    .collect();
                let mut template_parts: Vec<String> = Vec::new();
                for p in &all_ep_params {
                    if unvisited_for_ep.contains(p) {
                        template_parts.push(format!("    \"{}\": <PROVIDE_VALUE>", p));
                    } else {
                        template_parts.push(format!("    \"{}\": ...", p));
                    }
                }
                format!(
                    "\n\nIMPORTANT: The following params on this endpoint are NOT yet covered: [{}]. \
                     You MUST include ALL of these params in your test script (as JSON keys in the request body).\
                     Do NOT skip any of them.\n\
                     Example request body for {}:\n{{\n{}\n}}",
                    unvisited_for_ep.join(", "), task.endpoint, template_parts.join(",\n")
                )
            };
            let mandatory_msg = format!(
                "[MANDATORY TASK] You MUST test the following:\n\
                 Endpoint: {}\n\
                 Pattern: {}\n\
                 Description: {}{}\n\
                 \n\
                 === COVERAGE STATUS ===\n\
                 {}\n\
                 {}\n\
                 \n\
                 You MUST use one of the available tools to test this. If the assigned pattern doesn't fit a structured tool, use execute_test_script.\
                 Do NOT test anything outside this assignment. Focus on the assigned endpoint and pattern.",
                task.endpoint, task.pattern, task.description, params_hint, coverage_report, pattern_report
            );
            messages.push(Message::user(mandatory_msg));

            if turn == 1 && executor.has_active_sandbox() {
                messages.push(Message::user(
                    "[HINT] Your database from the previous turn is still active and will be AUTOMATICALLY reused. \
                     Just write a script that operates on the existing data — no need to create a new collection. \
                     For example: delete points and verify count, or search and verify scores."
                        .to_string(),
                ));
            }

            if turn > 0 && turn % 5 == 0 && !all_safety_nets.is_empty() {
                let batch_end = (sn_next_idx + sn_batch_size).min(sn_total);
                if sn_next_idx < sn_total {
                    info!("Safety Net incremental batch at turn {}: probes {}-{} of {}", turn, sn_next_idx, batch_end - 1, sn_total);
                    let batch_nets: Vec<_> = all_safety_nets[sn_next_idx..batch_end].to_vec();
                    let first_probe = !executor.has_active_sandbox();
                    for (name, script, result) in self.execute_safety_nets(&mut executor, &batch_nets, first_probe).await? {
                        if result.found_defect {
                            sn_defect_names.push(name.clone());
                            if self.multi_defect {
                                collected_defects.push(CollectedDefect {
                                    script,
                                    evidence: generator::RunEvidence {
                                        phase: format!("safety_net_turn{}", turn),
                                        db_url: result.db_url.clone(),
                                        stdout: String::new(),
                                        stderr: String::new(),
                                        classifier_reason: result.classification.reason.clone(),
                                        classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
                                        exit_success: result.exit_success,
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
                            "\n\n[SAFETY NET] Found {} defect(s) so far: {}.",
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
            }

            let response_msg = match self
                .llm_client
                .send_chat_with_tools(messages.clone(), build_turn_tools(turn, *self.early_mre_unlock.borrow()))
                .await {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("LLM call failed at turn {}: {}. Skipping turn.", turn + 1, e);
                    turn += 1;
                    continue;
                }
            };
            messages.push(response_msg.clone());

            let Some(tool_calls) = response_msg.tool_calls else {
                no_tool_call_streak += 1;
                if no_tool_call_streak >= 5 {
                    info!("LLM has not called any tool for {} consecutive turns. Stopping.", no_tool_call_streak);
                    break;
                }
                if no_tool_call_streak >= 2 {
                    let nudge = "You MUST call a tool now. Use execute_test_script, execute_stateful_test, or execute_differential_test to continue testing. Do not just explain — act.";
                    messages.push(Message::user(nudge.to_string()));
                    warn!("LLM did not call any tool (streak={}). Injecting nudge.", no_tool_call_streak);
                }
                continue;
            };
            no_tool_call_streak = 0;
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

                    // ── Boundary test rejection: redirect pure single-param tests ──
                    let boundary_params = ["limit=-1", "limit=0", "offset=-1", "offset=0", "nprobe=-1", "nprobe=0", "shardsNum=null", "shardsNum=0", "shard_number=0", "replication_factor=-1", "ef=-1", "ef=0", "hnsw_ef=-1", "hnsw_ef=0", "Authorization=null"];
                    let boundary_hits: Vec<_> = boundary_params.iter().filter(|p| code.contains(*p)).collect();
                    if should_reject_boundary_test(code, &self.batch_defects_summary) {
                        info!("Rejecting boundary test (turn {}): found params {:?}", turn, boundary_hits);
                        self.record_turn(turn, &task.endpoint, &task.pattern, "execute_test_script", "rejection", Some(code), None, true, Vec::new(), 0, None);
                        messages.push(Message::tool_response(
                            &tc.id,
                            format!(
                                "REJECTED: This looks like a single-parameter boundary test (detected: {:?}).\n\
                                 Boundary fuzzing is already covered by deterministic generators.\n\
                                 Instead, use execute_stateful_test to test STATE after sequences, or execute_test_script for concurrent/timing tests.\n\
                                 You have access to these tools — use them now.", boundary_hits),
                        ));
                        continue;
                    }
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
                        .execute_test(code, fresh_sandbox, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command, self.plugin.auth_header_value().unwrap_or(""))
                        .await?;

                    messages.push(Message::tool_response(&tc.id, &result.output));

                    let coverage_hits = detect_script_coverage(&code, &coverage_tracker.endpoints);
                    for (path, param) in coverage_hits { coverage_tracker.record_visit(&path, &param, "executed"); }

                    if let Some(sandbox) = result.sandbox {
                        info!("Got sandbox from ExecutionResult (fresh), oracle_pending={}", oracle.has_pending());
                        if oracle.has_pending() && !result.db_url.is_empty() {
                            info!("Running Oracle checks in sandbox (batch of {})...", oracle_batch_size);
                            let oracle_findings = oracle
                                .run_next_batch(&sandbox, &result.db_url, oracle_batch_size, self.plugin.auth_header_value().unwrap_or(""))
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
                        let sandbox = executor.take_sandbox().ok_or_else(|| anyhow::anyhow!("no active sandbox for Oracle despite has_active_sandbox check"))?;
                        let oracle_findings = oracle
                            .run_next_batch(&sandbox, &result.db_url, oracle_batch_size, self.plugin.auth_header_value().unwrap_or(""))
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

                    // ── US-1.2a: Auto defect detection on [DEFECT: ...] in stdout ──
                    let stdout = &result.stdout;
                    if stdout.contains("[DEFECT:") || stdout.contains("[DEFECT ") {
                        let result_key = format!("{}:{}", turn, code.len());
                        if !self.processed_defect_keys.borrow().contains(&result_key) {
                            self.processed_defect_keys.borrow_mut().insert(result_key.clone());

                            let classification = result.classification.clone();
                            if classification.disposition == ClassificationDisposition::CandidateDefect {
                                let defect_type_str = classification.defect_type.as_ref()
                                    .map(|dt| format!("{:?}", dt))
                                    .unwrap_or_else(|| "unknown".to_string());
                                let test_case_summary = format!("auto_detect turn {}", turn + 1);

                                info!("Auto defect detected: [{}] — triggering harness review", defect_type_str);
                                let review_record = self.harness.borrow_mut().review_or_fallback(
                                    self.llm_client,
                                    stdout,
                                    &result.stderr,
                                    &defect_type_str,
                                    code,
                                    &test_case_summary,
                                ).await;

                                match review_record.outcome {
                                    ReviewOutcome::ConfirmedDefect | ReviewOutcome::Uncertain => {
                                        info!("Auto defect harness outcome: {:?} (confidence={:.2}) — collecting", review_record.outcome, review_record.analysis.confidence);
                                        let initial_run = generator::RunEvidence {
                                            phase: "auto_detect".to_string(),
                                            db_url: result.db_url.clone(),
                                            stdout: String::new(),
                                            stderr: String::new(),
                                            classifier_reason: classification.reason.clone(),
                                            classifier_evidence_excerpt: classification.evidence_excerpt.clone(),
                                            exit_success: result.exit_success,
                                        };
                                        if let Some(ret) = collect_or_return(self.multi_defect, &mut collected_defects, code.to_string(), initial_run, classification) {
                                            self.save_diagnostics();
                                            return Ok(ret);
                                        }
                                        *self.early_mre_unlock.borrow_mut() = true;
                                        messages.push(Message::user(
                                            format!("[AUTO-DETECT] Defect [{}] detected and confirmed (confidence: {:.2}). submit_mre is now unlocked. Continue exploring or submit your MRE.",
                                                defect_type_str, review_record.analysis.confidence),
                                        ));
                                    }
                                    ReviewOutcome::FalsePositive => {
                                        info!("Auto defect harness: FALSE POSITIVE (confidence={:.2}) — skipping", review_record.analysis.confidence);
                                    }
                                }
                            }
                        } else {
                            info!("Auto defect: skipping duplicate detection for key {}", result_key);
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

                    self.record_turn(turn, &task.endpoint, &task.pattern, "execute_test_script", &classify_result(&result.stdout, result.exit_success), Some(code), None, false, Vec::new(), 0, Some(&result.stdout));
                }
                "submit_mre" => {
                    if !*self.early_mre_unlock.borrow() {
                        self.record_turn(turn, &task.endpoint, &task.pattern, "submit_mre", "rejection", None, None, false, Vec::new(), 0, None);
                        messages.push(Message::tool_response(
                            &tc.id,
                            "REJECTED: You must find a defect first before submitting. Continue testing the assigned task.".to_string(),
                        ));
                        continue;
                    }

                    info!("Agent submitted MRE. Running all remaining Oracle checks before final validation...");
                    while oracle.has_pending() {
                        if let Some(sandbox) = executor.take_sandbox() {
                            let db_url = crate::infra::build_db_url(sandbox.db_host.as_deref().unwrap_or("localhost"), self.db_port);
                            let violations = oracle.run_next_batch(&sandbox, &db_url, oracle_batch_size, self.plugin.auth_header_value().unwrap_or("")).await;
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
                        self.record_turn(turn, &task.endpoint, &task.pattern, "submit_mre", "rejection", None, None, false, Vec::new(), oracle_violations.len(), None);
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
                    let mut result = executor
                        .execute_test(&code, true, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command, self.plugin.auth_header_value().unwrap_or(""))
                        .await?;

                    let classification = result.classification.clone();

                    // ── Harness review: LLM-driven second opinion on defect validity ──
                    let defect_type_str = classification.defect_type.as_ref()
                        .map(|dt| format!("{:?}", dt))
                        .unwrap_or_else(|| "unknown".to_string());
                    let test_case_summary = format!("MRE submit turn {}", turn + 1);
                    let review_record = self.harness.borrow_mut().review_or_fallback(
                        self.llm_client,
                        &result.stdout,
                        &result.stderr,
                        &defect_type_str,
                        &code,
                        &test_case_summary,
                    ).await;

                    result.classification.llm_review = Some(LlmReview {
                        is_true_defect: review_record.analysis.is_real_defect,
                        confidence: review_record.analysis.confidence,
                        explanation: review_record.analysis.root_cause.clone(),
                    });

                    if review_record.outcome == ReviewOutcome::FalsePositive {
                        info!("Harness review: FALSE POSITIVE (confidence={:.2}) — skipping defect collection", review_record.analysis.confidence);
                        self.record_turn(turn, &task.endpoint, &task.pattern, "submit_mre", "error", Some(&code), Some(&defect_type_str), false, Vec::new(), 0, Some(&result.stdout));
                        messages.push(Message::tool_response(
                            &tc.id,
                            &format!("Harness review determined this is a FALSE POSITIVE (confidence: {:.2}). Root cause: {}. Continue exploring.",
                                review_record.analysis.confidence, review_record.analysis.root_cause),
                        ));
                        continue;
                    }

                    info!("Harness review outcome: {:?} (confidence={:.2})", review_record.outcome, review_record.analysis.confidence);

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
                            exit_success: result.exit_success,
                        };
                            if self.multi_defect {
                                collected_defects.push(CollectedDefect {
                                    script,
                                    evidence: initial_run,
                                    classification: result.classification,
                                });
                            } else {
                                self.save_diagnostics();
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
                            llm_review: None,
                        };
                        let initial_run = generator::RunEvidence {
                            phase: "oracle".to_string(),
                            db_url: result.db_url.clone(),
                            stdout: String::new(),
                            stderr: String::new(),
                            classifier_reason: oracle_classification.reason.clone(),
                            classifier_evidence_excerpt: oracle_classification.evidence_excerpt.clone(),
                            exit_success: result.exit_success,
                        };
                        if let Some(ret) = collect_or_return(self.multi_defect, &mut collected_defects, code.clone(), initial_run, oracle_classification) {
                            self.save_diagnostics();
                            return Ok(ret);
                        }
                        self.record_turn(turn, &task.endpoint, &task.pattern, "submit_mre", "success", Some(&code), Some(&defect_type_str), false, Vec::new(), oracle_violations.len(), Some(&result.stdout));
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
                        exit_success: result.exit_success,
                    };
                    let defect_type_for_log = classification.defect_type.as_ref().map(|dt| format!("{:?}", dt));
                    if let Some(ret) = collect_or_return(self.multi_defect, &mut collected_defects, code.clone(), initial_run, classification) {
                        self.save_diagnostics();
                        return Ok(ret);
                    }
                    self.record_turn(turn, &task.endpoint, &task.pattern, "submit_mre", "success", Some(&code), defect_type_for_log.as_deref(), false, Vec::new(), 0, Some(&result.stdout));
                    messages.push(Message::tool_response(&tc.id, "Defect collected. Continue exploring for more defects. Try a different endpoint or parameter."));
                    continue;
                }

                "execute_stateful_test" => {
                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let test_name = args.get("test_name").and_then(|v| v.as_str()).unwrap_or("unnamed");
                    let steps = args.get("steps").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                    let invariant = args.get("invariant").and_then(|v| v.as_str()).unwrap_or("");

                    let mut script = format!("# Stateful Test: {}\n", test_name);
                    script.push_str(&self.build_script_preamble(""));

                    for (i, step) in steps.iter().enumerate() {
                        let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("/");
                        let params = step.get("params").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        let expect_success = step.get("expect_success").and_then(|v| v.as_bool()).unwrap_or(true);
                        let params_str = serde_json::to_string(&params).unwrap_or_default();

                        script.push_str(&format!("# Step {}: {}\n", i + 1, action));
                        script.push_str(&format!("r{} = {}\n", i + 1, self.plugin.script_api_call(action, &params_str)));
                        if expect_success {
                            script.push_str(&format!("if {}: print('[DEFECT: STATE_LOGIC_VIOLATION] Step {} (action={}) expected success but got:', r{}); sys.exit(1)\n", self.script_success_check(&format!("r{}", i + 1)), i + 1, action, i + 1));
                        } else {
                            script.push_str(&format!("if not {}: print('[DEFECT: STATE_LOGIC_VIOLATION] Step {} (action={}) expected error but succeeded'); sys.exit(1)\n", self.script_success_check(&format!("r{}", i + 1)), i + 1, action));
                        }
                        if let Some(check) = step.get("state_check") {
                            let method = check.get("method").and_then(|v| v.as_str()).unwrap_or("");
                            let expected = check.get("expected").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                            let check_code = crate::agent::tools::generate_state_check_code(method, &expected, &params, self.plugin.target_style());
                            script.push_str(&check_code);
                        }
                        script.push_str("time.sleep(0.3)\n\n");
                    }
                    if !invariant.is_empty() {
                        script.push_str(&format!("# Final invariant: {}\n", invariant));
                        script.push_str("print(f'Invariant checked')\n");
                    }
                    script.push_str("print('All stateful test steps passed.')\nsys.exit(0)\n");

                    let fresh_sandbox = !executor.has_active_sandbox();
                    let result = executor.execute_test(&script, fresh_sandbox, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command, self.plugin.auth_header_value().unwrap_or("")).await?;
                    messages.push(Message::tool_response(&tc.id, &result.output));

                    let coverage_hits = detect_script_coverage(&script, &coverage_tracker.endpoints); for (path, param) in coverage_hits { coverage_tracker.record_visit(&path, &param, "executed"); };

                    if let Some(sandbox) = result.sandbox {
                        if oracle.has_pending() && !result.db_url.is_empty() {
                            let oracle_findings = oracle
                                .run_next_batch(&sandbox, &result.db_url, oracle_batch_size, self.plugin.auth_header_value().unwrap_or(""))
                                .await;
                            if !oracle_findings.is_empty() {
                                let violated_count = oracle_findings.iter().filter(|f| f.violated).count();
                                if violated_count > 0 {
                                    info!("Oracle found {} violation(s) after stateful test!", violated_count);
                                }
                                executor.state.record_oracle_findings(oracle_findings.clone());
                                let oracle_msg = build_oracle_findings_message(&oracle_findings);
                                if !oracle_msg.is_empty() {
                                    messages.push(Message::user(oracle_msg));
                                }
                            }
                        }
                        executor.put_sandbox(sandbox);
                    } else if executor.has_active_sandbox() && oracle.has_pending() && !result.db_url.is_empty() {
                        let sandbox = executor.take_sandbox().ok_or_else(|| anyhow::anyhow!("no active sandbox for Oracle despite has_active_sandbox check"))?;
                        let oracle_findings = oracle
                            .run_next_batch(&sandbox, &result.db_url, oracle_batch_size, self.plugin.auth_header_value().unwrap_or(""))
                            .await;
                        if !oracle_findings.is_empty() {
                            let violated_count = oracle_findings.iter().filter(|f| f.violated).count();
                            if violated_count > 0 {
                                info!("Oracle found {} violation(s) after stateful test!", violated_count);
                            }
                            executor.state.record_oracle_findings(oracle_findings.clone());
                            let oracle_msg = build_oracle_findings_message(&oracle_findings);
                            if !oracle_msg.is_empty() {
                                messages.push(Message::user(oracle_msg));
                            }
                        }
                        executor.put_sandbox(sandbox);
                    }

                    let stdout = &result.stdout;
                    if stdout.contains("[DEFECT:") || stdout.contains("[DEFECT ") {
                        let result_key = format!("{}:{}", turn, script.len());
                        if !self.processed_defect_keys.borrow().contains(&result_key) {
                            self.processed_defect_keys.borrow_mut().insert(result_key.clone());
                            let classification = result.classification.clone();
                            if classification.disposition == ClassificationDisposition::CandidateDefect {
                                let defect_type_str = classification.defect_type.as_ref()
                                    .map(|dt| format!("{:?}", dt))
                                    .unwrap_or_else(|| "unknown".to_string());
                                let test_case_summary = format!("stateful_auto_detect turn {}", turn + 1);
                                info!("Auto defect detected in stateful test: [{}] — triggering harness review", defect_type_str);
                                let review_record = self.harness.borrow_mut().review_or_fallback(
                                    self.llm_client,
                                    stdout,
                                    &result.stderr,
                                    &defect_type_str,
                                    &script,
                                    &test_case_summary,
                                ).await;
                                match review_record.outcome {
                                    ReviewOutcome::ConfirmedDefect | ReviewOutcome::Uncertain => {
                                        info!("Stateful auto defect harness outcome: {:?} (confidence={:.2}) — collecting", review_record.outcome, review_record.analysis.confidence);
                                        let initial_run = generator::RunEvidence {
                                            phase: "stateful_auto_detect".to_string(),
                                            db_url: result.db_url.clone(),
                                            stdout: String::new(),
                                            stderr: String::new(),
                                            classifier_reason: classification.reason.clone(),
                                            classifier_evidence_excerpt: classification.evidence_excerpt.clone(),
                                            exit_success: result.exit_success,
                                        };
                                        if let Some(ret) = collect_or_return(self.multi_defect, &mut collected_defects, script.clone(), initial_run, classification) {
                                            self.save_diagnostics();
                                            return Ok(ret);
                                        }
                                        *self.early_mre_unlock.borrow_mut() = true;
                                        messages.push(Message::user(
                                            format!("[AUTO-DETECT] Defect [{}] detected in stateful test (confidence: {:.2}). submit_mre is now unlocked.",
                                                defect_type_str, review_record.analysis.confidence),
                                        ));
                                    }
                                    ReviewOutcome::FalsePositive => {
                                        info!("Stateful auto defect harness: FALSE POSITIVE (confidence={:.2})", review_record.analysis.confidence);
                                    }
                                }
                            }
                        }
                    }

                    if executor.error_state.should_intervene() {
                        warn!(
                            "Agent hit the same error {} times in stateful test. Injecting SYSTEM INTERVENTION.",
                            executor.error_state.consecutive_same_errors
                        );
                        messages.push(Message::user(
                            "[SYSTEM INTERVENTION] You have failed with similar errors 3 times in stateful tests. You must change your approach entirely or stop.",
                        ));
                        executor.error_state.reset();
                    }

                    let state_check_methods: Vec<String> = steps.iter()
                        .filter_map(|s| s.get("state_check").and_then(|c| c.get("method")).and_then(|m| m.as_str()).map(|m| m.to_string()))
                        .collect();
                    self.record_turn(turn, &task.endpoint, &task.pattern, "execute_stateful_test", &classify_result(&result.stdout, result.exit_success), Some(&script), None, false, state_check_methods, 0, Some(&result.stdout));
                }

                "execute_differential_test" => {
                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                    let test_type = args.get("test_type").and_then(|v| v.as_str()).unwrap_or("search_correctness");
                    let setup_code = args.get("setup_code").and_then(|v| v.as_str()).unwrap_or("");
                    let call_a_label = args.get("call_a_label").and_then(|v| v.as_str()).unwrap_or("call_a");
                    let call_a_code = args.get("call_a_code").and_then(|v| v.as_str()).unwrap_or("");
                    let call_b_label = args.get("call_b_label").and_then(|v| v.as_str()).unwrap_or("call_b");
                    let call_b_code = args.get("call_b_code").and_then(|v| v.as_str()).unwrap_or("");
                    let comparison = args.get("comparison").and_then(|v| v.as_str()).unwrap_or("should_differ");

                    let defect_marker = match test_type {
                        "cross_endpoint_consistency" => "CROSS_ENDPOINT_INCONSISTENCY",
                        _ => "SEARCH_CORRECTNESS",
                    };

                    let comparison_logic = if comparison == "should_differ" {
                        format!(
                            "if result_a == result_b:\n\
                             \x20   print('[DEFECT: {}] {} and {} returned identical results: {{}} vs {{}}'.format(result_a, result_b))\n\
                             \x20   sys.exit(1)\n\
                             else:\n\
                             \x20   print('OK: {}={{}} differs from {}={{}} (as expected)'.format(result_a, result_b))\n\
                             \x20   sys.exit(0)\n",
                            defect_marker, call_a_label, call_b_label, call_a_label, call_b_label
                        )
                    } else {
                        format!(
                            "if result_a != result_b:\n\
                             \x20   print('[DEFECT: {}] {} and {} returned different results: {{}} vs {{}}'.format(result_a, result_b))\n\
                             \x20   sys.exit(1)\n\
                             else:\n\
                             \x20   print('OK: {}={{}} matches {}={{}} (as expected)'.format(result_a, result_b))\n\
                             \x20   sys.exit(0)\n",
                            defect_marker, call_a_label, call_b_label, call_a_label, call_b_label
                        )
                    };

                    let mut script = format!("# Differential Test: {}\n", test_type);
                    script.push_str(&self.build_script_preamble(""));
                    script.push_str(setup_code);
                    script.push_str("\nprint('SETUP_OK')\ntime.sleep(0.5)\n\n");
                    script.push_str(&format!("# Call A: {}\n", call_a_label));
                    script.push_str(call_a_code);
                    script.push_str("\nprint('{} = {}'.format(result_a))\n\n");
                    script.push_str(&format!("# Call B: {}\n", call_b_label));
                    script.push_str(call_b_code);
                    script.push_str("\nprint('{} = {}'.format(result_b))\n\n");
                    script.push_str("# Comparison\n");
                    script.push_str(&comparison_logic);

                    let fresh_sandbox = !executor.has_active_sandbox();
                    let result = executor.execute_test(&script, fresh_sandbox, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command, self.plugin.auth_header_value().unwrap_or("")).await?;
                    messages.push(Message::tool_response(&tc.id, &result.output));

                    let coverage_hits = detect_script_coverage(&script, &coverage_tracker.endpoints);
                    for (path, param) in coverage_hits { coverage_tracker.record_visit(&path, &param, "executed"); }

                    if let Some(sandbox) = result.sandbox {
                        if oracle.has_pending() && !result.db_url.is_empty() {
                            let oracle_findings = oracle.run_next_batch(&sandbox, &result.db_url, oracle_batch_size, self.plugin.auth_header_value().unwrap_or("")).await;
                            if !oracle_findings.is_empty() {
                                let violated_count = oracle_findings.iter().filter(|f| f.violated).count();
                                if violated_count > 0 {
                                    info!("Oracle found {} violation(s) after differential test!", violated_count);
                                }
                                executor.state.record_oracle_findings(oracle_findings.clone());
                                let oracle_msg = build_oracle_findings_message(&oracle_findings);
                                if !oracle_msg.is_empty() {
                                    messages.push(Message::user(oracle_msg));
                                }
                            }
                        }
                        executor.put_sandbox(sandbox);
                    } else if executor.has_active_sandbox() && oracle.has_pending() && !result.db_url.is_empty() {
                        let sandbox = executor.take_sandbox().ok_or_else(|| anyhow::anyhow!("no active sandbox for Oracle despite has_active_sandbox check"))?;
                        let oracle_findings = oracle.run_next_batch(&sandbox, &result.db_url, oracle_batch_size, self.plugin.auth_header_value().unwrap_or("")).await;
                        if !oracle_findings.is_empty() {
                            let violated_count = oracle_findings.iter().filter(|f| f.violated).count();
                            if violated_count > 0 {
                                info!("Oracle found {} violation(s) after differential test!", violated_count);
                            }
                            executor.state.record_oracle_findings(oracle_findings.clone());
                            let oracle_msg = build_oracle_findings_message(&oracle_findings);
                            if !oracle_msg.is_empty() {
                                messages.push(Message::user(oracle_msg));
                            }
                        }
                        executor.put_sandbox(sandbox);
                    }

                    let stdout = &result.stdout;
                    if stdout.contains("[DEFECT:") || stdout.contains("[DEFECT ") {
                        let result_key = format!("{}:{}", turn, script.len());
                        if !self.processed_defect_keys.borrow().contains(&result_key) {
                            self.processed_defect_keys.borrow_mut().insert(result_key.clone());
                            let classification = result.classification.clone();
                            if classification.disposition == ClassificationDisposition::CandidateDefect {
                                let defect_type_str = classification.defect_type.as_ref()
                                    .map(|dt| format!("{:?}", dt))
                                    .unwrap_or_else(|| "unknown".to_string());
                                let test_case_summary = format!("differential_auto_detect turn {}", turn + 1);
                                info!("Auto defect detected in differential test: [{}] — triggering harness review", defect_type_str);
                                let review_record = self.harness.borrow_mut().review_or_fallback(
                                    self.llm_client,
                                    stdout,
                                    &result.stderr,
                                    &defect_type_str,
                                    &script,
                                    &test_case_summary,
                                ).await;
                                match review_record.outcome {
                                    ReviewOutcome::ConfirmedDefect | ReviewOutcome::Uncertain => {
                                        info!("Differential auto defect harness outcome: {:?} (confidence={:.2}) — collecting", review_record.outcome, review_record.analysis.confidence);
                                        let initial_run = generator::RunEvidence {
                                            phase: "differential_auto_detect".to_string(),
                                            db_url: result.db_url.clone(),
                                            stdout: String::new(),
                                            stderr: String::new(),
                                            classifier_reason: classification.reason.clone(),
                                            classifier_evidence_excerpt: classification.evidence_excerpt.clone(),
                                            exit_success: result.exit_success,
                                        };
                                        if let Some(ret) = collect_or_return(self.multi_defect, &mut collected_defects, script.clone(), initial_run, classification) {
                                            self.save_diagnostics();
                                            return Ok(ret);
                                        }
                                        *self.early_mre_unlock.borrow_mut() = true;
                                        messages.push(Message::user(
                                            format!("[AUTO-DETECT] Defect [{}] detected in differential test (confidence: {:.2}). submit_mre is now unlocked.",
                                                defect_type_str, review_record.analysis.confidence),
                                        ));
                                    }
                                    ReviewOutcome::FalsePositive => {
                                        info!("Differential auto defect harness: FALSE POSITIVE (confidence={:.2})", review_record.analysis.confidence);
                                    }
                                }
                            }
                        }
                    }

                    if executor.error_state.should_intervene() {
                        warn!("Agent hit the same error {} times in differential test. Injecting SYSTEM INTERVENTION.", executor.error_state.consecutive_same_errors);
                        messages.push(Message::user(
                            "[SYSTEM INTERVENTION] You have failed with similar errors 3 times in differential tests. You must change your approach entirely or stop.",
                        ));
                        executor.error_state.reset();
                    }

                    self.record_turn(turn, &task.endpoint, &task.pattern, "execute_differential_test", &classify_result(&result.stdout, result.exit_success), Some(&script), None, false, Vec::new(), 0, Some(&result.stdout));
                }

                "get_coverage_report" => {
                    let report = coverage_tracker.report();
                    info!("get_coverage_report: {} entries tracked", coverage_tracker.visited_count());
                    messages.push(Message::tool_response(&tc.id, &report));

                    self.record_turn(turn, &task.endpoint, &task.pattern, "get_coverage_report", "success", None, None, false, Vec::new(), 0, None);
                }
                _ => {
                    messages.push(Message::tool_response(&tc.id, "Unknown tool."));
                }
            }

            // Record pattern from tool call arguments or mandatory task
            let args: serde_json::Value =
                serde_json::from_str(&tool_calls[0].function.arguments).unwrap_or_default();
            if let Some(pattern) = args.get("pattern_category").and_then(|v| v.as_str()) {
                pattern_tracker.record_pattern(pattern);
                info!("Recorded pattern '{}' from tool args (explored: {}/21)", pattern, pattern_tracker.explored_patterns().len());
            } else {
                pattern_tracker.record_pattern(&task.pattern);
                info!("Recorded pattern '{}' from mandatory task (explored: {}/21)", task.pattern, pattern_tracker.explored_patterns().len());
            }

            // ── Inject harness review context for next turn ──
            let review_ctx = self.harness.borrow().build_review_context();
            if !review_ctx.is_empty() {
                messages.push(Message::user(review_ctx));
            }

            turn += 1;
        }

        if let Some(code) = executor.last_test_code.clone() {
            info!("Coverage-driven exploration completed after {} turns. Submitting last test script as MRE.", turn);
            let result = executor
                .execute_test(&code, true, &self.db_image, &self.pip_packages, self.db_port, &self.sidecars, &self.db_env, &self.db_command, self.plugin.auth_header_value().unwrap_or(""))
                .await?;
            let initial_run = generator::RunEvidence {
                phase: "initial".to_string(),
                db_url: result.db_url.clone(),
                stdout: String::new(),
                stderr: String::new(),
                classifier_reason: result.classification.reason.clone(),
                classifier_evidence_excerpt: result.classification.evidence_excerpt.clone(),
                exit_success: result.exit_success,
            };
            self.save_diagnostics();
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
                    exit_success: result.exit_success,
                };
                self.save_diagnostics();
                return Ok((script, initial_run, result.classification, collected_defects));
            }
            warn!("Safety net '{}' did not trigger (properly rejected).", name);
        }
        if !collected_defects.is_empty() {
            let first = collected_defects.remove(0);
            self.save_diagnostics();
            return Ok((first.script, first.evidence, first.classification, collected_defects));
        }
        anyhow::bail!("No defect found by FA or any safety net probe");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_truncation() {
        assert!(should_truncate_messages(21));
        assert!(should_truncate_messages(30));
        assert!(!should_truncate_messages(20));
        assert!(!should_truncate_messages(19));
    }

    // ── US-1.2a: Auto defect detection tests ──

    #[test]
    fn test_auto_defect_detection() {
        // Verify classifier returns CandidateDefect for stdout containing [DEFECT: ...]
        let stdout = "[DEFECT: STATE_LOGIC_VIOLATION] rowCount mismatch: expected 10, got 0";
        let stderr = "";
        let result = crate::agent::classifier::analyze_execution_result(stdout, stderr, None);
        assert_eq!(result.disposition, ClassificationDisposition::CandidateDefect);
        assert!(result.defect_type.is_some());
    }

    #[test]
    fn test_auto_defect_no_defect_signal() {
        // Verify classifier does NOT return CandidateDefect for clean stdout
        let stdout = "All tests passed. rowCount=10";
        let stderr = "";
        let result = crate::agent::classifier::analyze_execution_result(stdout, stderr, None);
        assert_ne!(result.disposition, ClassificationDisposition::CandidateDefect);
    }

    #[test]
    fn test_auto_defect_dedup() {
        // Verify dedup: same (turn, code_len) key is only processed once
        let mut keys: HashSet<String> = HashSet::new();
        let key1 = format!("{}:{}", 3, 100);
        assert!(keys.insert(key1.clone()));
        assert!(!keys.insert(key1)); // duplicate rejected
        let key2 = format!("{}:{}", 3, 200); // different code len
        assert!(keys.insert(key2));
    }

    #[test]
    fn test_conditional_mre_unlock_before_threshold() {
        let tools = build_turn_tools(0, false);
        assert!(!tools.iter().any(|t| t.function.name == "submit_mre"));
        let tools = build_turn_tools(5, false);
        assert!(!tools.iter().any(|t| t.function.name == "submit_mre"));
    }

    #[test]
    fn test_conditional_mre_unlock_after_defect() {
        let tools = build_turn_tools(0, true);
        assert!(tools.iter().any(|t| t.function.name == "submit_mre"));
    }

    #[test]
    fn test_conditional_mre_unlock_early() {
        let tools = build_turn_tools(0, true);
        assert!(tools.iter().any(|t| t.function.name == "submit_mre"));
        let tools = build_turn_tools(4, true);
        assert!(tools.iter().any(|t| t.function.name == "submit_mre"));
    }

    #[test]
    fn test_build_system_prompt_format() {
        let contract = "api_endpoint: /v2/vectordb/collections/create";
        let prompt = build_system_prompt(contract);
        assert!(prompt.contains("Test STATE"));
        assert!(prompt.contains("State consistency"));
        assert!(prompt.contains("Concurrent races"));
        assert!(prompt.contains("TIMING"));
        assert!(prompt.contains("SEARCH_CORRECTNESS"));
        assert!(prompt.contains("CROSS_ENDPOINT"));
        assert!(prompt.contains("Search correctness"));
        assert!(prompt.contains("Cross-endpoint consistency"));
        assert!(prompt.contains("PATTERNS"));
        assert!(prompt.contains("search_correctness"));
        assert!(prompt.contains("cross_endpoint_chain"));
        assert!(prompt.contains("DEFECT: SEARCH_CORRECTNESS"));
        assert!(prompt.contains("DEFECT: CROSS_ENDPOINT_INCONSISTENCY"));
        assert!(prompt.contains("DEFECT: CONCURRENT_RACE"));
        let fixed_part = prompt.split("=== CONTRACT ===").next().unwrap_or("");
        assert!(fixed_part.len() < 3000, "Fixed part is {} chars, expected < 3000", fixed_part.len());
    }

    #[test]
    fn test_build_system_prompt_includes_contract() {
        let contract = "=== CONTRACT ===\napi_endpoint: /v2/vectordb/entities/delete\nlimit: integer, range [1, 16384]";
        let prompt = build_system_prompt(contract);
        assert!(prompt.contains(contract));
    }

    #[test]
    fn test_build_system_prompt_empty_contract() {
        let prompt = build_system_prompt("");
        assert!(prompt.contains("Test STATE"));
        assert!(prompt.contains("MISSION"));
    }

    #[test]
    fn test_build_turn_tools_coverage_driven() {
        let tools = build_turn_tools(0, false);
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().any(|t| t.function.name == "execute_stateful_test"));
        assert!(tools.iter().any(|t| t.function.name == "execute_test_script"));
        assert!(tools.iter().any(|t| t.function.name == "execute_differential_test"));
        assert!(tools.iter().any(|t| t.function.name == "get_coverage_report"));
        assert!(!tools.iter().any(|t| t.function.name == "submit_mre"));
    }

    #[test]
    fn test_build_turn_tools_with_early_unlock() {
        let tools = build_turn_tools(0, true);
        assert_eq!(tools.len(), 5);
        assert!(tools.iter().any(|t| t.function.name == "submit_mre"));
        assert!(tools.iter().any(|t| t.function.name == "execute_differential_test"));
    }

    // ── Mock infrastructure for FAOrchestrator method tests ──

    struct MockProbeTemplate;

    impl crate::agent::probe::ProbeTemplate for MockProbeTemplate {
        fn search_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn create_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn upsert_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn delete_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn scroll_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn recommend_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn search_params_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn update_config_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
        fn preamble(&self) -> &str { "" }
    }

    struct MockTargetPlugin {
        name: &'static str,
        probe: MockProbeTemplate,
    }

    impl TargetPlugin for MockTargetPlugin {
        fn name(&self) -> &str { self.name }
        fn target_image(&self, _version: &str) -> String { format!("{}:latest", self.name) }
        fn pip_packages(&self) -> Vec<String> { vec![] }
        fn db_port(&self) -> u16 { 19530 }
        fn safety_nets(&self) -> Vec<SafetyNet> { vec![] }
        fn create_reviewer(&self) -> Option<Box<dyn crate::review::IndependentReviewer>> { None }
        fn derive_oracle_checks(&self, _contract: &StructuredContract) -> Vec<crate::agent::oracle::InvariantCheck> { vec![] }
        fn target_style(&self) -> crate::target::TargetStyle { crate::target::TargetStyle::Milvus }
        fn doc_citation_url(&self) -> String { String::new() }
        fn probe_template(&self) -> &dyn crate::agent::probe::ProbeTemplate { &self.probe }
    }

    fn make_test_orchestrator<'a>(
        llm_client: &'a DeepSeekClient,
        plugin: &'a dyn TargetPlugin,
        contract: StructuredContract,
        batch_defects_summary: String,
    ) -> FAOrchestrator<'a> {
        FAOrchestrator {
            llm_client,
            plugin,
            contract_content: String::new(),
            contract,
            db_image: String::new(),
            pip_packages: vec![],
            db_port: 19530,
            sidecars: vec![],
            db_env: vec![],
            db_command: vec![],
            max_turns: 10,
            multi_defect: false,
            custom_system_prompt: None,
            custom_initial_message: None,
            batch_defects_summary,
            skip_safety_nets: false,
            harness: RefCell::new(Harness::new()),
            processed_defect_keys: RefCell::new(HashSet::new()),
            early_mre_unlock: RefCell::new(false),
            baseline_records: RefCell::new(Vec::new()),
            baseline_output: None,
            conversation_log: RefCell::new(Vec::new()),
            diagnostic_dir: None,
        }
    }

    fn empty_contract() -> StructuredContract {
        StructuredContract {
            api_endpoint: "search".to_string(),
            doc_url: String::new(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        }
    }

    fn ensure_llm_client() -> DeepSeekClient {
        let api_key = std::env::var("DEEPSEEK_API_KEY").unwrap_or_else(|_| "test-key".to_string());
        // SAFETY: test-only code, single-threaded test environment
        unsafe { std::env::set_var("DEEPSEEK_API_KEY", &api_key); }
        DeepSeekClient::new("https://api.example.com".to_string(), "test-model".to_string(), None).unwrap()
    }

    #[test]
    fn test_build_behavioral_section_empty() {
        let llm = ensure_llm_client();
        let plugin = MockTargetPlugin { name: "milvus", probe: MockProbeTemplate };
        let orch = make_test_orchestrator(&llm, &plugin, empty_contract(), String::new());
        let section = orch.build_behavioral_section();
        assert!(section.is_empty(), "Empty behavioral_contracts should produce empty section");
    }

    #[test]
    fn test_build_behavioral_section_with_contracts() {
        let llm = ensure_llm_client();
        let plugin = MockTargetPlugin { name: "milvus", probe: MockProbeTemplate };
        let mut contract = empty_contract();
        contract.behavioral_contracts = vec![
            crate::contract::schema::BehavioralContract {
                name: "count_after_insert".to_string(),
                category: crate::contract::schema::BehaviorCategory::StateConsistency,
                endpoints: vec!["/v2/vectordb/entities/insert".to_string()],
                precondition_script: String::new(),
                verification_script: "r = api('POST', '/v2/vectordb/collections/describe', ...)".to_string(),
                expected_outcome: "rowCount matches inserted count".to_string(),
                supersedes: None,
                mutation_rules: vec![],
            },
        ];
        let orch = make_test_orchestrator(&llm, &plugin, contract, String::new());
        let section = orch.build_behavioral_section();
        assert!(!section.is_empty());
        assert!(section.contains("BEHAVIORAL CONTRACTS"));
        assert!(section.contains("count_after_insert"));
        assert!(section.contains("StateConsistency"));
        assert!(section.contains("TESTVDB_DB_URL"));
    }

    #[test]
    fn test_build_defect_context_milvus() {
        let llm = ensure_llm_client();
        let plugin = MockTargetPlugin { name: "milvus", probe: MockProbeTemplate };
        let orch = make_test_orchestrator(&llm, &plugin, empty_contract(), String::new());
        let ctx = orch.build_defect_context();
        assert!(ctx.contains("DATABASE-SPECIFIC WEAKNESS MAP"));
        assert!(ctx.contains("MILVUS"));
    }

    #[test]
    fn test_build_defect_context_with_batch_summary() {
        let llm = ensure_llm_client();
        let plugin = MockTargetPlugin { name: "milvus", probe: MockProbeTemplate };
        let batch_summary = "limit=-1: accepted (expected rejection)\noffset=0: accepted (expected rejection)".to_string();
        let orch = make_test_orchestrator(&llm, &plugin, empty_contract(), batch_summary);
        let ctx = orch.build_defect_context();
        assert!(ctx.contains("EXPAND FROM THEM"));
        assert!(ctx.contains("limit=-1"));
        assert!(ctx.contains("SEMANTIC EQUIVALENTS"));
        assert!(ctx.contains("CROSS-ENDPOINT CONSISTENCY"));
        assert!(ctx.contains("DIFFERENTIAL TEST EXAMPLE"));
        assert!(ctx.contains("CROSS_ENDPOINT_INCONSISTENCY"));
    }

    #[test]
    fn test_build_defect_context_unknown_target() {
        let llm = ensure_llm_client();
        struct UnknownPlugin { probe: MockProbeTemplate }
        impl TargetPlugin for UnknownPlugin {
            fn name(&self) -> &str { "unknown_db" }
            fn target_image(&self, _v: &str) -> String { "unknown:latest".to_string() }
            fn pip_packages(&self) -> Vec<String> { vec![] }
            fn db_port(&self) -> u16 { 5432 }
            fn safety_nets(&self) -> Vec<SafetyNet> { vec![] }
            fn create_reviewer(&self) -> Option<Box<dyn crate::review::IndependentReviewer>> { None }
            fn derive_oracle_checks(&self, _c: &StructuredContract) -> Vec<crate::agent::oracle::InvariantCheck> { vec![] }
            fn target_style(&self) -> crate::target::TargetStyle { crate::target::TargetStyle::Milvus }
            fn doc_citation_url(&self) -> String { String::new() }
            fn probe_template(&self) -> &dyn crate::agent::probe::ProbeTemplate { &self.probe }
        }
        let plugin = UnknownPlugin { probe: MockProbeTemplate };
        let orch = make_test_orchestrator(&llm, &plugin, empty_contract(), String::new());
        let ctx = orch.build_defect_context();
        assert!(ctx.contains("DATABASE-SPECIFIC WEAKNESS MAP"));
        assert!(!ctx.contains("MILVUS"));
        assert!(!ctx.contains("QDRANT"));
    }

    #[test]
    fn test_boundary_reject_short_script_empty_summary() {
        let code = "r = api('POST', '/search', nprobe=0)\nprint(r)";
        assert!(should_reject_boundary_test(code, ""));
    }

    #[test]
    fn test_boundary_allow_seed_expansion() {
        let code = "r = api('POST', '/search', nprobe=0)\nprint(r)";
        let summary = "nprobe=0: accepted (expected rejection)";
        assert!(!should_reject_boundary_test(code, summary));
    }
}