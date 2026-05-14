use crate::agent::probe::{generate_probe, EndpointType};
use crate::contract::schema::{RangeConstraint, StructuredContract, TypeConstraint};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzTestCase {
    pub name: String,
    pub script: String,
    pub expected_rejection: bool,
    pub defect_marker: String,
    pub coverage_entry: Option<(String, String, String)>,
}

pub struct BoundaryValueGenerator;

impl BoundaryValueGenerator {
    pub fn from_contract(contract: &StructuredContract) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();
        let endpoint = &contract.api_endpoint;

        for rc in &contract.range_constraints {
            cases.extend(Self::from_range_constraint(rc, endpoint));
        }

        for tc in &contract.type_constraints {
            cases.extend(Self::from_type_constraint(tc, endpoint));
        }

        cases
    }

    fn from_range_constraint(rc: &RangeConstraint, endpoint: &str) -> Vec<FuzzTestCase> {
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
        ));

        cases
    }

    fn from_type_constraint(tc: &TypeConstraint, endpoint: &str) -> Vec<FuzzTestCase> {
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
                ));
                cases.push(Self::make_case(
                    &format!("{}_string_type", param),
                    ep_type,
                    param,
                    "\"abc\"",
                    &format!("{}=\"abc\" (string for int param)", param),
                    true,
                    endpoint,
                ));
            }
            "float" | "f64" | "double" => {
                cases.push(Self::make_string_case(
                    &format!("{}_nan", param),
                    param,
                    "float('nan')",
                    &format!("{}=NaN", param),
                    endpoint,
                ));
                cases.push(Self::make_string_case(
                    &format!("{}_inf", param),
                    param,
                    "float('inf')",
                    &format!("{}=Inf", param),
                    endpoint,
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
    ) -> FuzzTestCase {
        let is_search = crate::agent::probe::is_search_param(param);
        let script = if is_search {
            crate::agent::probe::search_params_probe(param, value, label)
        } else {
            generate_probe(ep_type, param, value, label)
        };

        FuzzTestCase {
            name: name.to_string(),
            script,
            expected_rejection,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
            coverage_entry: Some((endpoint.to_string(), param.to_string(), value.to_string())),
        }
    }

    fn make_string_case(
        name: &str,
        param: &str,
        value_expr: &str,
        label: &str,
        endpoint: &str,
    ) -> FuzzTestCase {
        let script = format!(
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
        );

        FuzzTestCase {
            name: name.to_string(),
            script,
            expected_rejection: true,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
            coverage_entry: Some((endpoint.to_string(), param.to_string(), value_expr.to_string())),
        }
    }
}

fn format_boundary(v: f64) -> String {
    crate::agent::probe::format_boundary(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{RangeConstraint, TypeConstraint};

    #[test]
    fn test_from_range_constraint() {
        let rc = RangeConstraint {
            param_name: "limit".to_string(),
            description: "limit must be >= 1 and <= 1000".to_string(),
            min: Some(1.0),
            max: Some(1000.0),
            violation_examples: vec![],
        };
        let cases = BoundaryValueGenerator::from_range_constraint(&rc, "/collections/{name}/points/search");
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|c| c.name.contains("zero")));
        assert!(cases.iter().any(|c| c.name.contains("negative")));
        assert!(cases.iter().any(|c| c.name.contains("above_max")));
    }

    #[test]
    fn test_from_type_constraint_integer() {
        let tc = TypeConstraint {
            param_name: "limit".to_string(),
            expected_type: "integer".to_string(),
            violation_examples: vec![],
        };
        let cases = BoundaryValueGenerator::from_type_constraint(&tc, "/collections/{name}/points/search");
        assert!(cases.iter().any(|c| c.name.contains("float_type")));
        assert!(cases.iter().any(|c| c.name.contains("string_type")));
    }

    #[test]
    fn test_from_contract() {
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
        let cases = BoundaryValueGenerator::from_contract(&contract);
        assert!(cases.len() >= 3);
    }
}
