use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::schema::{
    BehavioralContract, EndpointEntry,
    RangeConstraint, StateConstraint, StateInvariant, StructuredContract,
    TypeConstraint,
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintSource {
    ExplicitDoc,
    OpenapiDerived,
    ObservedBehavior,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AnnotatedTypeConstraint {
    pub constraint: TypeConstraint,
    pub endpoint: String,
    pub source: ConstraintSource,
    pub confidence: Confidence,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AnnotatedRangeConstraint {
    pub constraint: RangeConstraint,
    pub endpoint: String,
    pub source: ConstraintSource,
    pub confidence: Confidence,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AnnotatedStateConstraint {
    pub constraint: StateConstraint,
    pub endpoint: String,
    pub source: ConstraintSource,
    pub confidence: Confidence,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AnnotatedStateInvariant {
    pub invariant: StateInvariant,
    pub source: ConstraintSource,
    pub confidence: Confidence,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AnnotatedBehavioralContract {
    pub contract: BehavioralContract,
    pub source: ConstraintSource,
    pub confidence: Confidence,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ObservedBehavior {
    pub endpoint: String,
    pub param_name: String,
    pub description: String,
    pub observed_value: String,
    pub expected_behavior: String,
    pub actual_behavior: String,
    pub is_violation: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ViolationTarget {
    pub endpoint: String,
    pub param_name: String,
    pub violation_type: ViolationType,
    pub test_value: String,
    pub defect_marker: String,
    pub source_constraint: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    NullInjection,
    MissingRequired,
    TypeConfusion,
    BelowMin,
    AboveMax,
    ZeroValue,
    NegativeValue,
    InvalidEnum,
    EmptyString,
    Oversized,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ContractStore {
    pub target: String,
    pub version: String,
    pub endpoints: Vec<EndpointEntry>,
    pub type_constraints: Vec<AnnotatedTypeConstraint>,
    pub range_constraints: Vec<AnnotatedRangeConstraint>,
    pub state_constraints: Vec<AnnotatedStateConstraint>,
    pub state_invariants: Vec<AnnotatedStateInvariant>,
    pub behavioral_contracts: Vec<AnnotatedBehavioralContract>,
    pub observed_behaviors: Vec<ObservedBehavior>,
    #[serde(default)]
    pub required_params: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub enum_values: HashMap<String, Vec<String>>,
}

impl ContractStore {
    pub fn new(target: &str, version: &str) -> Self {
        Self {
            target: target.to_string(),
            version: version.to_string(),
            endpoints: Vec::new(),
            type_constraints: Vec::new(),
            range_constraints: Vec::new(),
            state_constraints: Vec::new(),
            state_invariants: Vec::new(),
            behavioral_contracts: Vec::new(),
            observed_behaviors: Vec::new(),
            required_params: HashMap::new(),
            enum_values: HashMap::new(),
        }
    }

    pub fn from_structured_contracts(
        target: &str,
        version: &str,
        contracts: &[StructuredContract],
        source: ConstraintSource,
        confidence: Confidence,
    ) -> Self {
        let mut store = Self::new(target, version);

        for contract in contracts {
            let endpoint = contract.api_endpoint.clone();

            for tc in &contract.type_constraints {
                store.type_constraints.push(AnnotatedTypeConstraint {
                    constraint: tc.clone(),
                    endpoint: endpoint.clone(),
                    source: source.clone(),
                    confidence: confidence.clone(),
                });
            }

            for rc in &contract.range_constraints {
                store.range_constraints.push(AnnotatedRangeConstraint {
                    constraint: rc.clone(),
                    endpoint: endpoint.clone(),
                    source: source.clone(),
                    confidence: confidence.clone(),
                });
            }

            for sc in &contract.state_constraints {
                store.state_constraints.push(AnnotatedStateConstraint {
                    constraint: sc.clone(),
                    endpoint: endpoint.clone(),
                    source: source.clone(),
                    confidence: confidence.clone(),
                });
            }

            for si in &contract.state_invariants {
                store.state_invariants.push(AnnotatedStateInvariant {
                    invariant: si.clone(),
                    source: source.clone(),
                    confidence: confidence.clone(),
                });
            }

            for bc in &contract.behavioral_contracts {
                store.behavioral_contracts.push(AnnotatedBehavioralContract {
                    contract: bc.clone(),
                    source: source.clone(),
                    confidence: confidence.clone(),
                });
            }
        }

        store
    }

    pub fn merge(&mut self, other: ContractStore) {
        for atc in other.type_constraints {
            let dup = self.type_constraints.iter().any(|existing| {
                existing.endpoint == atc.endpoint && existing.constraint.param_name == atc.constraint.param_name
            });
            if !dup {
                self.type_constraints.push(atc);
            }
        }

        for arc in other.range_constraints {
            let dup = self.range_constraints.iter().any(|existing| {
                existing.endpoint == arc.endpoint && existing.constraint.param_name == arc.constraint.param_name
            });
            if !dup {
                self.range_constraints.push(arc);
            }
        }

        self.state_constraints.extend(other.state_constraints);
        self.state_invariants.extend(other.state_invariants);
        self.behavioral_contracts.extend(other.behavioral_contracts);

        for obs in other.observed_behaviors {
            let dup = self.observed_behaviors.iter().any(|existing| {
                existing.description == obs.description
            });
            if !dup {
                self.observed_behaviors.push(obs);
            }
        }

        for (k, v) in other.required_params {
            let entry = self.required_params.entry(k).or_default();
            for param in v {
                if !entry.contains(&param) {
                    entry.push(param);
                }
            }
        }
        for (k, v) in other.enum_values {
            let entry = self.enum_values.entry(k).or_default();
            for val in v {
                if !entry.contains(&val) {
                    entry.push(val);
                }
            }
        }

        self.endpoints.extend(other.endpoints);
    }

    pub fn set_required_params(&mut self, endpoint: &str, params: Vec<String>) {
        self.required_params
            .insert(endpoint.to_string(), params);
    }

    pub fn set_enum_values(&mut self, param_name: &str, values: Vec<String>) {
        self.enum_values
            .insert(param_name.to_string(), values);
    }

    pub fn query_violations(&self) -> Vec<ViolationTarget> {
        let mut targets = Vec::new();

        for atc in &self.type_constraints {
            let param = &atc.constraint.param_name;
            let endpoint = &atc.endpoint;
            let expected_type = atc.constraint.expected_type.to_lowercase();

            targets.push(ViolationTarget {
                endpoint: endpoint.clone(),
                param_name: param.clone(),
                violation_type: ViolationType::NullInjection,
                test_value: "null".to_string(),
                defect_marker: "ILLEGAL_SUCCESS".to_string(),
                source_constraint: format!("type: {} must be {}", param, expected_type),
            });

            if expected_type.contains("int") {
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::TypeConfusion,
                    test_value: r#""not_a_number""#.to_string(),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("type: {} must be {}", param, expected_type),
                });
            } else if expected_type.contains("string") {
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::TypeConfusion,
                    test_value: "12345".to_string(),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("type: {} must be {}", param, expected_type),
                });
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::EmptyString,
                    test_value: r#""""#.to_string(),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("type: {} must be {}", param, expected_type),
                });
            } else if expected_type.contains("bool") {
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::TypeConfusion,
                    test_value: r#""not_a_bool""#.to_string(),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("type: {} must be {}", param, expected_type),
                });
            } else if expected_type.contains("float") || expected_type.contains("double") {
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::TypeConfusion,
                    test_value: r#""not_a_float""#.to_string(),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("type: {} must be {}", param, expected_type),
                });
            }
        }

        for arc in &self.range_constraints {
            let param = &arc.constraint.param_name;
            let endpoint = &arc.endpoint;

            if let Some(min) = arc.constraint.min {
                if min > 0.0 {
                    targets.push(ViolationTarget {
                        endpoint: endpoint.clone(),
                        param_name: param.clone(),
                        violation_type: ViolationType::BelowMin,
                        test_value: format!("{}", (min - 1.0) as i64),
                        defect_marker: "ILLEGAL_SUCCESS".to_string(),
                        source_constraint: format!("range: {} min={}", param, min),
                    });
                }
                if min == 1.0 {
                    targets.push(ViolationTarget {
                        endpoint: endpoint.clone(),
                        param_name: param.clone(),
                        violation_type: ViolationType::ZeroValue,
                        test_value: "0".to_string(),
                        defect_marker: "ILLEGAL_SUCCESS".to_string(),
                        source_constraint: format!("range: {} min={}", param, min),
                    });
                }
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::NegativeValue,
                    test_value: "-1".to_string(),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("range: {} min={}", param, min),
                });
            }

            if let Some(max) = arc.constraint.max {
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::AboveMax,
                    test_value: format!("{}", (max + 1.0) as i64),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("range: {} max={}", param, max),
                });
            }
        }

        for (endpoint, params) in &self.required_params {
            for param in params {
                targets.push(ViolationTarget {
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: ViolationType::MissingRequired,
                    test_value: "<remove_field>".to_string(),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    source_constraint: format!("required: {}", param),
                });
            }
        }

        for (param_name, values) in &self.enum_values {
            if values.is_empty() {
                continue;
            }
            let endpoint = self
                .type_constraints
                .iter()
                .find(|atc| atc.constraint.param_name == *param_name)
                .map(|atc| atc.endpoint.clone())
                .unwrap_or_default();

            targets.push(ViolationTarget {
                endpoint,
                param_name: param_name.clone(),
                violation_type: ViolationType::InvalidEnum,
                test_value: r#""INVALID_ENUM_VALUE_42""#.to_string(),
                defect_marker: "ILLEGAL_SUCCESS".to_string(),
                source_constraint: format!("enum: {} in {:?}", param_name, values),
            });
        }

        targets
    }

    pub fn query_violations_for_endpoint(&self, endpoint: &str) -> Vec<ViolationTarget> {
        self.query_violations()
            .into_iter()
            .filter(|t| t.endpoint == endpoint)
            .collect()
    }

    pub fn assimilate_observation(&mut self, observation: ObservedBehavior) {
        if observation.is_violation {
            let tc = TypeConstraint {
                param_name: observation.param_name.clone(),
                expected_type: format!(
                    "observed: {} should {} but actually {}",
                    observation.param_name, observation.expected_behavior, observation.actual_behavior
                ),
                violation_examples: vec![observation.observed_value.clone()],
            };
            self.type_constraints.push(AnnotatedTypeConstraint {
                constraint: tc,
                endpoint: observation.endpoint.clone(),
                source: ConstraintSource::ObservedBehavior,
                confidence: Confidence::High,
            });
        }
        self.observed_behaviors.push(observation);
    }

    pub fn persist<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize ContractStore")?;
        fs::write(path, json).context("Failed to write ContractStore to file")?;
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path)
            .context("Failed to read ContractStore file")?;
        let store: ContractStore = serde_json::from_str(&content)
            .context("Failed to deserialize ContractStore")?;
        Ok(store)
    }

    pub fn constraint_stats(&self) -> String {
        format!(
            "ContractStore[{} v{}]: {} endpoints, {} type_constraints, {} range_constraints, {} state_constraints, {} state_invariants, {} behavioral_contracts, {} observed_behaviors, {} required_params, {} enum_values → {} violation_targets",
            self.target,
            self.version,
            self.endpoints.len(),
            self.type_constraints.len(),
            self.range_constraints.len(),
            self.state_constraints.len(),
            self.state_invariants.len(),
            self.behavioral_contracts.len(),
            self.observed_behaviors.len(),
            self.required_params.len(),
            self.enum_values.len(),
            self.query_violations().len(),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedParamName {
    pub endpoint: String,
    pub json_path: String,
}

pub fn parse_param_name(param_name: &str) -> ParsedParamName {
    if let Some(dot_pos) = param_name.find('.') {
        ParsedParamName {
            endpoint: param_name[..dot_pos].to_string(),
            json_path: param_name[dot_pos + 1..].to_string(),
        }
    } else {
        ParsedParamName {
            endpoint: String::new(),
            json_path: param_name.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_contract() -> StructuredContract {
        StructuredContract {
            api_endpoint: "/v2/vectordb/entities/search".to_string(),
            doc_url: "https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/Query.md".to_string(),
            assertions: vec![],
            type_constraints: vec![
                TypeConstraint {
                    param_name: "limit".to_string(),
                    expected_type: "integer".to_string(),
                    violation_examples: vec![],
                },
                TypeConstraint {
                    param_name: "filter".to_string(),
                    expected_type: "string".to_string(),
                    violation_examples: vec![],
                },
            ],
            range_constraints: vec![
                RangeConstraint {
                    param_name: "limit".to_string(),
                    description: "limit must be >= 1 and <= 16384".to_string(),
                    min: Some(1.0),
                    max: Some(16384.0),
                    violation_examples: vec![],
                },
            ],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        }
    }

    #[test]
    fn test_contract_store_from_contracts() {
        let contract = make_test_contract();
        let store = ContractStore::from_structured_contracts(
            "milvus",
            "v2.4",
            &[contract],
            ConstraintSource::ExplicitDoc,
            Confidence::High,
        );

        assert_eq!(store.type_constraints.len(), 2);
        assert_eq!(store.range_constraints.len(), 1);
        assert_eq!(store.target, "milvus");
    }

    #[test]
    fn test_query_violations() {
        let contract = make_test_contract();
        let mut store = ContractStore::from_structured_contracts(
            "milvus",
            "v2.4",
            &[contract],
            ConstraintSource::ExplicitDoc,
            Confidence::High,
        );

        store.set_required_params(
            "/v2/vectordb/entities/query",
            vec!["collectionName".to_string(), "filter".to_string()],
        );
        store.set_enum_values("metricType", vec!["L2".to_string(), "COSINE".to_string(), "IP".to_string()]);

        let violations = store.query_violations();

        let null_injections: Vec<_> = violations.iter().filter(|v| matches!(v.violation_type, ViolationType::NullInjection)).collect();
        assert!(!null_injections.is_empty(), "Should have null injection violations");

        let type_confusions: Vec<_> = violations.iter().filter(|v| matches!(v.violation_type, ViolationType::TypeConfusion)).collect();
        assert!(!type_confusions.is_empty(), "Should have type confusion violations");

        let range_violations: Vec<_> = violations.iter().filter(|v| matches!(v.violation_type, ViolationType::BelowMin | ViolationType::AboveMax | ViolationType::ZeroValue | ViolationType::NegativeValue)).collect();
        assert!(!range_violations.is_empty(), "Should have range violations");

        let missing: Vec<_> = violations.iter().filter(|v| matches!(v.violation_type, ViolationType::MissingRequired)).collect();
        assert_eq!(missing.len(), 2, "Should have 2 missing required violations");

        let invalid_enum: Vec<_> = violations.iter().filter(|v| matches!(v.violation_type, ViolationType::InvalidEnum)).collect();
        assert_eq!(invalid_enum.len(), 1, "Should have 1 invalid enum violation");
    }

    #[test]
    fn test_query_violations_for_endpoint() {
        let contract = make_test_contract();
        let store = ContractStore::from_structured_contracts(
            "milvus",
            "v2.4",
            &[contract],
            ConstraintSource::ExplicitDoc,
            Confidence::High,
        );

        let violations = store.query_violations_for_endpoint("/v2/vectordb/entities/search");
        assert!(!violations.is_empty());

        let other = store.query_violations_for_endpoint("/v2/vectordb/collections/create");
        assert!(other.is_empty());
    }

    #[test]
    fn test_assimilate_observation() {
        let contract = make_test_contract();
        let mut store = ContractStore::from_structured_contracts(
            "milvus",
            "v2.4",
            &[contract],
            ConstraintSource::ExplicitDoc,
            Confidence::High,
        );

        let initial_tc_count = store.type_constraints.len();

        store.assimilate_observation(ObservedBehavior {
            endpoint: "/v2/vectordb/entities/query".to_string(),
            param_name: "filter".to_string(),
            description: "filter=null returns all entities".to_string(),
            observed_value: "null".to_string(),
            expected_behavior: "reject with error".to_string(),
            actual_behavior: "returns all entities".to_string(),
            is_violation: true,
        });

        assert_eq!(store.type_constraints.len(), initial_tc_count + 1);
        assert_eq!(store.observed_behaviors.len(), 1);
        let new_tc = &store.type_constraints.last().unwrap();
        assert!(matches!(new_tc.source, ConstraintSource::ObservedBehavior));
        assert!(matches!(new_tc.confidence, Confidence::High));
    }

    #[test]
    fn test_persist_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("contract_store.json");

        let contract = make_test_contract();
        let mut store = ContractStore::from_structured_contracts(
            "milvus",
            "v2.4",
            &[contract],
            ConstraintSource::ExplicitDoc,
            Confidence::High,
        );
        store.set_required_params(
            "/v2/vectordb/entities/query",
            vec!["collectionName".to_string()],
        );

        store.persist(&path).unwrap();
        let loaded = ContractStore::load(&path).unwrap();

        assert_eq!(store, loaded);
    }

    #[test]
    fn test_merge_stores() {
        let contract1 = StructuredContract {
            api_endpoint: "/v2/vectordb/entities/search".to_string(),
            doc_url: "".to_string(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };

        let contract2 = StructuredContract {
            api_endpoint: "/v2/vectordb/collections/create".to_string(),
            doc_url: "".to_string(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "dim".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };

        let mut store1 = ContractStore::from_structured_contracts(
            "milvus", "v2.4", &[contract1], ConstraintSource::ExplicitDoc, Confidence::High,
        );
        let store2 = ContractStore::from_structured_contracts(
            "milvus", "v2.4", &[contract2], ConstraintSource::OpenapiDerived, Confidence::Medium,
        );

        store1.merge(store2);
        assert_eq!(store1.type_constraints.len(), 2);
    }

    #[test]
    fn test_merge_dedup() {
        let contract = StructuredContract {
            api_endpoint: "/v2/vectordb/entities/search".to_string(),
            doc_url: "".to_string(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit range".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };

        let mut store1 = ContractStore::from_structured_contracts(
            "milvus", "v2.4", &[contract.clone()], ConstraintSource::ExplicitDoc, Confidence::High,
        );
        let store2 = ContractStore::from_structured_contracts(
            "milvus", "v2.4", &[contract], ConstraintSource::OpenapiDerived, Confidence::Medium,
        );

        store1.merge(store2);
        assert_eq!(store1.type_constraints.len(), 1, "type_constraints should dedup by endpoint+param_name");
        assert_eq!(store1.range_constraints.len(), 1, "range_constraints should dedup by endpoint+param_name");
    }

    #[test]
    fn test_merge_dedup_required_params_and_enum() {
        let mut store1 = ContractStore::new("milvus", "v2.4");
        store1.set_required_params("/ep1", vec!["p1".to_string(), "p2".to_string()]);
        store1.set_enum_values("metricType", vec!["L2".to_string(), "COSINE".to_string()]);

        let mut store2 = ContractStore::new("milvus", "v2.4");
        store2.set_required_params("/ep1", vec!["p2".to_string(), "p3".to_string()]);
        store2.set_enum_values("metricType", vec!["COSINE".to_string(), "IP".to_string()]);

        store1.merge(store2);
        let ep1_params = store1.required_params.get("/ep1").unwrap();
        assert_eq!(ep1_params.len(), 3, "required_params should dedup: p1, p2, p3");
        assert!(ep1_params.contains(&"p1".to_string()));
        assert!(ep1_params.contains(&"p2".to_string()));
        assert!(ep1_params.contains(&"p3".to_string()));

        let metric_vals = store1.enum_values.get("metricType").unwrap();
        assert_eq!(metric_vals.len(), 3, "enum_values should dedup: L2, COSINE, IP");
        assert!(metric_vals.contains(&"L2".to_string()));
        assert!(metric_vals.contains(&"COSINE".to_string()));
        assert!(metric_vals.contains(&"IP".to_string()));
    }

    #[test]
    fn test_parse_param_name_with_endpoint() {
        let parsed = parse_param_name("create_collection.optimizers_config.indexing_threshold");
        assert_eq!(parsed.endpoint, "create_collection");
        assert_eq!(parsed.json_path, "optimizers_config.indexing_threshold");
    }

    #[test]
    fn test_parse_param_name_without_endpoint() {
        let parsed = parse_param_name("limit");
        assert_eq!(parsed.endpoint, "");
        assert_eq!(parsed.json_path, "limit");
    }

    #[test]
    fn test_parse_param_name_single_dot() {
        let parsed = parse_param_name("search_points.limit");
        assert_eq!(parsed.endpoint, "search_points");
        assert_eq!(parsed.json_path, "limit");
    }
}
