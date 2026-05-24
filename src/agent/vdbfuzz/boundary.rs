use crate::agent::probe::{generate_probe, EndpointType, ProbeTemplate};
use crate::contract::schema::{RangeConstraint, StructuredContract, TypeConstraint};
use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzTestCase {
    pub name: String,
    pub script: String,
    pub expected_rejection: bool,
    pub defect_marker: String,
    pub coverage_entry: Option<(String, String, String)>,
    #[serde(default)]
    pub semantic_assertion: Option<String>,
}

pub struct BoundaryValueGenerator;

impl BoundaryValueGenerator {
    pub fn from_contract(contract: &StructuredContract, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();
        let endpoint = &contract.api_endpoint;

        for rc in &contract.range_constraints {
            cases.extend(Self::from_range_constraint(rc, endpoint, style));
        }

        for tc in &contract.type_constraints {
            cases.extend(Self::from_type_constraint(tc, endpoint, style));
        }

        cases
    }

    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();

        for arc in &store.range_constraints {
            cases.extend(Self::from_range_constraint(
                &arc.constraint,
                &arc.endpoint,
                style,
            ));
        }

        for atc in &store.type_constraints {
            cases.extend(Self::from_type_constraint(
                &atc.constraint,
                &atc.endpoint,
                style,
            ));
        }

        for (endpoint, params) in &store.required_params {
            for param in params {
                cases.push(Self::make_missing_required_case(param, endpoint, style));
            }
        }

        for (param_name, values) in &store.enum_values {
            if values.is_empty() {
                continue;
            }
            let endpoint = store
                .type_constraints
                .iter()
                .find(|atc| atc.constraint.param_name == *param_name)
                .map(|atc| atc.endpoint.as_str())
                .unwrap_or("unknown");
            cases.push(Self::make_invalid_enum_case(param_name, endpoint, style));
        }

        cases
    }

    fn from_range_constraint(rc: &RangeConstraint, endpoint: &str, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();
        let param = &rc.param_name;
        let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);

        if let Some(min) = rc.min {
            if min > 0.0 {
                cases.push(Self::make_case(
                    &format!("{}_below_min", param),
                    ep_type,
                    param,
                    &format_boundary(min - 1.0),
                    &format!("{} below min ({})", param, format_boundary(min - 1.0)),
                    true,
                    endpoint,
                    style,
                ));
            }
            if min == 1.0 {
                cases.push(Self::make_case(
                    &format!("{}_zero", param),
                    ep_type,
                    param,
                    "0",
                    &format!("{}=0", param),
                    true,
                    endpoint,
                    style,
                ));
            }
        }

        if let Some(max) = rc.max {
            cases.push(Self::make_case(
                &format!("{}_above_max", param),
                ep_type,
                param,
                &format_boundary(max + 1.0),
                &format!("{} above max ({})", param, format_boundary(max + 1.0)),
                true,
                endpoint,
                style,
            ));
        }

        cases.push(Self::make_case(
            &format!("{}_negative", param),
            ep_type,
            param,
            "-1",
            &format!("{}=-1", param),
            true,
            endpoint,
            style,
        ));

        cases
    }

    fn from_type_constraint(tc: &TypeConstraint, endpoint: &str, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();
        let param = &tc.param_name;
        let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);

        match tc.expected_type.to_lowercase().as_str() {
            "integer" | "int" | "u64" | "i64" | "usize" => {
                cases.push(Self::make_case(
                    &format!("{}_float_type", param),
                    ep_type,
                    param,
                    "3.5",
                    &format!("{}=3.5 (float for int param)", param),
                    true,
                    endpoint,
                    style,
                ));
                cases.push(Self::make_case(
                    &format!("{}_string_type", param),
                    ep_type,
                    param,
                    "\"abc\"",
                    &format!("{}=\"abc\" (string for int param)", param),
                    true,
                    endpoint,
                    style,
                ));
            }
            "float" | "f64" | "double" => {
                cases.push(Self::make_string_case(
                    &format!("{}_nan", param),
                    param,
                    "float('nan')",
                    &format!("{}=NaN", param),
                    endpoint,
                    style,
                ));
                cases.push(Self::make_string_case(
                    &format!("{}_inf", param),
                    param,
                    "float('inf')",
                    &format!("{}=Inf", param),
                    endpoint,
                    style,
                ));
            }
            "string" => {
                cases.push(Self::make_case(
                    &format!("{}_empty_string", param),
                    ep_type,
                    param,
                    "\"\"",
                    &format!("{}=\"\" (empty string)", param),
                    true,
                    endpoint,
                    style,
                ));
            }
            _ => {}
        }

        cases
    }

    fn make_case(
        name: &str,
        ep_type: EndpointType,
        param: &str,
        value: &str,
        label: &str,
        expected_rejection: bool,
        endpoint: &str,
        style: TargetStyle,
    ) -> FuzzTestCase {
        let script = match style {
            TargetStyle::Qdrant => {
                let is_search = crate::agent::probe::is_search_param(param);
                if is_search {
                    crate::agent::probe::search_params_probe(param, value, label)
                } else {
                    generate_probe(ep_type, param, value, label)
                }
            }
            TargetStyle::Milvus => {
                let is_search = is_milvus_search_param(param);
                if is_search {
                    crate::agent::probe_milvus::milvus_search_params_probe(param, value, label)
                } else {
                    milvus_generate_probe(ep_type, param, value, label)
                }
            }
            TargetStyle::Weaviate => {
                // Deterministic generators use Qdrant API paths; Weaviate probes are in SafetyNets + semantic.rs
                // Return empty script for now — SafetyNets handle Weaviate
                String::new()
            }
            TargetStyle::PgVector => String::new(),
        };

        FuzzTestCase {
            name: name.to_string(),
            script,
            expected_rejection,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
            coverage_entry: Some((endpoint.to_string(), param.to_string(), value.to_string())),
            semantic_assertion: None,
        }
    }

    fn make_string_case(
        name: &str,
        param: &str,
        value_expr: &str,
        label: &str,
        endpoint: &str,
        style: TargetStyle,
    ) -> FuzzTestCase {
        let script = match style {
            TargetStyle::Qdrant => qdrant_float_probe(param, value_expr, label),
            TargetStyle::Milvus => crate::agent::probe_milvus::milvus_search_probe(param, value_expr, label),
            TargetStyle::Weaviate => String::new(), // SafetyNets handle Weaviate
            TargetStyle::PgVector => String::new(),
        };

        FuzzTestCase {
            name: name.to_string(),
            script,
            expected_rejection: true,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
            coverage_entry: Some((endpoint.to_string(), param.to_string(), value_expr.to_string())),
            semantic_assertion: None,
        }
    }

    fn make_missing_required_case(param: &str, endpoint: &str, style: TargetStyle) -> FuzzTestCase {
        let script = match style {
            TargetStyle::Qdrant => {
                qdrant_missing_required_probe(param, endpoint)
            }
            TargetStyle::Milvus => {
                milvus_missing_required_probe(param, endpoint)
            }
            TargetStyle::Weaviate => String::new(), // SafetyNets handle Weaviate
            TargetStyle::PgVector => String::new(),
        };

        FuzzTestCase {
            name: format!("{}_missing_required", param),
            script,
            expected_rejection: true,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
            coverage_entry: Some((endpoint.to_string(), param.to_string(), "<remove>".to_string())),
            semantic_assertion: None,
        }
    }

    fn make_invalid_enum_case(param: &str, endpoint: &str, style: TargetStyle) -> FuzzTestCase {
        let script = match style {
            TargetStyle::Qdrant => {
                qdrant_invalid_enum_probe(param, endpoint)
            }
            TargetStyle::Milvus => {
                milvus_invalid_enum_probe(param, endpoint)
            }
            TargetStyle::Weaviate => String::new(), // SafetyNets handle Weaviate
            TargetStyle::PgVector => String::new(),
        };

        FuzzTestCase {
            name: format!("{}_invalid_enum", param),
            script,
            expected_rejection: true,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
            coverage_entry: Some((endpoint.to_string(), param.to_string(), "INVALID_ENUM_VALUE_42".to_string())),
            semantic_assertion: None,
        }
    }
}

fn is_milvus_search_param(param_name: &str) -> bool {
    let p = param_name.to_lowercase();
    matches!(p.as_str(), "nprobe" | "ef" | "radius" | "range_filter" | "level")
}

fn milvus_generate_probe(ep_type: EndpointType, param: &str, value: &str, label: &str) -> String {
    let template = crate::agent::probe::MilvusProbeTemplate;
    match ep_type {
        EndpointType::Search => template.search_probe(param, value, label),
        EndpointType::Create => template.create_probe(param, value, label),
        EndpointType::Upsert => template.upsert_probe(param, value, label),
        EndpointType::Delete | EndpointType::Scroll => {
            template.delete_probe(param, value, label)
        }
        EndpointType::Recommend => template.recommend_probe(param, value, label),
        EndpointType::Config => template.search_probe(param, value, label),
    }
}

fn qdrant_float_probe(param: &str, value_expr: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'fuzz_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{"points":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
body = {{"vector":[0.1,0.2,0.3,0.4],"limit":3}}
body["{param}"] = {value_expr}
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json=body)
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value_expr = value_expr,
        label = label,
    )
}

fn qdrant_missing_required_probe(param: &str, endpoint: &str) -> String {
    let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);
    match ep_type {
        EndpointType::Create => format!(
            r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'fuzz_' + uuid.uuid4().hex[:8]
body = {{"vectors":{{"size":4,"distance":"Cosine"}}}}
body.pop("{param}", None)
r = requests.put(f'{{BASE}}/collections/{{c}}', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] missing required {param} accepted'); sys.exit(1)
else: print(f'properly rejected missing {param}: {{r.status_code}}'); sys.exit(0)"#,
            param = param,
        ),
        _ => format!(
            r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'fuzz_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.5)
body = {{"vector":[0.1,0.2,0.3,0.4],"limit":3}}
body.pop("{param}", None)
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] missing required {param} accepted'); sys.exit(1)
else: print(f'properly rejected missing {param}: {{r.status_code}}'); sys.exit(0)"#,
            param = param,
        ),
    }
}

fn milvus_missing_required_probe(param: &str, _endpoint: &str) -> String {
    let create = crate::agent::probe_milvus::milvus_create_collection_default("c");
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'fuzz_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}}
body.pop("{param}", None)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] missing required {param} accepted'); sys.exit(1)
else: print(f'properly rejected missing {param}: {{r.json()}}'); sys.exit(0)"#,
        create = create,
        param = param,
    )
}

fn qdrant_invalid_enum_probe(param: &str, endpoint: &str) -> String {
    let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);
    match ep_type {
        EndpointType::Create => format!(
            r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'fuzz_' + uuid.uuid4().hex[:8]
body = {{"vectors":{{"size":4,"distance":"Cosine"}},"{param}":"INVALID_ENUM_VALUE_42"}}
r = requests.put(f'{{BASE}}/collections/{{c}}', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] invalid enum {param} accepted'); sys.exit(1)
else: print(f'properly rejected invalid enum {param}: {{r.status_code}}'); sys.exit(0)"#,
            param = param,
        ),
        _ => format!(
            r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'fuzz_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.5)
body = {{"vector":[0.1,0.2,0.3,0.4],"limit":3}}
body["{param}"] = "INVALID_ENUM_VALUE_42"
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] invalid enum {param} accepted'); sys.exit(1)
else: print(f'properly rejected invalid enum {param}: {{r.status_code}}'); sys.exit(0)"#,
            param = param,
        ),
    }
}

fn milvus_invalid_enum_probe(param: &str, _endpoint: &str) -> String {
    let create = crate::agent::probe_milvus::milvus_create_collection_default("c");
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'fuzz_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}}
body["{param}"] = "INVALID_ENUM_VALUE_42"
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] invalid enum {param} accepted'); sys.exit(1)
else: print(f'properly rejected invalid enum {param}: {{r.json()}}'); sys.exit(0)"#,
        create = create,
        param = param,
    )
}

fn format_boundary(v: f64) -> String {
    crate::agent::probe::format_boundary(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{RangeConstraint, TypeConstraint};

    #[test]
    fn test_from_range_constraint_qdrant() {
        let rc = RangeConstraint {
            param_name: "limit".to_string(),
            description: "limit must be >= 1 and <= 1000".to_string(),
            min: Some(1.0),
            max: Some(1000.0),
            violation_examples: vec![],
        };
        let cases = BoundaryValueGenerator::from_range_constraint(
            &rc,
            "/collections/{name}/points/search",
            TargetStyle::Qdrant,
        );
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|c| c.name.contains("zero")));
        assert!(cases.iter().any(|c| c.name.contains("negative")));
        assert!(cases.iter().any(|c| c.name.contains("above_max")));
    }

    #[test]
    fn test_from_range_constraint_milvus() {
        let rc = RangeConstraint {
            param_name: "limit".to_string(),
            description: "limit must be >= 1 and <= 1000".to_string(),
            min: Some(1.0),
            max: Some(1000.0),
            violation_examples: vec![],
        };
        let cases = BoundaryValueGenerator::from_range_constraint(
            &rc,
            "search",
            TargetStyle::Milvus,
        );
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|c| c.name.contains("zero")));
        assert!(cases.iter().any(|c| c.script.contains("Bearer root:Milvus")));
        assert!(cases.iter().any(|c| c.script.contains("r.json().get('code')")));
    }

    #[test]
    fn test_from_type_constraint_integer() {
        let tc = TypeConstraint {
            param_name: "limit".to_string(),
            expected_type: "integer".to_string(),
            violation_examples: vec![],
        };
        let cases = BoundaryValueGenerator::from_type_constraint(
            &tc,
            "/collections/{name}/points/search",
            TargetStyle::Qdrant,
        );
        assert!(cases.iter().any(|c| c.name.contains("float_type")));
        assert!(cases.iter().any(|c| c.name.contains("string_type")));
    }

    #[test]
    fn test_from_contract_qdrant() {
        let contract = StructuredContract {
            api_endpoint: "search_points".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };
        let cases = BoundaryValueGenerator::from_contract(&contract, TargetStyle::Qdrant);
        assert!(cases.len() >= 3);
        assert!(cases.iter().all(|c| !c.script.contains("Bearer root:Milvus")));
    }

    #[test]
    fn test_from_contract_milvus() {
        let contract = StructuredContract {
            api_endpoint: "search".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };
        let cases = BoundaryValueGenerator::from_contract(&contract, TargetStyle::Milvus);
        assert!(cases.len() >= 3);
        assert!(cases.iter().all(|c| c.script.contains("Bearer root:Milvus")));
        assert!(cases.iter().all(|c| c.script.contains("r.json().get('code')")));
    }

    #[test]
    fn test_milvus_search_param_dispatch() {
        let nprobe_cases = BoundaryValueGenerator::from_range_constraint(
            &RangeConstraint {
                param_name: "nprobe".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec![],
            },
            "search",
            TargetStyle::Milvus,
        );
        assert!(nprobe_cases.iter().any(|c| c.script.contains("searchParams")));
    }

    #[test]
    fn test_from_store_milvus() {
        use crate::contract::store::{ContractStore, ConstraintSource, Confidence};

        let mut store = ContractStore::new("milvus", "2.4");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: "search".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: "search".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });
        store.set_required_params("search", vec!["collectionName".to_string()]);
        store.set_enum_values("metricType", vec!["COSINE".to_string(), "L2".to_string(), "IP".to_string()]);

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.len() >= 5);
        assert!(cases.iter().all(|c| c.script.contains("Bearer root:Milvus")));
        assert!(cases.iter().any(|c| c.name.contains("missing_required")));
        assert!(cases.iter().any(|c| c.name.contains("invalid_enum")));
    }

    #[test]
    fn test_from_store_qdrant() {
        use crate::contract::store::{ContractStore, ConstraintSource, Confidence};

        let mut store = ContractStore::new("qdrant", "1.8");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: "search_points".to_string(),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::High,
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec![],
            },
            endpoint: "search_points".to_string(),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::High,
        });

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Qdrant);
        assert!(cases.len() >= 3);
        assert!(cases.iter().all(|c| !c.script.contains("Bearer root:Milvus")));
    }
}