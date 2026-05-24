use crate::contract::store::{AnnotatedRangeConstraint, AnnotatedTypeConstraint, ContractStore, ObservedBehavior};
use crate::contract::schema::{RangeConstraint, TypeConstraint};
use crate::contract::store::{Confidence, ConstraintSource};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDefect {
    pub test_name: String,
    pub test_prefix: String,
    pub defect_line: String,
    pub script: String,
    pub stdout: String,
    pub stderr: String,
    pub endpoint: Option<String>,
    pub param_name: Option<String>,
    #[serde(default)]
    pub exit_success: bool,
}

/// A clustered root cause: multiple defect instances collapsed into one actionable finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefectCluster {
    pub root_cause: String,
    pub defect_kind: DefectKind,
    pub count: usize,
    /// The strongest example from this cluster (clearest reproduction).
    pub exemplar: BatchDefect,
    /// Whether this cluster is likely a known benign pattern.
    pub likely_benign: bool,
    pub benign_rationale: String,
}

/// Known benign patterns: defect signals that are expected/design behavior, not real bugs.
/// (endpoint_substring, param_substring_or_empty, rationale)
const BENIGN_PATTERNS: &[(&str, &str, &str)] = &[
    // Qdrant/Milvus search silently drops unknown JSON keys — BUT only for non-search-params.
    // Real defects on search endpoint (hnsw_ef, score_threshold, limit, offset) must NOT be flagged.
    ("search", "", "Search endpoint silently ignores unrecognized JSON keys — expected design. HOWEVER, hnsw_ef=0, score_threshold out-of-range, and similar param violations on recognized keys are REAL defects."),
    ("list", "", "list/describe endpoints returning success for nonexistent resources is standard REST idempotency."),
];

/// Parameters that, when violated on the search endpoint, indicate a REAL defect (not benign param injection).
const SEARCH_REAL_DEFECT_PARAMS: &[&str] = &[
    "hnsw_ef", "score_threshold", "limit", "offset", "exact",
    "size", "vectors.size", "distance", "shard_number", "replication_factor",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectKind {
    IllegalSuccess,
    SequenceViolation,
    DifferentialMismatch,
    MetamorphicViolation,
    StateLogicViolation,
    Unknown,
}

impl DefectKind {
    pub fn from_defect_line(line: &str) -> Self {
        if line.contains("ILLEGAL_SUCCESS") {
            DefectKind::IllegalSuccess
        } else if line.contains("METAMORPHIC_VIOLATION") {
            DefectKind::MetamorphicViolation
        } else if line.contains("STATE_LOGIC_VIOLATION") {
            DefectKind::StateLogicViolation
        } else if line.contains("SEQUENCE_VIOLATION") {
            DefectKind::SequenceViolation
        } else if line.contains("DIFFERENTIAL_MISMATCH") {
            DefectKind::DifferentialMismatch
        } else {
            DefectKind::Unknown
        }
    }
}

pub struct ResultAnalyzer;

impl ResultAnalyzer {
    pub fn analyze(defects: &[BatchDefect]) -> Vec<ObservedBehavior> {
        let mut observations = Vec::new();
        for defect in defects {
            let kind = DefectKind::from_defect_line(&defect.defect_line);
            let (endpoint, param_name) = if let (Some(ep), Some(pn)) = (&defect.endpoint, &defect.param_name) {
                (ep.clone(), pn.clone())
            } else {
                Self::extract_context(&defect.test_name, &defect.test_prefix)
            };

            let (expected, actual) = match kind {
                DefectKind::IllegalSuccess => {
                    ("should reject invalid input".to_string(), "accepted invalid input".to_string())
                }
                DefectKind::SequenceViolation => {
                    ("operation sequence should succeed".to_string(), "operation sequence failed".to_string())
                }
                DefectKind::DifferentialMismatch => {
                    ("REST and SDK should agree".to_string(), "REST and SDK disagree".to_string())
                }
                DefectKind::MetamorphicViolation => {
                    ("metamorphic relation should hold".to_string(), "metamorphic relation violated".to_string())
                }
                DefectKind::StateLogicViolation => {
                    ("state transition should be consistent".to_string(), "state transition inconsistent".to_string())
                }
                DefectKind::Unknown => {
                    ("expected correct behavior".to_string(), "unexpected behavior observed".to_string())
                }
            };

            observations.push(ObservedBehavior {
                endpoint,
                param_name,
                description: defect.defect_line.clone(),
                observed_value: Self::extract_observed_value(&defect.defect_line),
                expected_behavior: expected,
                actual_behavior: actual,
                is_violation: true,
            });
        }
        observations
    }

    /// Collapse raw defects into root-cause clusters, marking known benign patterns.
    pub fn cluster_defects(defects: &[BatchDefect]) -> Vec<DefectCluster> {
        // Group by (kind, endpoint, param_name) — same endpoint + same param = same root cause.
        let mut groups: HashMap<(DefectKind, String, String), Vec<&BatchDefect>> = HashMap::new();
        for d in defects {
            let kind = DefectKind::from_defect_line(&d.defect_line);
            let (endpoint, param_name) = if let (Some(ep), Some(pn)) = (&d.endpoint, &d.param_name) {
                (ep.clone(), pn.clone())
            } else if d.test_prefix == "boundary" && !d.script.is_empty() {
                Self::parse_from_script(&d.script)
            } else {
                Self::extract_context(&d.test_name, &d.test_prefix)
            };
            // Normalize endpoint: strip host/port, keep path
            let short_ep = endpoint
                .split('/')
                .filter(|s| !s.is_empty())
                .last()
                .unwrap_or(&endpoint)
                .to_string();
            groups.entry((kind, short_ep, param_name)).or_default().push(d);
        }

        let mut clusters: Vec<DefectCluster> = Vec::new();
        for ((kind, ep, param), instances) in &groups {
            let count = instances.len();
            let exemplar = (*instances.first().unwrap()).clone();

            // Check if this is a known benign pattern — but override for recognized real-defect params.
            let benign_match = BENIGN_PATTERNS.iter().find(|(p, _, _)| ep.contains(p));
            let likely_benign = if let Some((_, _, _)) = benign_match {
                // If param is a known REAL defect param on search endpoint, override benign.
                !SEARCH_REAL_DEFECT_PARAMS.iter().any(|rp| param.contains(rp))
            } else {
                false
            };
            let benign_rationale = if likely_benign {
                benign_match.map(|(_, _, r)| r.to_string()).unwrap_or_default()
            } else {
                String::new()
            };

            let root_cause = if count == 1 {
                format!("[{:?}] {} — single instance", kind, exemplar.defect_line)
            } else {
                format!(
                    "[{:?}] {} distinct test names on endpoint '{}' param '{}' share same root cause",
                    kind, count, ep, param
                )
            };

            clusters.push(DefectCluster {
                root_cause,
                defect_kind: *kind,
                count,
                exemplar,
                likely_benign,
                benign_rationale,
            });
        }

        // Sort: non-benign first, then by count descending
        clusters.sort_by(|a, b| {
            b.likely_benign
                .cmp(&a.likely_benign)
                .then_with(|| b.count.cmp(&a.count))
        });

        clusters
    }

    /// Parse endpoint and param from the Python script content when structured fields are absent.
    fn parse_from_script(script: &str) -> (String, String) {
        let s = script.to_lowercase();
        let endpoint = if s.contains("/points/search") {
            "search"
        } else if s.contains("/points/scroll") {
            "scroll"
        } else if s.contains("/points/recommend") {
            "recommend"
        } else if s.contains("/points/count") {
            "count"
        } else if s.contains("/points") && (s.contains("upsert") || s.contains("put")) && !s.contains("/collections") {
            "upsert"
        } else if s.contains("/points") && s.contains("delete") {
            "delete"
        } else if s.contains("/collections") && (s.contains("put") || s.contains("create")) && !s.contains("/points") {
            "create_collection"
        } else if s.contains("/collections") && s.contains("delete") {
            "delete_collection"
        } else if s.contains("/collections") {
            "collections"
        } else {
            "unknown"
        };

        let param = if script.contains("hnsw_ef") {
            "hnsw_ef"
        } else if script.contains("score_threshold") {
            "score_threshold"
        } else if script.contains("\"limit\"") || script.contains("'limit'") {
            "limit"
        } else if script.contains("\"offset\"") || script.contains("'offset'") {
            "offset"
        } else if script.contains("vectors.size") || script.contains("\"size\""){
            "vectors.size"
        } else if script.contains("shard_number") {
            "shard_number"
        } else if script.contains("replication_factor") {
            "replication_factor"
        } else if script.contains("oversampling") {
            "oversampling"
        } else if script.contains("exact") {
            "exact"
        } else if script.contains("\"size") || script.contains("'size") {
            "size"
        } else if script.contains("distance") {
            "distance"
        } else if script.contains("dimension") || script.contains("\"dim\"") || script.contains("elementTypeParams") {
            "dim"
        } else if script.contains("count") {
            "count"
        } else if script.contains("payload") {
            "payload"
        } else if script.contains("size") {
            "size"
        } else if script.contains("collection_name") || script.contains("collectionName") {
            "collectionName"
        } else {
            "unknown"
        };
        (format!("/{}", endpoint), param.to_string())
    }

    fn extract_context(test_name: &str, prefix: &str) -> (String, String) {
        match prefix {
            "boundary" => {
                // Boundary tests don't populate endpoint/param_name; fall through to script parsing
                ("/unknown".to_string(), "unknown".to_string())
            }
            "mutation" => {
                let parts: Vec<&str> = test_name.split('_').collect();
                let param = parts.get(2).unwrap_or(&"unknown").to_string();
                let endpoint = parts.last().unwrap_or(&"unknown").to_string();
                (format!("/v2/vectordb/{}/{}", endpoint, "unknown"), param)
            }
            "state" => {
                (format!("/v2/vectordb/collections/{}", test_name), "state".to_string())
            }
            "meta" => {
                (format!("/v2/vectordb/entities/search"), "metamorphic".to_string())
            }
            "seq" => {
                (format!("/v2/vectordb/{}", test_name), "sequence".to_string())
            }
            "res" => {
                if test_name.contains("dimension") {
                    ("/v2/vectordb/collections/create".to_string(), "dim".to_string())
                } else if test_name.contains("collection_name") {
                    ("/v2/vectordb/collections/create".to_string(), "collectionName".to_string())
                } else {
                    ("/v2/vectordb/unknown".to_string(), "unknown".to_string())
                }
            }
            "combo" => {
                ("/v2/vectordb/collections/create".to_string(), "combo".to_string())
            }
            "diff" => {
                let op = test_name.strip_prefix("diff_").unwrap_or("unknown");
                (format!("/v2/vectordb/{}", op), op.to_string())
            }
            "conc" => {
                let op = test_name.strip_prefix("concurrent_").unwrap_or("unknown");
                (format!("/v2/vectordb/{}", op), "concurrent".to_string())
            }
            _ => ("/unknown".to_string(), "unknown".to_string()),
        }
    }

    fn extract_observed_value(defect_line: &str) -> String {
        if let Some(start) = defect_line.find("] ") {
            defect_line[start + 2..].to_string()
        } else {
            defect_line.to_string()
        }
    }

    pub fn assimilate_batch(store: &mut ContractStore, defects: &[BatchDefect]) -> usize {
        let observations = Self::analyze(defects);
        let mut new_count = 0usize;

        for obs in &observations {
            let already_known = store.observed_behaviors.iter().any(|existing| {
                existing.description == obs.description
            });
            if already_known {
                continue;
            }

            let kind = DefectKind::from_defect_line(&obs.description);

            match kind {
                DefectKind::IllegalSuccess => {
                    if obs.param_name == "dim" {
                        let dim_val = Self::extract_dim_from_description(&obs.description);
                        if let Some(val) = dim_val {
                            store.range_constraints.push(AnnotatedRangeConstraint {
                                constraint: RangeConstraint {
                                    param_name: "dim".to_string(),
                                    description: format!("dim must be < {} (observed: {}-dim accepted)", val, val),
                                    min: Some(1.0),
                                    max: Some((val - 1) as f64),
                                    violation_examples: vec![val.to_string()],
                                },
                                endpoint: obs.endpoint.clone(),
                                source: ConstraintSource::ObservedBehavior,
                                confidence: Confidence::High,
                            });
                        }
                    } else if obs.param_name == "collectionName" {
                        store.type_constraints.push(AnnotatedTypeConstraint {
                            constraint: TypeConstraint {
                                param_name: "collectionName".to_string(),
                                expected_type: "string_with_max_length".to_string(),
                                violation_examples: vec!["256_char_name".to_string()],
                            },
                            endpoint: obs.endpoint.clone(),
                            source: ConstraintSource::ObservedBehavior,
                            confidence: Confidence::High,
                        });
                    } else {
                        store.type_constraints.push(AnnotatedTypeConstraint {
                            constraint: TypeConstraint {
                                param_name: obs.param_name.clone(),
                                expected_type: format!(
                                    "observed: {} should {} but actually {}",
                                    obs.param_name, obs.expected_behavior, obs.actual_behavior
                                ),
                                violation_examples: vec![obs.observed_value.clone()],
                            },
                            endpoint: obs.endpoint.clone(),
                            source: ConstraintSource::ObservedBehavior,
                            confidence: Confidence::High,
                        });
                    }
                }
                DefectKind::SequenceViolation | DefectKind::DifferentialMismatch | DefectKind::MetamorphicViolation | DefectKind::StateLogicViolation => {
                    store.type_constraints.push(AnnotatedTypeConstraint {
                        constraint: TypeConstraint {
                            param_name: obs.param_name.clone(),
                            expected_type: format!(
                                "observed: {} should {} but actually {}",
                                obs.param_name, obs.expected_behavior, obs.actual_behavior
                            ),
                            violation_examples: vec![obs.observed_value.clone()],
                        },
                        endpoint: obs.endpoint.clone(),
                        source: ConstraintSource::ObservedBehavior,
                        confidence: Confidence::High,
                    });
                }
                DefectKind::Unknown => {}
            }

            store.observed_behaviors.push(obs.clone());
            new_count += 1;
        }

        new_count
    }

    fn extract_dim_from_description(desc: &str) -> Option<usize> {
        let re = regex::Regex::new(r"(\d+)-dim").ok()?;
        let caps = re.captures(desc)?;
        caps.get(1)?.as_str().parse::<usize>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_illegal_success() {
        let defects = vec![BatchDefect {
            test_name: "mutation_type_confusion_limit_search".to_string(),
            test_prefix: "mutation".to_string(),
            defect_line: "[DEFECT: ILLEGAL_SUCCESS] type confusion: limit='abc' accepted".to_string(),
            script: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            endpoint: None,
            param_name: None,
            exit_success: false,
        }];
        let obs = ResultAnalyzer::analyze(&defects);
        assert_eq!(obs.len(), 1);
        assert!(obs[0].is_violation);
        assert_eq!(obs[0].param_name, "confusion");
        assert!(obs[0].expected_behavior.contains("reject"));
    }

    #[test]
    fn test_analyze_with_structured_context() {
        let defects = vec![BatchDefect {
            test_name: "mutation_type_confusion_limit_search".to_string(),
            test_prefix: "mutation".to_string(),
            defect_line: "[DEFECT: ILLEGAL_SUCCESS] type confusion: limit='abc' accepted".to_string(),
            script: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            endpoint: Some("/collections/test/points/search".to_string()),
            param_name: Some("limit".to_string()),
            exit_success: false,
        }];
        let obs = ResultAnalyzer::analyze(&defects);
        assert_eq!(obs[0].endpoint, "/collections/test/points/search");
        assert_eq!(obs[0].param_name, "limit");
    }

    #[test]
    fn test_analyze_sequence_violation() {
        let defects = vec![BatchDefect {
            test_name: "concurrent_insert_search".to_string(),
            test_prefix: "conc".to_string(),
            defect_line: "[DEFECT: SEQUENCE_VIOLATION] concurrent insert+search crashed system".to_string(),
            script: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            endpoint: None,
            param_name: None,
            exit_success: false,
        }];
        let obs = ResultAnalyzer::analyze(&defects);
        assert_eq!(obs.len(), 1);
        assert!(obs[0].is_violation);
        assert!(obs[0].expected_behavior.contains("succeed"));
    }

    #[test]
    fn test_analyze_differential_mismatch() {
        let defects = vec![BatchDefect {
            test_name: "diff_create_collection".to_string(),
            test_prefix: "diff".to_string(),
            defect_line: "[DEFECT: DIFFERENTIAL_MISMATCH] create: rest_ok=True sdk_ok=False".to_string(),
            script: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            endpoint: None,
            param_name: None,
            exit_success: false,
        }];
        let obs = ResultAnalyzer::analyze(&defects);
        assert_eq!(obs.len(), 1);
        assert!(obs[0].expected_behavior.contains("agree"));
    }

    #[test]
    fn test_assimilate_batch_resource_dim() {
        let mut store = ContractStore::new("milvus", "v2.4");
        let defects = vec![BatchDefect {
            test_name: "resource_large_dimension_32768".to_string(),
            test_prefix: "res".to_string(),
            defect_line: "[DEFECT: ILLEGAL_SUCCESS] 32768-dim collection created (may cause OOM)".to_string(),
            script: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            endpoint: None,
            param_name: None,
            exit_success: false,
        }];
        let new_count = ResultAnalyzer::assimilate_batch(&mut store, &defects);
        assert_eq!(new_count, 1);
        assert!(store.range_constraints.iter().any(|arc| {
            arc.constraint.param_name == "dim"
                && arc.source == ConstraintSource::ObservedBehavior
        }));
        assert_eq!(store.observed_behaviors.len(), 1);
    }

    #[test]
    fn test_assimilate_batch_dedup() {
        let mut store = ContractStore::new("milvus", "v2.4");
        let defects = vec![BatchDefect {
            test_name: "resource_large_dimension_32768".to_string(),
            test_prefix: "res".to_string(),
            defect_line: "[DEFECT: ILLEGAL_SUCCESS] 32768-dim collection created".to_string(),
            script: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            endpoint: None,
            param_name: None,
            exit_success: false,
        }];
        ResultAnalyzer::assimilate_batch(&mut store, &defects);
        let new_count = ResultAnalyzer::assimilate_batch(&mut store, &defects);
        assert_eq!(new_count, 0, "Should not assimilate duplicate observations");
    }

    #[test]
    fn test_defect_kind_from_line() {
        assert_eq!(DefectKind::from_defect_line("[DEFECT: ILLEGAL_SUCCESS] foo"), DefectKind::IllegalSuccess);
        assert_eq!(DefectKind::from_defect_line("[DEFECT: SEQUENCE_VIOLATION] bar"), DefectKind::SequenceViolation);
        assert_eq!(DefectKind::from_defect_line("[DEFECT: DIFFERENTIAL_MISMATCH] baz"), DefectKind::DifferentialMismatch);
        assert_eq!(DefectKind::from_defect_line("[DEFECT: METAMORPHIC_VIOLATION] qux"), DefectKind::MetamorphicViolation);
        assert_eq!(DefectKind::from_defect_line("[DEFECT: STATE_LOGIC_VIOLATION] quux"), DefectKind::StateLogicViolation);
        assert_eq!(DefectKind::from_defect_line("[DEFECT: SOMETHING_ELSE] corge"), DefectKind::Unknown);
    }

    #[test]
    fn test_extract_dim_from_description() {
        assert_eq!(ResultAnalyzer::extract_dim_from_description("32768-dim collection created"), Some(32768));
        assert_eq!(ResultAnalyzer::extract_dim_from_description("0-dim collection created"), Some(0));
        assert_eq!(ResultAnalyzer::extract_dim_from_description("no dimension info"), None);
    }

    #[test]
    fn test_parse_from_script_boundary_search_offset() {
        let script = r#"body["offset"] = 0; r = requests.post(f'{BASE}/collections/{c}/points/search', json=body)"#;
        let (ep, param) = ResultAnalyzer::parse_from_script(script);
        assert_eq!(ep, "/search");
        assert_eq!(param, "offset");
    }

    #[test]
    fn test_parse_from_script_boundary_create_size() {
        let script = r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":0,"distance":"Cosine"}})"#;
        let (ep, param) = ResultAnalyzer::parse_from_script(script);
        assert_eq!(ep, "/create_collection");
        assert_eq!(param, "vectors.size");
    }

    #[test]
    fn test_cluster_defects_collapses_same_root_cause() {
        let defects = vec![
            BatchDefect {
                test_name: "offset_below_min".to_string(),
                test_prefix: "boundary".to_string(),
                defect_line: "[DEFECT: ILLEGAL_SUCCESS] offset below min (0) accepted".to_string(),
                script: String::new(), stdout: String::new(), stderr: String::new(),
                endpoint: Some("search".to_string()), param_name: Some("offset".to_string()),
                exit_success: false,
            },
            BatchDefect {
                test_name: "offset_zero".to_string(),
                test_prefix: "boundary".to_string(),
                defect_line: "[DEFECT: ILLEGAL_SUCCESS] offset=0 accepted".to_string(),
                script: String::new(), stdout: String::new(), stderr: String::new(),
                endpoint: Some("search".to_string()), param_name: Some("offset".to_string()),
                exit_success: false,
            },
            BatchDefect {
                test_name: "limit_below_min".to_string(),
                test_prefix: "boundary".to_string(),
                defect_line: "[DEFECT: ILLEGAL_SUCCESS] limit below min accepted".to_string(),
                script: String::new(), stdout: String::new(), stderr: String::new(),
                endpoint: Some("search".to_string()), param_name: Some("limit".to_string()),
                exit_success: false,
            },
        ];
        let clusters = ResultAnalyzer::cluster_defects(&defects);
        // offset cluster (2 instances) + limit cluster (1 instance) = 2 clusters
        assert_eq!(clusters.len(), 2);
        let offset_cluster = clusters.iter().find(|c| c.exemplar.param_name.as_deref() == Some("offset")).unwrap();
        assert_eq!(offset_cluster.count, 2);
        let limit_cluster = clusters.iter().find(|c| c.exemplar.param_name.as_deref() == Some("limit")).unwrap();
        assert_eq!(limit_cluster.count, 1);
    }
}
