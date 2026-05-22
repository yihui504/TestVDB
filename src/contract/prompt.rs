use crate::contract::store::{ContractStore, ViolationTarget, ViolationType};
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStrategy {
    BoundaryValue,
    TypeConfusion,
    MissingRequired,
    InvalidEnum,
    StateConsistency,
    Metamorphic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationScenario {
    pub name: String,
    pub endpoint: String,
    pub param_name: String,
    pub violation_type: ViolationType,
    pub test_value: String,
    pub description: String,
    pub pre_condition: String,
    pub expected_behavior: String,
    pub defect_marker: String,
    pub strategy: TestStrategy,
}

#[derive(Debug, Clone)]
pub struct ConstraintPrompt {
    pub system_prompt: String,
    pub violation_scenarios: Vec<ViolationScenario>,
    pub initial_message: String,
}

pub struct PromptGenerator {
    store: ContractStore,
    style: TargetStyle,
}

impl PromptGenerator {
    pub fn new(store: ContractStore, style: TargetStyle) -> Self {
        Self { store, style }
    }

    pub fn generate(&self) -> ConstraintPrompt {
        let scenarios = self.enumerate_scenarios();
        let system_prompt = self.build_system_prompt(&scenarios);
        let initial_message = self.build_initial_message(&scenarios);
        ConstraintPrompt {
            system_prompt,
            violation_scenarios: scenarios,
            initial_message,
        }
    }

    pub fn enumerate_scenarios(&self) -> Vec<ViolationScenario> {
        let mut scenarios = Vec::new();
        let violations = self.store.query_violations();

        let mut groups: HashMap<(String, String), Vec<&ViolationTarget>> = HashMap::new();
        for v in &violations {
            groups
                .entry((v.endpoint.clone(), v.param_name.clone()))
                .or_default()
                .push(v);
        }

        for ((endpoint, param), targets) in &groups {
            for target in targets {
                scenarios.push(ViolationScenario {
                    name: format!("{}_{}", param, fmt_vtype(&target.violation_type)),
                    endpoint: endpoint.clone(),
                    param_name: param.clone(),
                    violation_type: target.violation_type.clone(),
                    test_value: target.test_value.clone(),
                    description: describe_violation(target),
                    pre_condition: derive_pre_condition(endpoint),
                    expected_behavior: format!(
                        "Server should reject {}={}",
                        param, target.test_value
                    ),
                    defect_marker: target.defect_marker.clone(),
                    strategy: classify_strategy(&target.violation_type),
                });
            }

            if targets.len() < 3 {
                let existing: Vec<_> = targets.iter().map(|t| &t.violation_type).collect();
                self.supplement_scenarios(&mut scenarios, endpoint, param, &existing);
            }
        }

        for asc in &self.store.state_constraints {
            let keyword = asc
                .constraint
                .description
                .split_whitespace()
                .next()
                .unwrap_or("unknown");
            scenarios.push(ViolationScenario {
                name: format!("state_{}", keyword),
                endpoint: asc.endpoint.clone(),
                param_name: "state".to_string(),
                violation_type: ViolationType::NullInjection,
                test_value: "violate_precondition".to_string(),
                description: format!("State constraint: {}", asc.constraint.description),
                pre_condition: asc.constraint.description.clone(),
                expected_behavior: "Server should enforce state precondition".to_string(),
                defect_marker: "STATE_VIOLATION".to_string(),
                strategy: TestStrategy::StateConsistency,
            });
        }

        for abc in &self.store.behavioral_contracts {
            let keyword = abc
                .contract
                .name
                .split_whitespace()
                .next()
                .unwrap_or("unknown");
            let ep = abc.contract.endpoints.first().cloned().unwrap_or_default();
            let pre = if abc.contract.precondition_script.is_empty() {
                derive_pre_condition(&ep)
            } else {
                abc.contract.precondition_script.clone()
            };
            scenarios.push(ViolationScenario {
                name: format!("metamorphic_{}", keyword),
                endpoint: ep,
                param_name: "behavioral".to_string(),
                violation_type: ViolationType::NullInjection,
                test_value: "mutate_and_verify".to_string(),
                description: format!(
                    "Behavioral: {} — {}",
                    abc.contract.name, abc.contract.expected_outcome
                ),
                pre_condition: pre,
                expected_behavior: abc.contract.expected_outcome.clone(),
                defect_marker: "DATA_CORRUPTION".to_string(),
                strategy: TestStrategy::Metamorphic,
            });
        }

        scenarios
    }

    fn supplement_scenarios(
        &self,
        scenarios: &mut Vec<ViolationScenario>,
        endpoint: &str,
        param: &str,
        existing: &[&ViolationType],
    ) {
        if !existing
            .iter()
            .any(|t| matches!(t, ViolationType::NullInjection))
        {
            scenarios.push(ViolationScenario {
                name: format!("{}_null", param),
                endpoint: endpoint.to_string(),
                param_name: param.to_string(),
                violation_type: ViolationType::NullInjection,
                test_value: "null".to_string(),
                description: format!("Set {}=null, server should reject", param),
                pre_condition: derive_pre_condition(endpoint),
                expected_behavior: format!("Server should reject {}=null", param),
                defect_marker: "ILLEGAL_SUCCESS".to_string(),
                strategy: TestStrategy::TypeConfusion,
            });
        }

        if !existing
            .iter()
            .any(|t| matches!(t, ViolationType::EmptyString))
        {
            let is_string = self.store.type_constraints.iter().any(|atc| {
                atc.constraint.param_name == param
                    && atc.constraint.expected_type.to_lowercase().contains("string")
            });
            if is_string {
                scenarios.push(ViolationScenario {
                    name: format!("{}_empty", param),
                    endpoint: endpoint.to_string(),
                    param_name: param.to_string(),
                    violation_type: ViolationType::EmptyString,
                    test_value: "\"\"".to_string(),
                    description: format!("Set {}=\"\" (empty), server should reject", param),
                    pre_condition: derive_pre_condition(endpoint),
                    expected_behavior: format!("Server should reject empty {}", param),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    strategy: TestStrategy::TypeConfusion,
                });
            }
        }

        if !existing
            .iter()
            .any(|t| matches!(t, ViolationType::Oversized))
        {
            scenarios.push(ViolationScenario {
                name: format!("{}_oversized", param),
                endpoint: endpoint.to_string(),
                param_name: param.to_string(),
                violation_type: ViolationType::Oversized,
                test_value: "\"A\" * 100000".to_string(),
                description: format!("Set {} to oversized value, server should reject", param),
                pre_condition: derive_pre_condition(endpoint),
                expected_behavior: format!("Server should reject oversized {}", param),
                defect_marker: "ILLEGAL_SUCCESS".to_string(),
                strategy: TestStrategy::BoundaryValue,
            });
        }

        if !existing
            .iter()
            .any(|t| matches!(t, ViolationType::TypeConfusion))
        {
            let is_int = self.store.type_constraints.iter().any(|atc| {
                atc.constraint.param_name == param
                    && atc.constraint.expected_type.to_lowercase().contains("int")
            });
            if is_int {
                scenarios.push(ViolationScenario {
                    name: format!("{}_type_confusion", param),
                    endpoint: endpoint.to_string(),
                    param_name: param.to_string(),
                    violation_type: ViolationType::TypeConfusion,
                    test_value: r#""not_a_number""#.to_string(),
                    description: format!(
                        "Set {}=\"not_a_number\" (string for int), server should reject",
                        param
                    ),
                    pre_condition: derive_pre_condition(endpoint),
                    expected_behavior: format!(
                        "Server should reject string value for int param {}",
                        param
                    ),
                    defect_marker: "ILLEGAL_SUCCESS".to_string(),
                    strategy: TestStrategy::TypeConfusion,
                });
            }
        }
    }

    fn build_system_prompt(&self, scenarios: &[ViolationScenario]) -> String {
        let target_name = &self.store.target;
        let style_hint = match self.style {
            TargetStyle::Qdrant => {
                "Qdrant (check status_code==200, no auth header, /collections/ paths)"
            }
            TargetStyle::Milvus => {
                "Milvus (check r.json().get('code')==0, Bearer root:Milvus auth, /v2/vectordb/ paths)"
            }
            TargetStyle::Weaviate => {
                "Weaviate (check status_code==200, no auth by default, /v1/schema and /v1/objects REST paths)"
            }
            TargetStyle::PgVector => {
                "PgVector (SQL via psycopg2, connect to postgresql://postgres:postgres@host:5432/testvdb, CREATE EXTENSION vector, use cursor.execute() not HTTP requests)"
            }
        };

        let strategy_counts = count_strategies(scenarios);
        let strategy_section = build_strategy_section(&strategy_counts, scenarios);
        let top_scenarios: Vec<_> = scenarios.iter().take(20).collect();
        let scenario_section = build_scenario_section(&top_scenarios, scenarios.len());

        format!(
            "You are a contract-driven bug hunter for {target}. \
Your mission: find REAL defects where the server violates its documented contracts.\n\
\n\
=== TARGET STYLE: {style} ===\n\
All test scripts must follow the {style} conventions.\n\
\n\
=== VIOLATION SCENARIOS ({total} total) ===\n\
{scenario_section}\
\n\
=== EXPLORATION STRATEGY ===\n\
{strategy_section}\
\n\
=== TEST TEMPLATE ===\n\
Every test script MUST follow this structure:\n\
1. SETUP: Create required resources (collection, index, etc.)\n\
2. PRE-VERIFY: Check state BEFORE the test action\n\
3. ACTION: Send the violating request\n\
4. POST-VERIFY: Check state AFTER the test action\n\
5. VERDICT: Print [DEFECT: TYPE] if violation found\n\
\n\
=== DEFECT MARKERS ===\n\
- [DEFECT: ILLEGAL_SUCCESS] — Server accepts input that should be rejected\n\
- [DEFECT: STATE_VIOLATION] — Server state is inconsistent after operation\n\
- [DEFECT: DATA_CORRUPTION] — Write then read returns different data\n\
- [DEFECT: POOR_DIAGNOSTICS] — Server silently discards data without error\n\
\n\
=== MANDATORY RULES ===\n\
1. DO NOT submit MRE before turn 5. You MUST explore at least 5 turns first.\n\
2. DO NOT repeat the same test pattern. Each turn MUST test a DIFFERENT violation scenario.\n\
3. If AUTO-GENERATED scripts are provided, you MUST execute at least ONE before writing your own.\n\
4. You MUST test at least 3 DIFFERENT violation scenarios before submitting.\n\
5. After finding a defect, test 2 MORE scenarios to see if the same class of defect exists elsewhere.\n\
\n\
=== SCRIPT RULES ===\n\
- Use {{{{TESTVDB_DB_URL}}}} as DB URL placeholder\n\
- time.sleep(0.5) after create, 0.3 after upsert\n\
- Print [DEFECT: TYPE] on defect\n\
- sys.exit(1) on defect, sys.exit(0) on pass\n\
- Unique collection name with uuid\n\
- Submit with submit_mre when >= 3 surviving assertions found",
            target = target_name,
            style = style_hint,
            total = scenarios.len(),
            scenario_section = scenario_section,
            strategy_section = strategy_section,
        )
    }

    fn build_initial_message(&self, scenarios: &[ViolationScenario]) -> String {
        let high_priority: Vec<_> = scenarios
            .iter()
            .filter(|s| {
                matches!(
                    s.strategy,
                    TestStrategy::BoundaryValue | TestStrategy::TypeConfusion
                )
            })
            .take(5)
            .collect();

        if high_priority.is_empty() {
            return "Begin exploration. Write a script and use execute_test_script(fresh_sandbox=true) to test it.".to_string();
        }

        let mut msg =
            "START by testing these HIGH-PRIORITY violation scenarios:\n\n".to_string();
        for (i, s) in high_priority.iter().enumerate() {
            msg.push_str(&format!(
                "{}. [{}] {} — {} (test: {}={})\n",
                i + 1,
                fmt_strategy(&s.strategy),
                s.name,
                s.description,
                s.param_name,
                s.test_value,
            ));
        }
        msg.push_str(
            "\nExecute a test for one of these scenarios first, then explore others.\n",
        );
        msg
    }
}

fn fmt_vtype(vt: &ViolationType) -> &'static str {
    match vt {
        ViolationType::NullInjection => "null",
        ViolationType::MissingRequired => "missing",
        ViolationType::TypeConfusion => "type_confusion",
        ViolationType::BelowMin => "below_min",
        ViolationType::AboveMax => "above_max",
        ViolationType::ZeroValue => "zero",
        ViolationType::NegativeValue => "negative",
        ViolationType::InvalidEnum => "invalid_enum",
        ViolationType::EmptyString => "empty",
        ViolationType::Oversized => "oversized",
    }
}

fn describe_violation(v: &ViolationTarget) -> String {
    match &v.violation_type {
        ViolationType::NullInjection => {
            format!("Set {}=null on {}, server should reject", v.param_name, v.endpoint)
        }
        ViolationType::MissingRequired => {
            format!("Omit required {} on {}, server should reject", v.param_name, v.endpoint)
        }
        ViolationType::TypeConfusion => {
            format!(
                "Set {}={} (wrong type) on {}, server should reject",
                v.param_name, v.test_value, v.endpoint
            )
        }
        ViolationType::BelowMin => {
            format!(
                "Set {}={} (below min) on {}, server should reject",
                v.param_name, v.test_value, v.endpoint
            )
        }
        ViolationType::AboveMax => {
            format!(
                "Set {}={} (above max) on {}, server should reject",
                v.param_name, v.test_value, v.endpoint
            )
        }
        ViolationType::ZeroValue => {
            format!("Set {}=0 on {}, server should reject", v.param_name, v.endpoint)
        }
        ViolationType::NegativeValue => {
            format!("Set {}=-1 on {}, server should reject", v.param_name, v.endpoint)
        }
        ViolationType::InvalidEnum => {
            format!(
                "Set {}={} (invalid enum) on {}, server should reject",
                v.param_name, v.test_value, v.endpoint
            )
        }
        ViolationType::EmptyString => {
            format!(
                "Set {}=\"\" (empty) on {}, server should reject",
                v.param_name, v.endpoint
            )
        }
        ViolationType::Oversized => {
            format!(
                "Set {} to oversized value on {}, server should reject",
                v.param_name, v.endpoint
            )
        }
    }
}

fn derive_pre_condition(endpoint: &str) -> String {
    let ep = endpoint.to_lowercase();
    if ep.contains("search") || ep.contains("query") {
        "Collection with data must exist".to_string()
    } else if ep.contains("create") {
        "Clean state (no existing collection)".to_string()
    } else if ep.contains("insert") || ep.contains("upsert") {
        "Collection must exist before inserting".to_string()
    } else if ep.contains("delete") || ep.contains("drop") {
        "Collection with data must exist before deleting".to_string()
    } else {
        "Appropriate resource must exist".to_string()
    }
}

fn classify_strategy(vt: &ViolationType) -> TestStrategy {
    match vt {
        ViolationType::NullInjection
        | ViolationType::TypeConfusion
        | ViolationType::EmptyString => TestStrategy::TypeConfusion,
        ViolationType::MissingRequired => TestStrategy::MissingRequired,
        ViolationType::BelowMin
        | ViolationType::AboveMax
        | ViolationType::ZeroValue
        | ViolationType::NegativeValue
        | ViolationType::Oversized => TestStrategy::BoundaryValue,
        ViolationType::InvalidEnum => TestStrategy::InvalidEnum,
    }
}

fn count_strategies(scenarios: &[ViolationScenario]) -> Vec<(TestStrategy, usize)> {
    let mut counts = HashMap::new();
    for s in scenarios {
        *counts.entry(s.strategy).or_insert(0) += 1;
    }
    let mut result: Vec<_> = counts.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1));
    result
}

fn build_strategy_section(
    strategy_counts: &[(TestStrategy, usize)],
    scenarios: &[ViolationScenario],
) -> String {
    let mut section = String::new();
    section.push_str(&format!("Total violation scenarios: {}\n\n", scenarios.len()));

    for (strategy, count) in strategy_counts {
        let hint = match strategy {
            TestStrategy::BoundaryValue => {
                "Test parameter boundaries: min-1, max+1, zero, negative"
            }
            TestStrategy::TypeConfusion => {
                "Test type violations: null, wrong type, empty string"
            }
            TestStrategy::MissingRequired => {
                "Test missing required parameters: omit the field"
            }
            TestStrategy::InvalidEnum => {
                "Test invalid enum values: values not in allowed set"
            }
            TestStrategy::StateConsistency => {
                "Test state transitions: verify pre/post conditions"
            }
            TestStrategy::Metamorphic => {
                "Test semantic invariants: write→read, distance ordering"
            }
        };
        section.push_str(&format!(
            "- {} ({} scenarios): {}\n",
            fmt_strategy(strategy),
            count,
            hint
        ));
    }

    section.push_str(
        "\nPriority: BoundaryValue > TypeConfusion > MissingRequired > InvalidEnum > StateConsistency > Metamorphic\n\
         Turn 1-3: BoundaryValue and TypeConfusion (highest defect yield)\n\
         Turn 4-6: MissingRequired and InvalidEnum\n\
         Turn 7+: StateConsistency and Metamorphic\n",
    );

    section
}

fn build_scenario_section(
    top_scenarios: &[&ViolationScenario],
    total: usize,
) -> String {
    let mut section = String::new();
    for (i, s) in top_scenarios.iter().enumerate() {
        section.push_str(&format!(
            "{}. [{}] {} — endpoint: {}, param: {}, test: {}\n   Pre: {}\n   Expected: {}\n\n",
            i + 1,
            fmt_strategy(&s.strategy),
            s.name,
            s.endpoint,
            s.param_name,
            s.test_value,
            s.pre_condition,
            s.expected_behavior,
        ));
    }
    if total > 20 {
        section.push_str(&format!(
            "... and {} more scenarios.\n",
            total - 20
        ));
    }
    section
}

fn fmt_strategy(s: &TestStrategy) -> &'static str {
    match s {
        TestStrategy::BoundaryValue => "BOUNDARY",
        TestStrategy::TypeConfusion => "TYPE_CONFUSION",
        TestStrategy::MissingRequired => "MISSING_REQUIRED",
        TestStrategy::InvalidEnum => "INVALID_ENUM",
        TestStrategy::StateConsistency => "STATE",
        TestStrategy::Metamorphic => "METAMORPHIC",
    }
}

pub fn count_unique_strategies(scenarios: &[ViolationScenario]) -> usize {
    let strategies: HashSet<_> = scenarios.iter().map(|s| std::mem::discriminant(&s.strategy)).collect();
    strategies.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{
        BehavioralContract, BehaviorCategory, RangeConstraint, StateConstraint,
        StructuredContract, TypeConstraint,
    };
    use crate::contract::store::{Confidence, ConstraintSource};

    fn make_test_store() -> ContractStore {
        let contract = StructuredContract {
            api_endpoint: "/v2/vectordb/entities/search".to_string(),
            doc_url: "https://milvus.io/api-reference".to_string(),
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
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit must be >= 1 and <= 16384".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            }],
            state_constraints: vec![StateConstraint {
                description: "collection must exist before search".to_string(),
                determinism: crate::contract::schema::Determinism::Deterministic,
                setup_script_template: None,
            }],
            state_invariants: vec![],
            behavioral_contracts: vec![BehavioralContract {
                name: "distance_ordering".to_string(),
                category: BehaviorCategory::SemanticCorrectness,
                endpoints: vec!["/v2/vectordb/entities/search".to_string()],
                precondition_script: String::new(),
                verification_script: String::new(),
                expected_outcome: "L2 distances should be in ascending order".to_string(),
                supersedes: None,
                mutation_rules: vec![],
            }],
        };

        let mut store = ContractStore::from_structured_contracts(
            "milvus",
            "v2.4",
            &[contract],
            ConstraintSource::ExplicitDoc,
            Confidence::High,
        );

        store.set_required_params(
            "/v2/vectordb/entities/search",
            vec!["collectionName".to_string()],
        );
        store.set_enum_values(
            "metricType",
            vec!["COSINE".to_string(), "L2".to_string(), "IP".to_string()],
        );

        store
    }

    #[test]
    fn test_enumerate_scenarios_min_3_per_contract() {
        let store = make_test_store();
        let pgen = PromptGenerator::new(store, TargetStyle::Milvus);
        let scenarios = pgen.enumerate_scenarios();

        let mut param_groups: HashMap<String, Vec<&ViolationScenario>> = HashMap::new();
        for s in &scenarios {
            param_groups
                .entry(s.param_name.clone())
                .or_default()
                .push(s);
        }

        for (param, group) in &param_groups {
            if *param != "state" && *param != "behavioral" {
                assert!(
                    group.len() >= 3,
                    "Param '{}' has only {} scenarios, need >= 3",
                    param,
                    group.len()
                );
            }
        }
    }

    #[test]
    fn test_system_prompt_contains_contract_info() {
        let store = make_test_store();
        let pgen = PromptGenerator::new(store, TargetStyle::Milvus);
        let prompt = pgen.generate();

        assert!(prompt.system_prompt.contains("milvus"));
        assert!(prompt.system_prompt.contains("contract-driven"));
        assert!(prompt.system_prompt.contains("VIOLATION SCENARIOS"));
        assert!(prompt.system_prompt.contains("EXPLORATION STRATEGY"));
        assert!(prompt.system_prompt.contains("TEST TEMPLATE"));
        assert!(prompt.system_prompt.contains("DEFECT MARKERS"));
        assert!(prompt.system_prompt.contains("[DEFECT: ILLEGAL_SUCCESS]"));
        assert!(prompt.system_prompt.contains("[DEFECT: STATE_VIOLATION]"));
        assert!(prompt.system_prompt.contains("[DEFECT: DATA_CORRUPTION]"));
    }

    #[test]
    fn test_system_prompt_contains_target_style() {
        let store = make_test_store();

        let pgen_milvus = PromptGenerator::new(store.clone(), TargetStyle::Milvus);
        let prompt_milvus = pgen_milvus.generate();
        assert!(prompt_milvus.system_prompt.contains("Bearer root:Milvus"));
        assert!(prompt_milvus.system_prompt.contains("r.json().get('code')"));

        let pgen_qdrant = PromptGenerator::new(store, TargetStyle::Qdrant);
        let prompt_qdrant = pgen_qdrant.generate();
        assert!(prompt_qdrant.system_prompt.contains("status_code"));
        assert!(!prompt_qdrant.system_prompt.contains("Bearer root:Milvus"));
    }

    #[test]
    fn test_initial_message_has_high_priority() {
        let store = make_test_store();
        let pgen = PromptGenerator::new(store, TargetStyle::Milvus);
        let prompt = pgen.generate();

        assert!(prompt.initial_message.contains("HIGH-PRIORITY"));
        assert!(!prompt.initial_message.is_empty());
    }

    #[test]
    fn test_strategy_classification() {
        assert_eq!(
            classify_strategy(&ViolationType::NullInjection),
            TestStrategy::TypeConfusion
        );
        assert_eq!(
            classify_strategy(&ViolationType::BelowMin),
            TestStrategy::BoundaryValue
        );
        assert_eq!(
            classify_strategy(&ViolationType::MissingRequired),
            TestStrategy::MissingRequired
        );
        assert_eq!(
            classify_strategy(&ViolationType::InvalidEnum),
            TestStrategy::InvalidEnum
        );
    }

    #[test]
    fn test_state_and_metamorphic_scenarios() {
        let store = make_test_store();
        let pgen = PromptGenerator::new(store, TargetStyle::Milvus);
        let scenarios = pgen.enumerate_scenarios();

        let state_scenarios: Vec<_> = scenarios
            .iter()
            .filter(|s| s.strategy == TestStrategy::StateConsistency)
            .collect();
        assert!(!state_scenarios.is_empty(), "Should have state scenarios");
        assert!(state_scenarios[0].defect_marker == "STATE_VIOLATION");

        let meta_scenarios: Vec<_> = scenarios
            .iter()
            .filter(|s| s.strategy == TestStrategy::Metamorphic)
            .collect();
        assert!(!meta_scenarios.is_empty(), "Should have metamorphic scenarios");
        assert!(meta_scenarios[0].defect_marker == "DATA_CORRUPTION");
    }

    #[test]
    fn test_pre_condition_derivation() {
        assert!(derive_pre_condition("/v2/vectordb/entities/search").contains("Collection with data"));
        assert!(derive_pre_condition("/v2/vectordb/collections/create").contains("Clean state"));
        assert!(derive_pre_condition("/v2/vectordb/entities/insert").contains("Collection must exist"));
        assert!(derive_pre_condition("/v2/vectordb/collections/drop").contains("Collection with data"));
    }

    #[test]
    fn test_empty_store() {
        let store = ContractStore::new("empty", "1.0");
        let pgen = PromptGenerator::new(store, TargetStyle::Qdrant);
        let prompt = pgen.generate();

        assert!(prompt.violation_scenarios.is_empty());
        assert!(prompt.system_prompt.contains("0 total"));
        assert!(prompt.initial_message.contains("Begin exploration"));
    }

    #[test]
    fn test_scenario_descriptions_not_empty() {
        let store = make_test_store();
        let pgen = PromptGenerator::new(store, TargetStyle::Milvus);
        let scenarios = pgen.enumerate_scenarios();

        for s in &scenarios {
            assert!(!s.description.is_empty(), "Scenario '{}' has empty description", s.name);
            assert!(!s.pre_condition.is_empty(), "Scenario '{}' has empty pre_condition", s.name);
            assert!(!s.expected_behavior.is_empty(), "Scenario '{}' has empty expected_behavior", s.name);
        }
    }
}