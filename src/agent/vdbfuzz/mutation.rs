use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationTestCase {
    pub name: String,
    pub mutation_type: MutationType,
    pub endpoint: String,
    pub param_name: String,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationType {
    TypeConfusion,
    NullInjection,
    MissingRequired,
    Oversized,
    UnknownParam,
    ExtraFields,
    AboveMax,
    BelowMin,
    InvalidEnum,
}

pub struct MutationTestGenerator;

impl MutationTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<MutationTestCase> {
        let mut cases = Vec::new();

        let mut endpoint_params: HashMap<String, Vec<ParamInfo>> = HashMap::new();
        for atc in &store.type_constraints {
            let ep = atc.endpoint.clone();
            let param = atc.constraint.param_name.clone();
            let expected_type = atc.constraint.expected_type.to_lowercase();
            let is_required = store.required_params.get(&ep)
                .map(|params| params.contains(&param))
                .unwrap_or(false);
            let enum_vals = store.enum_values.get(&param).cloned().unwrap_or_default();

            endpoint_params
                .entry(ep)
                .or_default()
                .push(ParamInfo {
                    name: param,
                    expected_type,
                    is_required,
                    enum_values: enum_vals,
                });
        }

        for (endpoint, params) in &endpoint_params {
            for param in params {
                cases.extend(Self::generate_type_confusion(endpoint, param, style));
                cases.push(Self::generate_null_injection(endpoint, param, style));
                if param.is_required {
                    cases.push(Self::generate_missing_required(endpoint, param, style));
                }
                cases.push(Self::generate_oversized(endpoint, param, style));
                cases.push(Self::generate_unknown_param(endpoint, param, style));
                cases.push(Self::generate_extra_fields(endpoint, param, style));
            }
        }

        for arc in &store.range_constraints {
            if let Some(max) = arc.constraint.max {
                cases.push(Self::generate_above_max(&arc.endpoint, &arc.constraint.param_name, max, style));
            }
            if let Some(min) = arc.constraint.min {
                cases.push(Self::generate_below_min(&arc.endpoint, &arc.constraint.param_name, min, style));
            }
        }

        for (param, values) in &store.enum_values {
            cases.push(Self::generate_invalid_enum(param, values, style));
        }

        cases
    }

    fn generate_type_confusion(endpoint: &str, param: &ParamInfo, style: TargetStyle) -> Vec<MutationTestCase> {
        let mut cases = Vec::new();
        let t = &param.expected_type;

        let (bad_value, desc) = if t.contains("int") {
            (r#""not_a_number""#, "string for int")
        } else if t.contains("float") || t.contains("double") {
            (r#""not_a_float""#, "string for float")
        } else if t.contains("bool") {
            (r#""not_a_bool""#, "string for bool")
        } else if t.contains("string") {
            ("12345", "int for string")
        } else {
            (r#""wrong_type""#, "wrong type")
        };

        cases.push(MutationTestCase {
            name: format!("{}_type_confusion", param.name),
            mutation_type: MutationType::TypeConfusion,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script: build_mutation_script(endpoint, &format!("body[\"{}\"] = {}", param.name, bad_value), &format!("{}={}", param.name, desc), true, style),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        });

        cases
    }

    fn generate_null_injection(endpoint: &str, param: &ParamInfo, style: TargetStyle) -> MutationTestCase {
        MutationTestCase {
            name: format!("{}_null", param.name),
            mutation_type: MutationType::NullInjection,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script: build_mutation_script(endpoint, &format!("body[\"{}\"] = None", param.name), &format!("{}=None", param.name), true, style),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_missing_required(endpoint: &str, param: &ParamInfo, style: TargetStyle) -> MutationTestCase {
        MutationTestCase {
            name: format!("{}_missing", param.name),
            mutation_type: MutationType::MissingRequired,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script: build_mutation_script(endpoint, &format!("body.pop(\"{}\", None)", param.name), &format!("missing {}", param.name), true, style),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_oversized(endpoint: &str, param: &ParamInfo, style: TargetStyle) -> MutationTestCase {
        let t = &param.expected_type;
        let (mutation, desc) = if t.contains("int") {
            (format!("body[\"{}\"] = 999999", param.name), format!("{}=999999", param.name))
        } else if t.contains("string") {
            (format!("body[\"{}\"] = 'A' * 100000", param.name), format!("{}=oversized", param.name))
        } else {
            (format!("body[\"{}\"] = 999999", param.name), format!("{}=oversized", param.name))
        };

        MutationTestCase {
            name: format!("{}_oversized", param.name),
            mutation_type: MutationType::Oversized,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script: build_mutation_script(endpoint, &mutation, &desc, true, style),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_unknown_param(endpoint: &str, _param: &ParamInfo, style: TargetStyle) -> MutationTestCase {
        MutationTestCase {
            name: format!("{}_unknown_param", endpoint.replace('/', "_")),
            mutation_type: MutationType::UnknownParam,
            endpoint: endpoint.to_string(),
            param_name: "unknownParam".to_string(),
            script: build_mutation_script(endpoint, r#"body["unknownParam"] = 123"#, "unknownParam=123", true, style),
            defect_marker: "PERMISSIVE_PARSING".to_string(),
        }
    }

    fn generate_extra_fields(endpoint: &str, _param: &ParamInfo, style: TargetStyle) -> MutationTestCase {
        MutationTestCase {
            name: format!("{}_extra_fields", endpoint.replace('/', "_")),
            mutation_type: MutationType::ExtraFields,
            endpoint: endpoint.to_string(),
            param_name: "extraField".to_string(),
            script: build_mutation_script(endpoint, r#"body["extraField"] = "unexpected""#, "extraField=unexpected", true, style),
            defect_marker: "PERMISSIVE_PARSING".to_string(),
        }
    }

    fn generate_above_max(endpoint: &str, param_name: &str, max: f64, style: TargetStyle) -> MutationTestCase {
        let above = max as i64 + 1;
        MutationTestCase {
            name: format!("{}_above_max", param_name),
            mutation_type: MutationType::AboveMax,
            endpoint: endpoint.to_string(),
            param_name: param_name.to_string(),
            script: build_mutation_script(endpoint, &format!("body[\"{}\"] = {}", param_name, above), &format!("{}={}", param_name, above), true, style),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_below_min(endpoint: &str, param_name: &str, min: f64, style: TargetStyle) -> MutationTestCase {
        let below = if min > 0.0 { 0 } else { min as i64 - 1 };
        MutationTestCase {
            name: format!("{}_below_min", param_name),
            mutation_type: MutationType::BelowMin,
            endpoint: endpoint.to_string(),
            param_name: param_name.to_string(),
            script: build_mutation_script(endpoint, &format!("body[\"{}\"] = {}", param_name, below), &format!("{}={}", param_name, below), true, style),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_invalid_enum(param_name: &str, valid_values: &[String], style: TargetStyle) -> MutationTestCase {
        let invalid = format!("INVALID_{}", valid_values.first().map(|v| v.to_uppercase()).unwrap_or("VAL".to_string()));
        MutationTestCase {
            name: format!("{}_invalid_enum", param_name),
            mutation_type: MutationType::InvalidEnum,
            endpoint: String::new(),
            param_name: param_name.to_string(),
            script: build_mutation_script("", &format!("body[\"{}\"] = \"{}\"", param_name, invalid), &format!("{}={}", param_name, invalid), false, style),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }
}

struct ParamInfo {
    name: String,
    expected_type: String,
    is_required: bool,
    enum_values: Vec<String>,
}

fn build_mutation_script(
    endpoint: &str,
    mutation_line: &str,
    label: &str,
    needs_setup: bool,
    style: TargetStyle,
) -> String {
    match style {
        TargetStyle::Milvus => build_milvus_mutation_script(endpoint, mutation_line, label, needs_setup),
        TargetStyle::Qdrant | TargetStyle::Weaviate => build_qdrant_mutation_script(endpoint, mutation_line, label, needs_setup),
        TargetStyle::PgVector => String::new(),
    }
}

fn build_milvus_mutation_script(endpoint: &str, mutation_line: &str, label: &str, needs_setup: bool) -> String {
    let setup_block = if needs_setup {
        format!(
            r#"{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
"#,
            create = crate::agent::probe_milvus::milvus_create_collection_default("c"),
        )
    } else {
        String::new()
    };

    let base_body = infer_base_body_milvus(endpoint);

    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'mut_' + uuid.uuid4().hex[:8]
{setup_block}body = {base_body}
{mutation_line}
r = requests.post(f'{{BASE}}{endpoint}', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        setup_block = setup_block,
        base_body = base_body,
        mutation_line = mutation_line,
        endpoint = endpoint,
        label = label,
    )
}

fn build_qdrant_mutation_script(endpoint: &str, mutation_line: &str, label: &str, needs_setup: bool) -> String {
    let setup_block = if needs_setup {
        r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
"#.to_string()
    } else {
        String::new()
    };

    let base_body = infer_base_body_qdrant(endpoint);
    let http_method = infer_http_method_qdrant(endpoint);

    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'mut_' + uuid.uuid4().hex[:8]
{setup_block}body = {base_body}
{mutation_line}
r = requests.{http_method}(f'{{BASE}}{endpoint}', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        setup_block = setup_block,
        base_body = base_body,
        mutation_line = mutation_line,
        endpoint = endpoint,
        label = label,
        http_method = http_method,
    )
}

fn infer_base_body_milvus(endpoint: &str) -> &'static str {
    if endpoint.contains("collections/create") {
        r#"{"collectionName":c,"dimension":4}"#
    } else if endpoint.contains("entities/insert") || endpoint.contains("entities/upsert") {
        r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#
    } else if endpoint.contains("entities/search") {
        r#"{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}"#
    } else if endpoint.contains("entities/query") {
        r#"{"collectionName":c,"filter":"id > 0","limit":3}"#
    } else if endpoint.contains("entities/delete") {
        r#"{"collectionName":c,"filter":"id > 0"}"#
    } else if endpoint.contains("indexes/create") {
        r#"{"collectionName":c,"indexType":"IVF_FLAT","fieldName":"vector"}"#
    } else if endpoint.contains("partitions/create") {
        r#"{"collectionName":c,"partitionName":"test_part"}"#
    } else if endpoint.contains("databases/create") {
        r#"{"dbName":"test_db"}"#
    } else {
        r#"{"collectionName":c}"#
    }
}

fn infer_http_method_qdrant(endpoint: &str) -> &'static str {
    if endpoint.contains("points/search") || endpoint.contains("points/delete") || endpoint.contains("points/scroll") || endpoint.contains("points/count") || endpoint.contains("points/recommend") || endpoint.contains("points/payload") || endpoint.contains("aliases") || endpoint.contains("points/clear") {
        "post"
    } else if endpoint.contains("collections") && !endpoint.contains("points") && !endpoint.contains("aliases") {
        if endpoint.ends_with("collections") || endpoint.matches('/').count() <= 2 {
            "put"
        } else {
            let after_collections: Vec<&str> = endpoint.split("collections/").last().unwrap_or("").splitn(2, '/').collect();
            if after_collections.len() > 1 {
                "get"
            } else {
                "put"
            }
        }
    } else if endpoint.contains("points") && !endpoint.contains("search") && !endpoint.contains("delete") && !endpoint.contains("scroll") && !endpoint.contains("count") && !endpoint.contains("recommend") && !endpoint.contains("payload") && !endpoint.contains("clear") {
        "put"
    } else {
        "post"
    }
}

fn infer_base_body_qdrant(endpoint: &str) -> &'static str {
    if endpoint.contains("collections") && !endpoint.contains("points") && !endpoint.contains("aliases") {
        r#"{"vectors":{"size":4,"distance":"Cosine"}}"#
    } else if endpoint.contains("points/search") {
        r#"{"vector":[0.1,0.2,0.3,0.4],"limit":3}"#
    } else if endpoint.contains("points/delete") {
        r#"{"points":[1]}"#
    } else if endpoint.contains("points") {
        r#"{"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#
    } else {
        r#"{}"#
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeMutationTarget {
    pub endpoint: String,
    pub param_name: String,
    pub param_type: String,
    pub injection_category: InjectionCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionCategory {
    SqlInjection,
    FloatBoundary,
    UnicodeAttack,
    FormatString,
    PathTraversal,
    CommandInjection,
}

#[derive(Debug, Clone)]
pub struct CreativeMutationPrompt {
    pub targets: Vec<CreativeMutationTarget>,
    pub prompt: String,
}

impl CreativeMutationPrompt {
    pub fn from_store(store: &ContractStore) -> Self {
        let mut targets = Vec::new();

        for atc in &store.type_constraints {
            let t = atc.constraint.expected_type.to_lowercase();
            if t.contains("string") {
                targets.push(CreativeMutationTarget {
                    endpoint: atc.endpoint.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::SqlInjection,
                });
                targets.push(CreativeMutationTarget {
                    endpoint: atc.endpoint.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::UnicodeAttack,
                });
                targets.push(CreativeMutationTarget {
                    endpoint: atc.endpoint.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::FormatString,
                });
            }
            if t.contains("float") || t.contains("double") || t.contains("number") {
                targets.push(CreativeMutationTarget {
                    endpoint: atc.endpoint.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::FloatBoundary,
                });
            }
            if t.contains("string") && atc.constraint.param_name.to_lowercase().contains("path") {
                targets.push(CreativeMutationTarget {
                    endpoint: atc.endpoint.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::PathTraversal,
                });
            }
            if t.contains("string") && atc.constraint.param_name.to_lowercase().contains("expr") {
                targets.push(CreativeMutationTarget {
                    endpoint: atc.endpoint.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::CommandInjection,
                });
            }
        }

        let prompt = Self::build_prompt(&targets);
        CreativeMutationPrompt { targets, prompt }
    }

    fn build_prompt(targets: &[CreativeMutationTarget]) -> String {
        let mut prompt = String::from(
            "=== CREATIVE MUTATION TARGETS ===\n\
             The following parameters need creative injection testing beyond basic type confusion.\n\
             For each target, generate a Python test script that sends the malicious payload.\n\n",
        );

        let mut grouped: HashMap<InjectionCategory, Vec<&CreativeMutationTarget>> = HashMap::new();
        for t in targets {
            grouped.entry(t.injection_category).or_default().push(t);
        }

        for (category, items) in &grouped {
            let category_name = match category {
                InjectionCategory::SqlInjection => "SQL Injection",
                InjectionCategory::FloatBoundary => "Float Boundary (NaN/Inf/subnormal)",
                InjectionCategory::UnicodeAttack => "Unicode Attack (homoglyphs/BIDI/null bytes)",
                InjectionCategory::FormatString => "Format String (%s/%n/%x)",
                InjectionCategory::PathTraversal => "Path Traversal (../)",
                InjectionCategory::CommandInjection => "Command Injection (; | & $())",
            };
            let payloads = match category {
                InjectionCategory::SqlInjection => vec![
                    r#"' OR '1'='1"#,
                    r#"'; DROP TABLE--"#,
                    r#"' UNION SELECT *--"#,
                    r#"1; SELECT * FROM"#,
                    r#"' OR 1=1 --"#,
                ],
                InjectionCategory::FloatBoundary => vec![
                    "float('nan')",
                    "float('inf')",
                    "float('-inf')",
                    "1e-323",
                    "1e309",
                    "0.1 + 0.2",
                ],
                InjectionCategory::UnicodeAttack => vec![
                    r#""\u0000""#,
                    r#""\uffff""#,
                    r#""café""#,
                    r#""\u202e""#,
                    r#""\u200b""#,
                ],
                InjectionCategory::FormatString => vec![
                    r#""%s%s%s%s""#,
                    r#""%n""#,
                    r#""%x%x%x""#,
                    r#""%d%d%d""#,
                ],
                InjectionCategory::PathTraversal => vec![
                    r#""../../../etc/passwd""#,
                    r#""..\\..\\..\\windows""#,
                    r#""....//....//etc""#,
                ],
                InjectionCategory::CommandInjection => vec![
                    r#""; ls -la""#,
                    r#""| cat /etc/passwd""#,
                    r#""$(whoami)""#,
                    r#""`id`""#,
                ],
            };

            prompt.push_str(&format!("--- {} ({} targets) ---\n", category_name, items.len()));
            for item in items {
                prompt.push_str(&format!(
                    "  endpoint: {} | param: {} (type: {})\n",
                    item.endpoint, item.param_name, item.param_type,
                ));
            }
            prompt.push_str("  Suggested payloads:\n");
            for p in &payloads {
                prompt.push_str(&format!("    - {}\n", p));
            }
            prompt.push('\n');
        }

        prompt.push_str(
            "RULES:\n\
             1. Each test script must use {{TESTVDB_DB_URL}} as DB URL placeholder\n\
             2. Print [DEFECT: ILLEGAL_SUCCESS] if the server accepts the malicious payload\n\
             3. sys.exit(1) on defect, sys.exit(0) on proper rejection\n\
             4. Use unique collection names with uuid\n\
             5. Create required resources before testing\n",
        );

        prompt
    }
}

pub fn count_creative_categories(targets: &[CreativeMutationTarget]) -> usize {
    let categories: HashSet<_> = targets.iter().map(|t| std::mem::discriminant(&t.injection_category)).collect();
    categories.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{RangeConstraint, TypeConstraint};
    use crate::contract::store::{AnnotatedRangeConstraint, AnnotatedTypeConstraint, Confidence, ConstraintSource};

    fn make_test_store() -> ContractStore {
        let mut store = ContractStore::new("milvus", "2.4");

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/entities/search".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "filter".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/entities/search".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "dimension".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/collections/create".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        store.range_constraints.push(AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/entities/search".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        store.set_required_params(
            "/v2/vectordb/entities/search",
            vec!["collectionName".to_string(), "limit".to_string()],
        );

        store
    }

    #[test]
    fn test_from_store_milvus_generates_all_types() {
        let store = make_test_store();
        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Milvus);

        let type_confusions: Vec<_> = cases.iter().filter(|c| c.mutation_type == MutationType::TypeConfusion).collect();
        assert!(!type_confusions.is_empty(), "Should have type_confusion cases");

        let null_injections: Vec<_> = cases.iter().filter(|c| c.mutation_type == MutationType::NullInjection).collect();
        assert!(!null_injections.is_empty(), "Should have null_injection cases");

        let missing: Vec<_> = cases.iter().filter(|c| c.mutation_type == MutationType::MissingRequired).collect();
        assert!(!missing.is_empty(), "Should have missing_required cases");

        let oversized: Vec<_> = cases.iter().filter(|c| c.mutation_type == MutationType::Oversized).collect();
        assert!(!oversized.is_empty(), "Should have oversized cases");

        let unknown: Vec<_> = cases.iter().filter(|c| c.mutation_type == MutationType::UnknownParam).collect();
        assert!(!unknown.is_empty(), "Should have unknown_param cases");

        let extra: Vec<_> = cases.iter().filter(|c| c.mutation_type == MutationType::ExtraFields).collect();
        assert!(!extra.is_empty(), "Should have extra_fields cases");
    }

    #[test]
    fn test_milvus_scripts_contain_auth() {
        let store = make_test_store();
        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Milvus);

        assert!(!cases.is_empty());
        for case in &cases {
            assert!(case.script.contains("Bearer root:Milvus"), "Milvus script missing auth: {}", case.name);
            assert!(case.script.contains("r.json().get('code')"), "Milvus script missing code check: {}", case.name);
        }
    }

    #[test]
    fn test_qdrant_scripts_no_auth() {
        let store = make_test_store();
        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Qdrant);

        assert!(!cases.is_empty());
        for case in &cases {
            assert!(!case.script.contains("Bearer root:Milvus"), "Qdrant script should not have auth: {}", case.name);
        }
    }

    #[test]
    fn test_type_confusion_int_param() {
        let store = make_test_store();
        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Milvus);

        let limit_tc: Vec<_> = cases.iter()
            .filter(|c| c.param_name == "limit" && c.mutation_type == MutationType::TypeConfusion)
            .collect();
        assert!(!limit_tc.is_empty());
        assert!(limit_tc[0].script.contains(r#"body["limit"] = "not_a_number""#));
    }

    #[test]
    fn test_missing_required_only_for_required() {
        let store = make_test_store();
        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Milvus);

        let missing_limit: Vec<_> = cases.iter()
            .filter(|c| c.param_name == "limit" && c.mutation_type == MutationType::MissingRequired)
            .collect();
        assert!(!missing_limit.is_empty(), "limit is required, should have missing case");

        let missing_filter: Vec<_> = cases.iter()
            .filter(|c| c.param_name == "filter" && c.mutation_type == MutationType::MissingRequired)
            .collect();
        assert!(missing_filter.is_empty(), "filter is not required, should not have missing case");
    }

    #[test]
    fn test_defect_markers() {
        let store = make_test_store();
        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Milvus);

        let illegal: Vec<_> = cases.iter().filter(|c| c.defect_marker == "ILLEGAL_SUCCESS").collect();
        let permissive: Vec<_> = cases.iter().filter(|c| c.defect_marker == "PERMISSIVE_PARSING").collect();
        assert!(!illegal.is_empty(), "Should have ILLEGAL_SUCCESS markers");
        assert!(!permissive.is_empty(), "Should have PERMISSIVE_PARSING markers");
    }

    #[test]
    fn test_creative_prompt_from_store() {
        let store = make_test_store();
        let creative = CreativeMutationPrompt::from_store(&store);

        assert!(!creative.targets.is_empty(), "Should have creative mutation targets");
        assert!(!creative.prompt.is_empty(), "Should have generated prompt");

        let sql_targets: Vec<_> = creative.targets.iter()
            .filter(|t| t.injection_category == InjectionCategory::SqlInjection)
            .collect();
        assert!(!sql_targets.is_empty(), "Should have SQL injection targets for string params");

        let float_targets: Vec<_> = creative.targets.iter()
            .filter(|t| t.injection_category == InjectionCategory::FloatBoundary)
            .collect();
        assert!(float_targets.is_empty(), "No float params in test store, should have no float targets");
    }

    #[test]
    fn test_creative_prompt_contains_payloads() {
        let store = make_test_store();
        let creative = CreativeMutationPrompt::from_store(&store);

        assert!(creative.prompt.contains("SQL Injection"), "Prompt should mention SQL Injection");
        assert!(creative.prompt.contains("Unicode Attack"), "Prompt should mention Unicode Attack");
        assert!(creative.prompt.contains("Format String"), "Prompt should mention Format String");
        assert!(creative.prompt.contains("ILLEGAL_SUCCESS"), "Prompt should mention DEFECT marker");
    }

    #[test]
    fn test_creative_prompt_with_float_param() {
        let mut store = ContractStore::new("milvus", "2.4");
        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "score".to_string(),
                expected_type: "float".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/entities/search".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        let creative = CreativeMutationPrompt::from_store(&store);
        let float_targets: Vec<_> = creative.targets.iter()
            .filter(|t| t.injection_category == InjectionCategory::FloatBoundary)
            .collect();
        assert!(!float_targets.is_empty(), "Should have float boundary targets for float params");
        assert!(creative.prompt.contains("Float Boundary"), "Prompt should mention Float Boundary");
    }
}