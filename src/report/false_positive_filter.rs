use crate::agent::classifier::DefectType;
use crate::agent::orchestrator::CollectedDefect;
use crate::contract::analyzer::BatchDefect;
use crate::contract::schema::StructuredContract;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterReason {
    ApiParamMismatch {
        param: String,
        detail: String,
    },
    DuplicateDefect {
        original_endpoint: String,
        original_defect_type: String,
        original_trigger: String,
    },
    ByDesignPattern {
        pattern_name: String,
        detail: String,
    },
}

impl std::fmt::Display for FilterReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterReason::ApiParamMismatch { param, detail } => {
                write!(f, "API_PARAM_MISMATCH(param={}): {}", param, detail)
            }
            FilterReason::DuplicateDefect {
                original_endpoint,
                original_defect_type,
                original_trigger,
            } => {
                write!(
                    f,
                    "DUPLICATE(endpoint={}, type={}, trigger={})",
                    original_endpoint, original_defect_type, original_trigger
                )
            }
            FilterReason::ByDesignPattern {
                pattern_name,
                detail,
            } => {
                write!(f, "BY_DESIGN({}): {}", pattern_name, detail)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterResult {
    pub passed: bool,
    pub reason: Option<FilterReason>,
    pub confidence: f32,
    pub endpoint: String,
    pub defect_type: String,
    pub trigger_pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSummary {
    pub total_input: usize,
    pub passed: usize,
    pub rejected: usize,
    pub results: Vec<FilterResult>,
}

struct ByDesignPattern {
    name: &'static str,
    targets: &'static [&'static str],
    script_contains: &'static [&'static str],
    defect_type_matches: &'static [DefectType],
    detail: &'static str,
}

impl ByDesignPattern {
    fn matches(&self, target: &str, script: &str, defect_type: &Option<DefectType>) -> bool {
        if !self.targets.is_empty() && !self.targets.contains(&target) {
            return false;
        }

        let script_matches = if self.script_contains.is_empty() {
            true
        } else {
            self.script_contains.iter().all(|s| script.contains(s))
        };

        let type_matches = if self.defect_type_matches.is_empty() {
            true
        } else {
            defect_type
                .as_ref()
                .map(|dt| self.defect_type_matches.contains(dt))
                .unwrap_or(false)
        };

        script_matches && type_matches
    }
}

static BY_DESIGN_PATTERNS: &[ByDesignPattern] = &[
    ByDesignPattern {
        name: "qdrant_shard_number_negative",
        targets: &["qdrant"],
        script_contains: &["shard_number", "-1"],
        defect_type_matches: &[DefectType::IllegalSuccess, DefectType::ParamIgnored],
        detail: "Qdrant silently normalizes negative shard_number to 1 (known behavior)",
    },
    ByDesignPattern {
        name: "qdrant_replication_factor_negative",
        targets: &["qdrant"],
        script_contains: &["replication_factor", "-1"],
        defect_type_matches: &[DefectType::IllegalSuccess, DefectType::ParamIgnored],
        detail: "Qdrant silently normalizes negative replication_factor (known behavior)",
    },
    ByDesignPattern {
        name: "qdrant_hnsw_ef_zero",
        targets: &["qdrant"],
        script_contains: &["hnsw_ef", "0"],
        defect_type_matches: &[DefectType::IllegalSuccess, DefectType::ParamIgnored],
        detail: "Qdrant hnsw_ef=0 is accepted but ignored (known behavior, issue #9039)",
    },
    ByDesignPattern {
        name: "vectors_size_zero",
        targets: &[],
        script_contains: &["vectors", "size", "0"],
        defect_type_matches: &[DefectType::IllegalSuccess],
        detail: "vectors.size=0 may be accepted by design (allows zero-size vector configs)",
    },
    ByDesignPattern {
        name: "optimizers_config_edge",
        targets: &["qdrant"],
        script_contains: &["optimizers_config", "indexing_threshold"],
        defect_type_matches: &[DefectType::IllegalSuccess, DefectType::ParamIgnored],
        detail: "Qdrant silently normalizes edge-case optimizers_config values",
    },
    ByDesignPattern {
        name: "pgvector_boundary_sql",
        targets: &["pgvector"],
        script_contains: &["limit", "0"],
        defect_type_matches: &[DefectType::IllegalSuccess],
        detail: "PostgreSQL accepts limit=0 as valid SQL (returns empty result, not an error)",
    },
    ByDesignPattern {
        name: "weaviate_dim_mismatch_500",
        targets: &["weaviate"],
        script_contains: &["dim"],
        defect_type_matches: &[DefectType::IllegalSuccess],
        detail: "Weaviate silently normalizes dimension mismatch to 500 (known behavior)",
    },
    ByDesignPattern {
        name: "milvus_limit_negative",
        targets: &["milvus"],
        script_contains: &["limit", "-1"],
        defect_type_matches: &[DefectType::IllegalSuccess],
        detail: "Milvus may accept negative limit with documented behavior",
    },
];

pub fn extract_endpoint_from_script(script: &str) -> String {
    let s = script.to_lowercase();
    if s.contains("/entities/search") || s.contains("/points/search") || s.contains("graphql") {
        "search".to_string()
    } else if s.contains("/entities/insert") || (s.contains("/points") && s.contains("upsert")) || (s.contains("/collections/") && s.contains("/points") && !s.contains("/search") && !s.contains("/scroll") && !s.contains("/recommend") && !s.contains("/count") && !s.contains("/delete")) {
        "insert".to_string()
    } else if s.contains("/entities/delete") || (s.contains("/points") && s.contains("delete")) {
        "delete".to_string()
    } else if s.contains("/collections/create") || (s.contains("/collections") && s.contains("put") && !s.contains("/points")) {
        "create".to_string()
    } else if s.contains("/collections/delete") {
        "delete_collection".to_string()
    } else if s.contains("/collections/describe") || s.contains("/collections/get") {
        "describe".to_string()
    } else if s.contains("/entities/query") || s.contains("/points/scroll") {
        "query".to_string()
    } else if s.contains("/collections") {
        "collection".to_string()
    } else if s.contains("/indexes") {
        "index".to_string()
    } else {
        "unknown".to_string()
    }
}

pub fn extract_trigger_pattern(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}\n{}", stdout.to_lowercase(), stderr.to_lowercase());

    let known_signals = [
        "illegal_success", "param_ignored", "poor_diagnostics", "data_corruption",
        "state_violation", "search_correctness", "cross_endpoint_inconsistency",
        "concurrent_race", "sequence_violation", "differential_mismatch",
        "metamorphic_violation", "state_logic_violation",
    ];

    for signal in &known_signals {
        let tag = format!("[defect: {signal}]");
        if combined.contains(&tag) {
            return format!("[defect:{}]", signal);
        }
    }

    if combined.contains("assertionerror") {
        return "assertion_error".to_string();
    }
    if combined.contains("traceback") {
        return "runtime_error".to_string();
    }

    "unknown".to_string()
}

fn check_api_params(
    script: &str,
    contract: &StructuredContract,
) -> Option<FilterReason> {
    let contract_params: HashSet<&str> = contract
        .type_constraints
        .iter()
        .map(|tc| tc.param_name.as_str())
        .chain(contract.range_constraints.iter().map(|rc| rc.param_name.as_str()))
        .collect();

    if contract_params.is_empty() {
        return None;
    }

    let script_params = [
        "limit", "offset", "nprobe", "ef", "hnsw_ef", "shard_number",
        "shardsNum", "replication_factor", "score_threshold", "exact",
        "vectors.size", "size", "dim", "distance", "on_disk",
        "indexing_threshold", "oversampling", "quantization",
    ];

    let mut found_params: Vec<&str> = Vec::new();
    let mut missing_params: Vec<&str> = Vec::new();

    for param in &script_params {
        if script.contains(param) {
            found_params.push(param);
            if !contract_params.contains(param)
                && !contract_params.iter().any(|cp| cp.contains(param) || param.contains(cp))
            {
                missing_params.push(param);
            }
        }
    }

    if found_params.is_empty() {
        return None;
    }

    if missing_params.len() as f32 / found_params.len() as f32 > 0.5 {
        Some(FilterReason::ApiParamMismatch {
            param: missing_params.join(","),
            detail: format!(
                "{} of {} script params not in contract: {}",
                missing_params.len(),
                found_params.len(),
                missing_params.join(", ")
            ),
        })
    } else {
        None
    }
}

fn check_by_design(
    script: &str,
    defect_type: &Option<DefectType>,
    target: &str,
) -> Option<FilterReason> {
    for pattern in BY_DESIGN_PATTERNS {
        if pattern.matches(target, script, defect_type) {
            return Some(FilterReason::ByDesignPattern {
                pattern_name: pattern.name.to_string(),
                detail: pattern.detail.to_string(),
            });
        }
    }
    None
}

fn check_ignore_policy(
    script: &str,
    contract: &StructuredContract,
) -> Option<FilterReason> {
    for (param, policy) in &contract.rejection_policies {
        if *policy == crate::contract::schema::RejectionPolicy::Ignore
            && script.to_lowercase().contains(&param.to_lowercase())
        {
            return Some(FilterReason::ByDesignPattern {
                pattern_name: "contract_ignore_policy".to_string(),
                detail: format!(
                    "Contract marks param '{}' with Ignore rejection policy",
                    param
                ),
            });
        }
    }
    None
}

fn build_defect_key(endpoint: &str, defect_type: &Option<DefectType>, trigger: &str) -> String {
    let dt = defect_type
        .as_ref()
        .map(|d| format!("{:?}", d))
        .unwrap_or_else(|| "unknown".to_string());
    format!("{}|{}|{}", endpoint, dt, trigger)
}

pub struct FalsePositiveFilter;

impl FalsePositiveFilter {
    pub fn filter_collected_defects(
        defects: &[CollectedDefect],
        contract: &StructuredContract,
        target: &str,
    ) -> FilterSummary {
        let mut results: Vec<FilterResult> = Vec::new();
        let mut seen_keys: HashSet<String> = HashSet::new();
        let mut passed_defects: Vec<usize> = Vec::new();
        let mut rejected_defects: Vec<usize> = Vec::new();

        for (i, defect) in defects.iter().enumerate() {
            let script = &defect.script;
            let classification = &defect.classification;
            let defect_type = classification.defect_type.clone();
            let endpoint = extract_endpoint_from_script(script);
            let trigger = extract_trigger_pattern(
                &defect.evidence.stdout,
                &defect.evidence.stderr,
            );

            let defect_key = build_defect_key(&endpoint, &defect_type, &trigger);

            let mut filter_result = FilterResult {
                passed: true,
                reason: None,
                confidence: 0.9,
                endpoint: endpoint.clone(),
                defect_type: defect_type
                    .as_ref()
                    .map(|d| format!("{:?}", d))
                    .unwrap_or_else(|| "unknown".to_string()),
                trigger_pattern: trigger.clone(),
            };

            if let Some(reason) = check_ignore_policy(script, contract) {
                filter_result.passed = false;
                filter_result.reason = Some(reason);
                filter_result.confidence = 0.95;
                results.push(filter_result);
                rejected_defects.push(i);
                continue;
            }

            if let Some(reason) = check_by_design(script, &defect_type, target) {
                filter_result.passed = false;
                filter_result.reason = Some(reason);
                filter_result.confidence = 0.9;
                results.push(filter_result);
                rejected_defects.push(i);
                continue;
            }

            if !seen_keys.insert(defect_key) {
                let original = results.iter().find(|r| {
                    r.endpoint == endpoint
                        && r.defect_type
                            == defect_type
                                .as_ref()
                                .map(|d| format!("{:?}", d))
                                .unwrap_or_else(|| "unknown".to_string())
                        && r.trigger_pattern == trigger
                });

                let (orig_ep, orig_dt, orig_tr) = original.map_or(
                    (endpoint.clone(), "unknown".to_string(), trigger.clone()),
                    |r| {
                        (
                            r.endpoint.clone(),
                            r.defect_type.clone(),
                            r.trigger_pattern.clone(),
                        )
                    },
                );

                filter_result.passed = false;
                filter_result.reason = Some(FilterReason::DuplicateDefect {
                    original_endpoint: orig_ep,
                    original_defect_type: orig_dt,
                    original_trigger: orig_tr,
                });
                filter_result.confidence = 0.99;
                results.push(filter_result);
                rejected_defects.push(i);
                continue;
            }

            if let Some(reason) = check_api_params(script, contract) {
                filter_result.passed = false;
                filter_result.reason = Some(reason);
                filter_result.confidence = 0.7;
                results.push(filter_result);
                rejected_defects.push(i);
            } else {
                filter_result.confidence = 0.85;
                results.push(filter_result);
                passed_defects.push(i);
            }
        }

        let summary = FilterSummary {
            total_input: defects.len(),
            passed: passed_defects.len(),
            rejected: rejected_defects.len(),
            results,
        };

        Self::log_summary(&summary, target);

        summary
    }

    pub fn filter_batch_defects(
        defects: &[BatchDefect],
        contract: &StructuredContract,
        target: &str,
    ) -> FilterSummary {
        let mut results: Vec<FilterResult> = Vec::new();
        let mut seen_keys: HashSet<String> = HashSet::new();
        let mut passed_count = 0usize;
        let mut rejected_count = 0usize;

        for defect in defects {
            let script = &defect.script;
            let endpoint = defect
                .endpoint
                .clone()
                .unwrap_or_else(|| extract_endpoint_from_script(script));
            let trigger = extract_trigger_pattern(&defect.stdout, &defect.stderr);
            let defect_type = if defect.defect_line.contains("ILLEGAL_SUCCESS") {
                Some(DefectType::IllegalSuccess)
            } else if defect.defect_line.contains("SEQUENCE_VIOLATION") {
                Some(DefectType::SequenceViolation)
            } else if defect.defect_line.contains("DIFFERENTIAL_MISMATCH") {
                Some(DefectType::DifferentialMismatch)
            } else if defect.defect_line.contains("METAMORPHIC_VIOLATION") {
                Some(DefectType::MetamorphicViolation)
            } else if defect.defect_line.contains("STATE_LOGIC_VIOLATION") {
                Some(DefectType::StateLogicViolation)
            } else {
                None
            };

            let defect_key = build_defect_key(&endpoint, &defect_type, &trigger);

            let mut filter_result = FilterResult {
                passed: true,
                reason: None,
                confidence: 0.9,
                endpoint: endpoint.clone(),
                defect_type: defect_type
                    .as_ref()
                    .map(|d| format!("{:?}", d))
                    .unwrap_or_else(|| "unknown".to_string()),
                trigger_pattern: trigger.clone(),
            };

            if let Some(reason) = check_ignore_policy(script, contract) {
                filter_result.passed = false;
                filter_result.reason = Some(reason);
                filter_result.confidence = 0.95;
                results.push(filter_result);
                rejected_count += 1;
                continue;
            }

            if let Some(reason) = check_by_design(script, &defect_type, target) {
                filter_result.passed = false;
                filter_result.reason = Some(reason);
                filter_result.confidence = 0.9;
                results.push(filter_result);
                rejected_count += 1;
                continue;
            }

            if !seen_keys.insert(defect_key) {
                filter_result.passed = false;
                filter_result.reason = Some(FilterReason::DuplicateDefect {
                    original_endpoint: endpoint.clone(),
                    original_defect_type: defect_type
                        .as_ref()
                        .map(|d| format!("{:?}", d))
                        .unwrap_or_else(|| "unknown".to_string()),
                    original_trigger: trigger.clone(),
                });
                filter_result.confidence = 0.99;
                results.push(filter_result);
                rejected_count += 1;
                continue;
            }

            if let Some(reason) = check_api_params(script, contract) {
                filter_result.passed = false;
                filter_result.reason = Some(reason);
                filter_result.confidence = 0.7;
                results.push(filter_result);
                rejected_count += 1;
            } else {
                filter_result.confidence = 0.85;
                results.push(filter_result);
                passed_count += 1;
            }
        }

        let summary = FilterSummary {
            total_input: defects.len(),
            passed: passed_count,
            rejected: rejected_count,
            results,
        };

        Self::log_summary(&summary, target);

        summary
    }

    fn log_summary(summary: &FilterSummary, target: &str) {
        let log_path = "false_positive_filter.log";
        let mut log_content = format!(
            "=== FalsePositiveFilter Summary for target={} ===\n",
            target
        );
        log_content.push_str(&format!(
            "Total: {} | Passed: {} | Rejected: {}\n",
            summary.total_input, summary.passed, summary.rejected
        ));
        log_content.push_str("---\n");

        for result in &summary.results {
            let status = if result.passed { "PASS" } else { "REJECT" };
            let reason_str = result
                .reason
                .as_ref()
                .map(|r| r.to_string())
                .unwrap_or_else(|| "none".to_string());
            log_content.push_str(&format!(
                "[{}] endpoint={} type={} trigger={} confidence={:.2} reason={}\n",
                status,
                result.endpoint,
                result.defect_type,
                result.trigger_pattern,
                result.confidence,
                reason_str
            ));
        }

        if let Err(e) = std::fs::write(log_path, &log_content) {
            warn!("Failed to write false positive filter log: {}", e);
        } else {
            info!(
                "FalsePositiveFilter log written to {} ({} passed, {} rejected)",
                log_path, summary.passed, summary.rejected
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::classifier::{ClassificationDisposition, ClassificationResult};
    use crate::report::generator::RunEvidence;

    fn make_test_collected_defect(
        script: &str,
        stdout: &str,
        stderr: &str,
        defect_type: DefectType,
    ) -> CollectedDefect {
        CollectedDefect {
            script: script.to_string(),
            evidence: RunEvidence {
                phase: "test".to_string(),
                db_url: "http://localhost".to_string(),
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
                classifier_reason: "test".to_string(),
                classifier_evidence_excerpt: "test".to_string(),
                exit_success: false,
            },
            classification: ClassificationResult {
                disposition: ClassificationDisposition::CandidateDefect,
                defect_type: Some(defect_type),
                reason: "test".to_string(),
                evidence_excerpt: "test".to_string(),
                sub_type: None,
                llm_review: None,
            },
        }
    }

    fn make_test_contract() -> StructuredContract {
        StructuredContract {
            api_endpoint: "/collections/{collection_name}/points/search".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![
                crate::contract::schema::TypeConstraint {
                    param_name: "limit".to_string(),
                    expected_type: "integer".to_string(),
                    violation_examples: vec![],
                },
                crate::contract::schema::TypeConstraint {
                    param_name: "offset".to_string(),
                    expected_type: "integer".to_string(),
                    violation_examples: vec![],
                },
            ],
            range_constraints: vec![
                crate::contract::schema::RangeConstraint {
                    param_name: "limit".to_string(),
                    description: "limit must be >= 1".to_string(),
                    min: Some(1.0),
                    max: None,
                    violation_examples: vec![],
                },
            ],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_extract_endpoint_search() {
        let script = "r = requests.post(f'{BASE}/collections/c/points/search', json=body)";
        assert_eq!(extract_endpoint_from_script(script), "search");
    }

    #[test]
    fn test_extract_endpoint_create() {
        let script = "r = requests.put(f'{BASE}/collections/c', json=body)";
        assert_eq!(extract_endpoint_from_script(script), "create");
    }

    #[test]
    fn test_extract_endpoint_insert() {
        let script = "r = requests.post(f'{BASE}/collections/c/points', json={\"points\": [...]})";
        assert_eq!(extract_endpoint_from_script(script), "insert");
    }

    #[test]
    fn test_extract_trigger_illegal_success() {
        let stdout = "[DEFECT: ILLEGAL_SUCCESS] limit=-1 accepted";
        assert_eq!(
            extract_trigger_pattern(stdout, ""),
            "[defect:illegal_success]"
        );
    }

    #[test]
    fn test_extract_trigger_state_violation() {
        let stdout = "[DEFECT: STATE_VIOLATION] rowCount mismatch";
        assert_eq!(
            extract_trigger_pattern(stdout, ""),
            "[defect:state_violation]"
        );
    }

    #[test]
    fn test_extract_trigger_unknown() {
        assert_eq!(extract_trigger_pattern("all good", ""), "unknown");
    }

    #[test]
    fn test_check_by_design_qdrant_shard_number() {
        let script = "shard_number=-1";
        let reason = check_by_design(script, &Some(DefectType::IllegalSuccess), "qdrant");
        assert!(reason.is_some());
        match reason.unwrap() {
            FilterReason::ByDesignPattern { pattern_name, .. } => {
                assert_eq!(pattern_name, "qdrant_shard_number_negative");
            }
            _ => panic!("expected ByDesignPattern"),
        }
    }

    #[test]
    fn test_check_by_design_not_qdrant_for_milvus() {
        let script = "shard_number=-1";
        let reason = check_by_design(script, &Some(DefectType::IllegalSuccess), "milvus");
        assert!(reason.is_none());
    }

    #[test]
    fn test_check_api_params_all_match() {
        let contract = make_test_contract();
        let script = "body['limit'] = -1";
        let reason = check_api_params(script, &contract);
        assert!(reason.is_none());
    }

    #[test]
    fn test_check_api_params_mismatch() {
        let contract = make_test_contract();
        let script = "body['hnsw_ef'] = -1; body['shard_number'] = -1";
        let reason = check_api_params(script, &contract);
        assert!(reason.is_some());
    }

    #[test]
    fn test_check_ignore_policy() {
        let mut contract = make_test_contract();
        contract.rejection_policies.insert(
            "shard_number".to_string(),
            crate::contract::schema::RejectionPolicy::Ignore,
        );
        let script = "body['shard_number'] = -1";
        let reason = check_ignore_policy(script, &contract);
        assert!(reason.is_some());
    }

    #[test]
    fn test_dedup_collected_defects() {
        let contract = make_test_contract();
        let defects = vec![
            make_test_collected_defect(
                "body['limit'] = -1",
                "[DEFECT: ILLEGAL_SUCCESS] limit=-1 accepted",
                "",
                DefectType::IllegalSuccess,
            ),
            make_test_collected_defect(
                "body['limit'] = -1; body['offset'] = -1",
                "[DEFECT: ILLEGAL_SUCCESS] limit=-1 and offset=-1 accepted",
                "",
                DefectType::IllegalSuccess,
            ),
        ];
        let summary = FalsePositiveFilter::filter_collected_defects(&defects, &contract, "test");
        assert_eq!(summary.total_input, 2);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.rejected, 1);
    }

    #[test]
    fn test_filter_batch_defects_empty() {
        let contract = make_test_contract();
        let summary = FalsePositiveFilter::filter_batch_defects(&[], &contract, "test");
        assert_eq!(summary.total_input, 0);
        assert_eq!(summary.passed, 0);
    }

    #[test]
    fn test_filter_batch_defects_by_design() {
        let contract = make_test_contract();
        let defects = vec![BatchDefect {
            test_name: "test".to_string(),
            test_prefix: "boundary".to_string(),
            defect_line: "[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted".to_string(),
            script: "hnsw_ef=0; shard_number=-1".to_string(),
            stdout: "[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted".to_string(),
            stderr: String::new(),
            endpoint: Some("search".to_string()),
            param_name: Some("hnsw_ef".to_string()),
            exit_success: false,
        }];
        let summary = FalsePositiveFilter::filter_batch_defects(&defects, &contract, "qdrant");
        assert_eq!(summary.total_input, 1);
        assert_eq!(summary.rejected, 1);
    }

    #[test]
    fn test_filter_batch_defects_pass() {
        let contract = make_test_contract();
        let defects = vec![BatchDefect {
            test_name: "test".to_string(),
            test_prefix: "boundary".to_string(),
            defect_line: "[DEFECT: ILLEGAL_SUCCESS] limit=-1 accepted".to_string(),
            script: "body['limit'] = -1".to_string(),
            stdout: "[DEFECT: ILLEGAL_SUCCESS] limit=-1 accepted".to_string(),
            stderr: String::new(),
            endpoint: Some("search".to_string()),
            param_name: Some("limit".to_string()),
            exit_success: false,
        }];
        let summary = FalsePositiveFilter::filter_batch_defects(&defects, &contract, "test");
        assert_eq!(summary.total_input, 1);
        assert_eq!(summary.passed, 1);
    }
}