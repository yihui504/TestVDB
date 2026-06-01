pub mod analyzer;
pub mod schema;
pub mod openapi;
pub mod store;
pub mod prompt;
pub mod gate;

use anyhow::Context;
use schema::{StructuredContract, EndpointRegistry, BehavioralContract, TypeConstraint, RangeConstraint, StateConstraint, Determinism, BehaviorCategory};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn load_endpoint_registry(path: &Path) -> anyhow::Result<EndpointRegistry> {
    let content = fs::read_to_string(path)
        .context("Failed to read endpoint registry file")?;
    let registry: EndpointRegistry = toml::from_str(&content)
        .context("Failed to parse endpoint registry TOML")?;
    Ok(registry)
}

pub fn load_behavioral_templates<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<BehavioralContract>> {
    let content = fs::read_to_string(path)
        .context("Failed to read behavioral templates file")?;
    let templates: Vec<BehavioralContract> = serde_json::from_str(&content)
        .context("Failed to parse behavioral templates JSON")?;
    Ok(templates)
}

pub fn save_contract_json<P: AsRef<Path>>(
    contract: &StructuredContract,
    path: P,
) -> anyhow::Result<()> {
    let json_string = serde_json::to_string_pretty(contract)
        .context("Failed to serialize contract to JSON")?;
    fs::write(path, json_string).context("Failed to write contract to file")?;
    Ok(())
}

pub fn load_contract_json<P: AsRef<Path>>(path: P) -> anyhow::Result<StructuredContract> {
    let file_content = fs::read_to_string(path).context("Failed to read contract file")?;
    let contract: StructuredContract = serde_json::from_str(&file_content)
        .context("Failed to deserialize contract from JSON")?;
    Ok(contract)
}

pub fn merge_contracts_from_ka(contracts: &[StructuredContract], doc_url: &str) -> StructuredContract {
    let mut merged_type: Vec<TypeConstraint> = Vec::new();
    let mut merged_range: Vec<RangeConstraint> = Vec::new();
    let mut merged_state: Vec<StateConstraint> = Vec::new();
    let mut merged_behavioral: Vec<BehavioralContract> = Vec::new();
    let mut merged_api_endpoints = Vec::new();

    for c in contracts {
        merged_api_endpoints.push(c.api_endpoint.clone());
        for a in &c.assertions {
            let a_lower = a.to_lowercase();
            if a_lower.starts_with("[type]") {
                let content = a[6..].trim();
                let param = content.split_whitespace().next().unwrap_or("unknown");
                merged_type.push(TypeConstraint {
                    param_name: param.to_string(),
                    expected_type: content.to_string(),
                    violation_examples: vec![],
                });
            } else if a_lower.starts_with("[range]") {
                let content = a[7..].trim();
                let param = content.split_whitespace().next().unwrap_or("unknown");
                merged_range.push(RangeConstraint {
                    param_name: param.to_string(),
                    description: content.to_string(),
                    min: None,
                    max: None,
                    violation_examples: vec![],
                });
            } else if a_lower.starts_with("[state") {
                let is_deterministic = a_lower.contains("deterministic") || !a_lower.contains("non-deterministic") && a_lower.starts_with("[state]");
                let content = if let Some(idx) = a.find("] ") {
                    a[idx+2..].trim().to_string()
                } else {
                    a.clone()
                };
                merged_state.push(StateConstraint {
                    description: content,
                    determinism: if is_deterministic {
                        Determinism::Deterministic
                    } else {
                        Determinism::NonDeterministic
                    },
                    setup_script_template: None,
                });
            } else if a_lower.starts_with("[behavior") {
                let category = if a_lower.starts_with("[behavior:state]") {
                    BehaviorCategory::StateConsistency
                } else if a_lower.starts_with("[behavior:semantic]") {
                    BehaviorCategory::SemanticCorrectness
                } else if a_lower.starts_with("[behavior:interface]") {
                    BehaviorCategory::InterfaceConsistency
                } else if a_lower.starts_with("[behavior:diagnostic]") {
                    BehaviorCategory::DiagnosticQuality
                } else {
                    BehaviorCategory::StateConsistency
                };
                let content = if let Some(idx) = a.find("] ") {
                    a[idx+2..].trim().to_string()
                } else {
                    a.clone()
                };
                merged_behavioral.push(BehavioralContract {
                    name: format!("ka_{}", content.split_whitespace().next().unwrap_or("unknown")),
                    category,
                    endpoints: vec![],
                    precondition_script: String::new(),
                    verification_script: String::new(),
                    expected_outcome: content,
                    supersedes: None,
                    mutation_rules: vec![],
                });
            }
        }
    }

    let merged_api_endpoint = merged_api_endpoints.join("+");

    StructuredContract {
        api_endpoint: merged_api_endpoint,
        doc_url: doc_url.to_string(),
        assertions: contracts.iter().flat_map(|c| c.assertions.clone()).collect(),
        type_constraints: merged_type,
        range_constraints: merged_range,
        state_constraints: merged_state,
        state_invariants: contracts.iter().flat_map(|c| c.state_invariants.clone()).collect(),
        behavioral_contracts: {
            let mut bc: Vec<BehavioralContract> = contracts.iter().flat_map(|c| c.behavioral_contracts.clone()).collect();
            bc.extend(merged_behavioral);
            bc
        },
        rejection_policies: HashMap::new(),
        nested_params: HashMap::new(),
    }
}

pub fn parse_constraints_from_assertions(assertions: &[String]) -> (Vec<RangeConstraint>, Vec<TypeConstraint>) {
    let mut range_constraints = Vec::new();
    let mut type_constraints = Vec::new();

    for assertion in assertions {
        let a = assertion.to_lowercase();
        let prefix_stripped = if a.contains("] ") {
            a.split("] ").nth(1).unwrap_or(&a).to_string()
        } else {
            a.clone()
        };

        if a.contains("must be an integer") || a.contains("must be a positive integer") || a.contains("must be integer") || a.contains("integer (not float") {
            if let Some(param) = extract_param_name(&prefix_stripped) {
                type_constraints.push(TypeConstraint {
                    param_name: param,
                    expected_type: "integer".to_string(),
                    violation_examples: vec![],
                });
            }
        } else if a.contains("boolean") || a.contains("must be bool") {
            if let Some(param) = extract_param_name(&prefix_stripped) {
                type_constraints.push(TypeConstraint {
                    param_name: param,
                    expected_type: "boolean".to_string(),
                    violation_examples: vec![],
                });
            }
        } else if a.contains("one of:") || a.contains("one of ") {
            // "param must be one of: a, b, c" or "param must be a, b, or c"
            if let Some(param) = extract_param_name(&prefix_stripped) {
                let values: String = if let Some(idx) = a.find("one of:") {
                    a[idx + 7..].trim().to_string()
                } else if let Some(idx) = a.find("one of ") {
                    a[idx + 7..].trim().to_string()
                } else {
                    String::new()
                };
                type_constraints.push(TypeConstraint {
                    param_name: param,
                    expected_type: format!("enum({})", values),
                    violation_examples: vec![],
                });
            }
        } else if a.contains("must be >") || a.contains("must be greater") {
            let min_val = extract_numeric_constraint(&prefix_stripped, ">");
            if let Some(param) = extract_param_name(&prefix_stripped) {
                range_constraints.push(RangeConstraint {
                    param_name: param,
                    description: assertion.clone(),
                    min: Some((min_val + 1.0).max(min_val)),
                    max: None,
                    violation_examples: vec![],
                });
            }
        } else if a.contains("must be >=") || a.contains("must be non-negative") || a.contains("must not be 0 or negative") || a.contains("must not be 0") {
            let min_val = extract_numeric_constraint(&prefix_stripped, ">=");
            if let Some(param) = extract_param_name(&prefix_stripped) {
                range_constraints.push(RangeConstraint {
                    param_name: param,
                    description: assertion.clone(),
                    min: Some(min_val),
                    max: None,
                    violation_examples: vec![],
                });
            }
        } else if a.contains("between ") && a.contains(" and ") {
            if let Some((min_val, max_val)) = extract_between_constraint(&prefix_stripped) {
                if let Some(param) = extract_param_name(&prefix_stripped) {
                    range_constraints.push(RangeConstraint {
                        param_name: param,
                        description: assertion.clone(),
                        min: Some(min_val),
                        max: Some(max_val),
                        violation_examples: vec![],
                    });
                }
            }
        }
    }

    (range_constraints, type_constraints)
}

fn extract_param_name(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() { return None; }

    let skip_words: std::collections::HashSet<&str> = ["vector", "search", "create", "upsert", "delete", "update", "must", "should", "the", "a", "an", "is", "be", "when"].iter().copied().collect();

    for w in &words {
        let clean = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.');
        if clean.is_empty() || clean.chars().any(|c| c.is_uppercase()) { continue; }
        if skip_words.contains(clean) { continue; }
        if clean.parse::<f64>().is_ok() { continue; }

        if clean == "params.hnsw_ef" || clean == "hnsw_ef" { return Some("hnsw_ef".to_string()); }
        if clean == "params.exact" || clean == "exact" { return Some("exact".to_string()); }
        if clean == "score_threshold" || clean == "score_threshold" { return Some("score_threshold".to_string()); }

        if ["limit", "offset", "top", "dimension", "vectors.size", "shard_number", "replication_factor"].contains(&clean) {
            return Some(clean.to_string());
        }
        if clean.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') && clean.len() >= 2 {
            let stripped = if clean.starts_with("params.") {
                &clean["params.".len()..]
            } else if clean.contains(".params.") {
                let idx = clean.find(".params.").expect("contains check guarantees find succeeds");
                &clean[idx + ".params.".len()..]
            } else {
                clean
            };
            return Some(stripped.to_string());
        }
    }
    None
}

fn extract_numeric_constraint(text: &str, op: &str) -> f64 {
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_numeric() && c != '.' && c != '-');
        if let Ok(val) = clean.parse::<f64>() {
            if op == ">" { return val; }
            return val;
        }
    }

    match op {
        ">=" | ">" => 0.0,
        _ => 0.0,
    }
}

fn extract_between_constraint(text: &str) -> Option<(f64, f64)> {
    let parts: Vec<f64> = text.split_whitespace()
        .filter_map(|w| {
            let clean = w.trim_matches(|c: char| !c.is_numeric() && c != '.' && c != '-');
            clean.parse::<f64>().ok()
        })
        .collect();
    if parts.len() >= 2 {
        Some((parts[0], parts[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::RangeConstraint;
    use tempfile::tempdir;

    #[test]
    fn test_serialize_deserialize_contract() {
        let json_str = r#"{
            "api_endpoint": "create_collection",
            "doc_url": "https://milvus.io/docs/create_collection.md",
            "assertions": [
                "dimension must be > 0"
            ]
        }"#;

        let contract: StructuredContract = serde_json::from_str(json_str).unwrap();
        assert_eq!(contract.api_endpoint, "create_collection");
        assert_eq!(
            contract.doc_url,
            "https://milvus.io/docs/create_collection.md"
        );
        assert_eq!(contract.assertions.len(), 1);
        assert_eq!(contract.assertions[0], "dimension must be > 0");
    }

    #[test]
    fn test_save_and_load_contract() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("contract.json");

        let contract = StructuredContract {
            api_endpoint: "search".to_string(),
            doc_url: "https://qdrant.tech/documentation/search/".to_string(),
            assertions: vec!["top must be > 0".to_string()],
            type_constraints: vec![],
            range_constraints: vec![RangeConstraint {
                param_name: "top".to_string(),
                description: "top must be > 0".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec!["0".to_string()],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: HashMap::new(),
            nested_params: HashMap::new(),
        };

        save_contract_json(&contract, &file_path).unwrap();
        let loaded = load_contract_json(&file_path).unwrap();

        assert_eq!(contract, loaded);
    }

    #[test]
    fn test_extract_param_name_nested_searchparams() {
        assert_eq!(
            extract_param_name("searchparams.params.nprobe must be > 0"),
            Some("nprobe".to_string())
        );
        assert_eq!(
            extract_param_name("params.nprobe must be > 0"),
            Some("nprobe".to_string())
        );
        assert_eq!(
            extract_param_name("limit must be > 0"),
            Some("limit".to_string())
        );
        assert_eq!(
            extract_param_name("vectors.size must be > 0"),
            Some("vectors.size".to_string())
        );
    }

    #[test]
    fn test_parse_nprobe_range_constraint() {
        let assertions = vec![
            "[SEARCH] searchParams.params.nprobe must be > 0".to_string(),
        ];
        let (ranges, _types) = parse_constraints_from_assertions(&assertions);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].param_name, "nprobe");
        assert!(ranges[0].min.is_some());
    }
}
