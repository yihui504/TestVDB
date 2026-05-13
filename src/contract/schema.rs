use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub min: Option<String>,  // FUTURE: parse from description (e.g., "> 0" → Some("0"))
    #[serde(default)]
    pub max: Option<String>,  // FUTURE: parse from description
    #[serde(default)]
    pub violation_examples: Vec<String>,  // FUTURE: auto-generate boundary values
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

/// Registry of all REST endpoints to extract contracts for.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct EndpointRegistry {
    pub target: String,
    pub version: String,
    pub endpoints: Vec<EndpointEntry>,
}
