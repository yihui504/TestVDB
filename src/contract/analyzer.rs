use crate::contract::store::{AnnotatedRangeConstraint, AnnotatedTypeConstraint, ContractStore, ObservedBehavior};
use crate::contract::schema::{RangeConstraint, TypeConstraint};
use crate::contract::store::{Confidence, ConstraintSource};
use serde::{Deserialize, Serialize};

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
}

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

    fn extract_context(test_name: &str, prefix: &str) -> (String, String) {
        match prefix {
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
}
