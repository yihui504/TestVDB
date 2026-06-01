use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RejectionPolicy {
    #[default]
    Reject,
    Ignore,
}

fn deserialize_f64_or_string<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de;

    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    match val {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_f64()),
        Some(serde_json::Value::String(s)) => s.parse::<f64>().map(Some).map_err(|_| {
            de::Error::custom(format!("cannot parse '{}' as f64 for min/max field", s))
        }),
        Some(other) => Err(de::Error::custom(format!(
            "expected number or string for min/max field, got {:?}",
            other
        ))),
    }
}

fn serialize_f64_option_as_string<S>(val: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match val {
        None => serializer.serialize_none(),
        Some(v) => serializer.serialize_str(&v.to_string()),
    }
}

/// A structured contract extracted from official vector database documentation.
///
/// Contains three layers of constraints:
/// 1. Type constraints: parameter JSON type must match expectation
/// 2. Range constraints: parameter value within valid range
/// 3. State constraints: preconditions before endpoint invocation
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct StructuredContract {
    pub api_endpoint: String,
    pub doc_url: String,
    #[serde(default)]
    pub assertions: Vec<String>,
    #[serde(default)]
    pub type_constraints: Vec<TypeConstraint>,
    #[serde(default)]
    pub range_constraints: Vec<RangeConstraint>,
    #[serde(default)]
    pub state_constraints: Vec<StateConstraint>,
    #[serde(default)]
    pub state_invariants: Vec<StateInvariant>,
    #[serde(default)]
    pub behavioral_contracts: Vec<BehavioralContract>,
    #[serde(default)]
    pub rejection_policies: HashMap<String, RejectionPolicy>,
    #[serde(default)]
    pub nested_params: HashMap<String, HashMap<String, Vec<String>>>,
}

/// Layer 1: Parameter JSON type must match the expected type.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct TypeConstraint {
    pub param_name: String,
    pub expected_type: String,
    #[serde(default)]
    pub violation_examples: Vec<String>,  // FUTURE: populate from tag→layer parser
}

/// Layer 2: Parameter type is correct but value is out of range.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RangeConstraint {
    pub param_name: String,
    pub description: String,
    #[serde(default, deserialize_with = "deserialize_f64_or_string", serialize_with = "serialize_f64_option_as_string")]
    pub min: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_f64_or_string", serialize_with = "serialize_f64_option_as_string")]
    pub max: Option<f64>,
    #[serde(default)]
    pub violation_examples: Vec<String>,
}

/// Whether a state constraint can be deterministically verified.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Determinism {
    Deterministic,
    NonDeterministic,
}

/// Type of invariant check the Oracle can perform.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    CountConsistency,
    ExistenceCheck,
    ValueRange,
    Idempotency,
}

/// An explicit state invariant that the Oracle can automatically verify.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct StateInvariant {
    pub name: String,
    pub check_type: CheckType,
    pub endpoint: String,
    #[serde(default)]
    pub precondition: String,
    pub assertion_script: String,
}

/// Layer 3: Preconditions before calling the endpoint.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct StateConstraint {
    pub description: String,
    pub determinism: Determinism,
    #[serde(default)]
    pub setup_script_template: Option<String>,  // FUTURE: auto-generate setup script
}

/// An endpoint entry in the endpoint registry.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct EndpointEntry {
    pub name: String,
    pub api_path: String,
    pub docs_url: String,
    pub category: String,
}

/// Category of behavioral contract.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorCategory {
    StateConsistency,
    SemanticCorrectness,
    InterfaceConsistency,
    DiagnosticQuality,
}

/// A mutation rule that tests a negative variant of a behavioral contract.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct MutationRule {
    pub name: String,
    pub target_field: String,
    pub mutation_type: String,
    pub mutated_script: String,
    pub expected_result: String,
    pub defect_type: String,
}

/// A behavioral contract defining a system property that should always hold.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct BehavioralContract {
    pub name: String,
    pub category: BehaviorCategory,
    #[serde(default)]
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub precondition_script: String,
    pub verification_script: String,
    #[serde(default)]
    pub expected_outcome: String,
    #[serde(default)]
    pub supersedes: Option<String>,
    #[serde(default)]
    pub mutation_rules: Vec<MutationRule>,
}

/// Result of a behavioral contract test execution.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct BehaviorTestResult {
    pub contract_name: String,
    pub category: BehaviorCategory,
    pub result: String,
}

/// Registry of all REST endpoints to extract contracts for.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct EndpointRegistry {
    pub target: String,
    pub version: String,
    pub endpoints: Vec<EndpointEntry>,
}
