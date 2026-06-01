use crate::contract::schema::RejectionPolicy;
use crate::contract::store::ContractStore;
use crate::target::TargetStyle;

fn defect_marker_for(store: &ContractStore, param_name: &str, endpoint: &str) -> String {
    if store.get_rejection_policy(param_name, endpoint) == RejectionPolicy::Ignore {
        "PARAM_IGNORED".to_string()
    } else {
        "ILLEGAL_SUCCESS".to_string()
    }
}
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn is_header_param(param_name: &str) -> bool {
    matches!(
        param_name,
        "Request-Timeout" | "Authorization" | "Request-Header"
    )
}

fn param_container(param_name: &str, endpoint: &str, store: &ContractStore) -> String {
    let nested_parent = store.nested_params
        .get(endpoint)
        .and_then(|parents| {
            parents.iter().find_map(|(parent, children)| {
                if children.iter().any(|c| c == param_name) {
                    Some(parent.as_str())
                } else {
                    None
                }
            })
        });
    match nested_parent {
        Some(parent) => format!("body[\"{}\"]", parent),
        None => "body".to_string(),
    }
}

fn build_param_set(param_name: &str, endpoint: &str, store: &ContractStore, value: &str) -> String {
    if is_header_param(param_name) {
        return format!("HEADERS[\"{}\"] = {}", param_name, value);
    }
    if param_name.contains('.') {
        let parts: Vec<&str> = param_name.splitn(2, '.').collect();
        return format!("body.setdefault(\"{}\", {{}})[\"{}\"] = {}", parts[0], parts[1], value);
    }
    if param_name == "vector" && endpoint.contains("entities/search") {
        return format!("body[\"data\"] = {}", value);
    }
    let container = param_container(param_name, endpoint, store);
    format!("{}[\"{}\"] = {}", container, param_name, value)
}

fn build_param_pop(param_name: &str, endpoint: &str, store: &ContractStore) -> String {
    if is_header_param(param_name) {
        return format!("HEADERS.pop(\"{}\", None)", param_name);
    }
    if param_name.contains('.') {
        let parts: Vec<&str> = param_name.splitn(2, '.').collect();
        return format!("body.get(\"{}\", {{}}).pop(\"{}\", None)", parts[0], parts[1]);
    }
    if param_name == "vector" && endpoint.contains("entities/search") {
        return "body.pop(\"data\", None)".to_string();
    }
    let container = param_container(param_name, endpoint, store);
    format!("{}.pop(\"{}\", None)", container, param_name)
}

fn param_exists_in_base_body(param_name: &str, endpoint: &str, style: TargetStyle, store: &ContractStore) -> bool {
    if is_header_param(param_name) {
        return false;
    }
    let base_body = match style {
        TargetStyle::Milvus => infer_base_body_milvus(endpoint, store),
        TargetStyle::Qdrant => {
            let raw = infer_base_body_qdrant(endpoint);
            serde_json::from_str::<serde_json::Value>(raw).unwrap_or_default()
        }
        TargetStyle::Weaviate => infer_base_body_weaviate(endpoint),
        TargetStyle::PgVector => return false,
    };
    if param_name.contains('.') {
        let parts: Vec<&str> = param_name.splitn(2, '.').collect();
        base_body.get(parts[0])
            .and_then(|v| v.get(parts[1]))
            .is_some()
    } else if param_name == "vector" && endpoint.contains("entities/search") {
        base_body.get("data").is_some()
    } else {
        base_body.get(param_name).is_some()
    }
}

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

        let known_api_paths: std::collections::HashSet<String> = store.endpoints
            .iter()
            .map(|e| e.api_path.trim_start_matches('/').to_string())
            .collect();
        let has_known_endpoints = !known_api_paths.is_empty();

        let mut endpoint_params: HashMap<String, Vec<ParamInfo>> = HashMap::new();
        for atc in &store.type_constraints {
            let ep = match &atc.endpoint {
                Some(e) => e.clone(),
                None => continue,
            };
            let param = atc.constraint.param_name.clone();
            if !store.is_param_for_endpoint(&param, &ep) {
                continue;
            }
            if has_known_endpoints && !known_api_paths.contains(ep.trim_start_matches('/')) {
                continue;
            }
            let expected_type = atc.constraint.expected_type.to_lowercase();
            let is_required = store.required_params.get(&ep)
                .map(|params| params.contains(&param))
                .unwrap_or(false);
            let enum_vals = store.enum_values.get(&param).cloned().unwrap_or_default();
            let nested_parent = store.nested_params
                .get(&ep)
                .and_then(|parents| {
                    parents.iter().find_map(|(parent, children)| {
                        if children.iter().any(|c| c == &param) {
                            Some(parent.clone())
                        } else {
                            None
                        }
                    })
                });

            endpoint_params
                .entry(ep)
                .or_default()
                .push(ParamInfo {
                    name: param,
                    expected_type,
                    is_required,
                    enum_values: enum_vals,
                    nested_parent,
                });
        }

        for (endpoint, params) in &endpoint_params {
            for param in params {
                cases.extend(Self::generate_type_confusion(endpoint, param, style, store));
                cases.push(Self::generate_null_injection(endpoint, param, style, store));
                if param.is_required && param_exists_in_base_body(&param.name, endpoint, style, store) {
                    cases.push(Self::generate_missing_required(endpoint, param, style, store));
                }
                cases.push(Self::generate_oversized(endpoint, param, style, store));
                cases.push(Self::generate_unknown_param(endpoint, param, style, store));
                cases.push(Self::generate_extra_fields(endpoint, param, style, store));
            }
        }

        for arc in &store.range_constraints {
            let ep = match &arc.endpoint {
                Some(e) => e,
                None => continue,
            };
            if !store.is_param_for_endpoint(&arc.constraint.param_name, ep) {
                continue;
            }
            if has_known_endpoints && !known_api_paths.contains(ep.trim_start_matches('/')) {
                continue;
            }
            if let Some(max) = arc.constraint.max {
                cases.push(Self::generate_above_max(ep, &arc.constraint.param_name, max, style, store));
            }
            if let Some(min) = arc.constraint.min {
                cases.push(Self::generate_below_min(ep, &arc.constraint.param_name, min, style, store));
            }
        }

        for (param, values) in &store.enum_values {
            let endpoint = store.type_constraints.iter()
                .find(|atc| atc.constraint.param_name == *param)
                .and_then(|atc| atc.endpoint.as_deref())
                .unwrap_or("");
            if !endpoint.is_empty() && !store.is_param_for_endpoint(param, endpoint) {
                continue;
            }
            if !endpoint.is_empty() && has_known_endpoints && !known_api_paths.contains(endpoint.trim_start_matches('/')) {
                continue;
            }
            cases.push(Self::generate_invalid_enum(param, values, style, store, endpoint));
        }

        cases
    }

    fn generate_type_confusion(endpoint: &str, param: &ParamInfo, style: TargetStyle, store: &ContractStore) -> Vec<MutationTestCase> {
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

        let defect_marker = defect_marker_for(store, &param.name, endpoint);
        let mutation_line = build_param_set(&param.name, endpoint, store, bad_value);
        let script = build_mutation_script(endpoint, &mutation_line, &format!("{}={}", param.name, desc), true, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        cases.push(MutationTestCase {
            name: format!("{}_type_confusion", param.name),
            mutation_type: MutationType::TypeConfusion,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script,
            defect_marker,
        });

        cases
    }

    fn generate_null_injection(endpoint: &str, param: &ParamInfo, style: TargetStyle, store: &ContractStore) -> MutationTestCase {
        let defect_marker = if !param.is_required {
            "PARAM_IGNORED".to_string()
        } else {
            defect_marker_for(store, &param.name, endpoint)
        };
        let mutation_line = build_param_set(&param.name, endpoint, store, "None");
        let script = build_mutation_script(endpoint, &mutation_line, &format!("{}=None", param.name), true, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        MutationTestCase {
            name: format!("{}_null", param.name),
            mutation_type: MutationType::NullInjection,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script,
            defect_marker,
        }
    }

    fn generate_missing_required(endpoint: &str, param: &ParamInfo, style: TargetStyle, store: &ContractStore) -> MutationTestCase {
        let mutation_line = build_param_pop(&param.name, endpoint, store);
        MutationTestCase {
            name: format!("{}_missing", param.name),
            mutation_type: MutationType::MissingRequired,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script: build_mutation_script(endpoint, &mutation_line, &format!("missing {}", param.name), true, style, store),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_oversized(endpoint: &str, param: &ParamInfo, style: TargetStyle, store: &ContractStore) -> MutationTestCase {
        let t = &param.expected_type;
        let (mutation, desc) = if t.contains("int") {
            (build_param_set(&param.name, endpoint, store, "999999"), format!("{}=999999", param.name))
        } else if t.contains("string") {
            (build_param_set(&param.name, endpoint, store, "'A' * 100000"), format!("{}=oversized", param.name))
        } else {
            (build_param_set(&param.name, endpoint, store, "999999"), format!("{}=oversized", param.name))
        };

        let defect_marker = defect_marker_for(store, &param.name, endpoint);
        let script = build_mutation_script(endpoint, &mutation, &desc, true, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        MutationTestCase {
            name: format!("{}_oversized", param.name),
            mutation_type: MutationType::Oversized,
            endpoint: endpoint.to_string(),
            param_name: param.name.clone(),
            script,
            defect_marker,
        }
    }

    fn generate_unknown_param(endpoint: &str, _param: &ParamInfo, style: TargetStyle, store: &ContractStore) -> MutationTestCase {
        let defect_marker = "PERMISSIVE_PARSING".to_string();
        let script = build_mutation_script(endpoint, r#"body["unknownParam"] = 123"#, "unknownParam=123", true, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        MutationTestCase {
            name: format!("{}_unknown_param", endpoint.replace('/', "_")),
            mutation_type: MutationType::UnknownParam,
            endpoint: endpoint.to_string(),
            param_name: "unknownParam".to_string(),
            script,
            defect_marker,
        }
    }

    fn generate_extra_fields(endpoint: &str, _param: &ParamInfo, style: TargetStyle, store: &ContractStore) -> MutationTestCase {
        let defect_marker = "PERMISSIVE_PARSING".to_string();
        let script = build_mutation_script(endpoint, r#"body["extraField"] = "unexpected""#, "extraField=unexpected", true, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        MutationTestCase {
            name: format!("{}_extra_fields", endpoint.replace('/', "_")),
            mutation_type: MutationType::ExtraFields,
            endpoint: endpoint.to_string(),
            param_name: "extraField".to_string(),
            script,
            defect_marker,
        }
    }

    fn generate_above_max(endpoint: &str, param_name: &str, max: f64, style: TargetStyle, store: &ContractStore) -> MutationTestCase {
        let above = max as i64 + 1;
        let defect_marker = defect_marker_for(store, param_name, endpoint);
        let mutation_line = build_param_set(param_name, endpoint, store, &above.to_string());
        let script = build_mutation_script(endpoint, &mutation_line, &format!("{}={}", param_name, above), true, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        MutationTestCase {
            name: format!("{}_above_max", param_name),
            mutation_type: MutationType::AboveMax,
            endpoint: endpoint.to_string(),
            param_name: param_name.to_string(),
            script,
            defect_marker,
        }
    }

    fn generate_below_min(endpoint: &str, param_name: &str, min: f64, style: TargetStyle, store: &ContractStore) -> MutationTestCase {
        let below = if min > 0.0 { 0 } else { min as i64 - 1 };
        let defect_marker = defect_marker_for(store, param_name, endpoint);
        let mutation_line = build_param_set(param_name, endpoint, store, &below.to_string());
        let script = build_mutation_script(endpoint, &mutation_line, &format!("{}={}", param_name, below), true, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        MutationTestCase {
            name: format!("{}_below_min", param_name),
            mutation_type: MutationType::BelowMin,
            endpoint: endpoint.to_string(),
            param_name: param_name.to_string(),
            script,
            defect_marker,
        }
    }

    fn generate_invalid_enum(param_name: &str, valid_values: &[String], style: TargetStyle, store: &ContractStore, endpoint: &str) -> MutationTestCase {
        let invalid = format!("INVALID_{}", valid_values.first().map(|v| v.to_uppercase()).unwrap_or("VAL".to_string()));
        let defect_marker = defect_marker_for(store, param_name, endpoint);
        let mutation_line = build_param_set(param_name, endpoint, store, &format!("\"{}\"", invalid));
        let script = build_mutation_script(endpoint, &mutation_line, &format!("{}={}", param_name, invalid), false, style, store);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));
        MutationTestCase {
            name: format!("{}_invalid_enum", param_name),
            mutation_type: MutationType::InvalidEnum,
            endpoint: endpoint.to_string(),
            param_name: param_name.to_string(),
            script,
            defect_marker,
        }
    }
}

struct ParamInfo {
    name: String,
    expected_type: String,
    is_required: bool,
    enum_values: Vec<String>,
    nested_parent: Option<String>,
}

fn build_mutation_script(
    endpoint: &str,
    mutation_line: &str,
    label: &str,
    needs_setup: bool,
    style: TargetStyle,
    store: &ContractStore,
) -> String {
    match style {
        TargetStyle::Milvus => build_milvus_mutation_script(endpoint, mutation_line, label, needs_setup, store),
        TargetStyle::Qdrant => build_qdrant_mutation_script(endpoint, mutation_line, label, needs_setup),
        TargetStyle::Weaviate => build_weaviate_mutation_script(endpoint, mutation_line, label, needs_setup),
        TargetStyle::PgVector => String::new(),
    }
}

fn build_milvus_mutation_script(endpoint: &str, mutation_line: &str, label: &str, needs_setup: bool, store: &ContractStore) -> String {
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

    let base_body = infer_base_body_milvus(endpoint, store);
    let body_json = serde_json::to_string(&base_body).expect("serializing serde_json::Value is infallible");
    let body_python = body_json.replace("\"__C__\"", "c");

    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
c = 'mut_' + uuid.uuid4().hex[:8]
{setup_block}body = {body_python}
{mutation_line}
r = requests.post(f'{{BASE}}{endpoint}', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        setup_block = setup_block,
        body_python = body_python,
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

fn infer_base_body_milvus(endpoint: &str, store: &ContractStore) -> serde_json::Value {
    let mut body = if endpoint.contains("collections/create") {
        serde_json::json!({"collectionName": "__C__", "dimension": 4})
    } else if endpoint.contains("entities/insert") || endpoint.contains("entities/upsert") {
        serde_json::json!({"collectionName": "__C__", "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]})
    } else if endpoint.contains("entities/search") {
        serde_json::json!({"collectionName": "__C__", "data": [[0.1, 0.2, 0.3, 0.4]], "limit": 3})
    } else if endpoint.contains("entities/query") {
        serde_json::json!({"collectionName": "__C__", "filter": "id > 0", "limit": 3})
    } else if endpoint.contains("entities/delete") {
        serde_json::json!({"collectionName": "__C__", "filter": "id > 0"})
    } else if endpoint.contains("indexes/create") {
        serde_json::json!({"collectionName": "__C__", "indexType": "IVF_FLAT", "fieldName": "vector"})
    } else if endpoint.contains("partitions/create") {
        serde_json::json!({"collectionName": "__C__", "partitionName": "test_part"})
    } else if endpoint.contains("databases/create") {
        serde_json::json!({"dbName": "test_db"})
    } else {
        serde_json::json!({"collectionName": "__C__"})
    };

    if endpoint.contains("entities/search") {
        let search_params = store.get_nested_params(endpoint, "searchParams");
        if !search_params.is_empty() {
            let mut sp = serde_json::Map::new();
            for param in &search_params {
                match param.as_str() {
                    "nprobe" => { sp.insert("nprobe".to_string(), serde_json::json!(10)); }
                    "ef" => { sp.insert("ef".to_string(), serde_json::json!(64)); }
                    _ => { sp.insert(param.clone(), serde_json::json!(1)); }
                }
            }
            body.as_object_mut().expect("infer_base_body_milvus returns JSON object").insert("searchParams".to_string(), serde_json::Value::Object(sp));
        }
    }

    body
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

fn infer_base_body_weaviate(endpoint: &str) -> serde_json::Value {
    if endpoint.contains("/v1/schema") && !endpoint.contains("/{") && !endpoint.contains("/v1/schema/") {
        serde_json::json!({"class":"__C__","vectorIndexConfig":{"distance":"cosine","efConstruction":128,"maxConnections":64},"properties":[{"name":"title","dataType":["string"]}]})
    } else if endpoint.contains("/v1/batch") {
        serde_json::json!({"objects":[{"class":"__C__","properties":{"title":"test"},"vector":[0.1,0.2,0.3,0.4]}]})
    } else if endpoint.contains("/v1/objects") && !endpoint.contains("/v1/objects/") {
        serde_json::json!({"class":"__C__","properties":{"title":"test"},"vector":[0.1,0.2,0.3,0.4]})
    } else {
        serde_json::json!({})
    }
}

fn infer_http_method_weaviate(endpoint: &str) -> &'static str {
    if endpoint.contains("/v1/schema/") || endpoint.contains("/v1/objects/") {
        "delete"
    } else {
        "post"
    }
}

fn build_weaviate_mutation_script(endpoint: &str, mutation_line: &str, label: &str, needs_setup: bool) -> String {
    let setup_block = if needs_setup {
        r#"r = requests.post(f'{BASE}/v1/schema', json={"class":c,"vectorIndexConfig":{"distance":"cosine","efConstruction":128,"maxConnections":64},"properties":[{"name":"title","dataType":["string"]}]})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(1)
"#.to_string()
    } else {
        String::new()
    };

    let base_body = infer_base_body_weaviate(endpoint);
    let body_json = serde_json::to_string(&base_body).expect("serializing serde_json::Value is infallible");
    let body_python = body_json.replace("\"__C__\"", "c");
    let http_method = infer_http_method_weaviate(endpoint);

    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'Mut_' + uuid.uuid4().hex[:8]
{setup_block}body = {body_python}
{mutation_line}
r = requests.{http_method}(f'{{BASE}}{endpoint}', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        setup_block = setup_block,
        body_python = body_python,
        mutation_line = mutation_line,
        http_method = http_method,
        endpoint = endpoint,
        label = label,
    )
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
            let ep = match &atc.endpoint {
                Some(e) => e.clone(),
                None => continue,
            };
            let t = atc.constraint.expected_type.to_lowercase();
            if t.contains("string") {
                targets.push(CreativeMutationTarget {
                    endpoint: ep.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::SqlInjection,
                });
                targets.push(CreativeMutationTarget {
                    endpoint: ep.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::UnicodeAttack,
                });
                targets.push(CreativeMutationTarget {
                    endpoint: ep.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::FormatString,
                });
            }
            if t.contains("float") || t.contains("double") || t.contains("number") {
                targets.push(CreativeMutationTarget {
                    endpoint: ep.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::FloatBoundary,
                });
            }
            if t.contains("string") && atc.constraint.param_name.to_lowercase().contains("path") {
                targets.push(CreativeMutationTarget {
                    endpoint: ep.clone(),
                    param_name: atc.constraint.param_name.clone(),
                    param_type: atc.constraint.expected_type.clone(),
                    injection_category: InjectionCategory::PathTraversal,
                });
            }
            if t.contains("string") && atc.constraint.param_name.to_lowercase().contains("expr") {
                targets.push(CreativeMutationTarget {
                    endpoint: ep.clone(),
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
    use crate::contract::schema::{RangeConstraint, RejectionPolicy, TypeConstraint};
    use crate::contract::store::{AnnotatedRangeConstraint, AnnotatedTypeConstraint, Confidence, ConstraintSource};

    fn make_test_store() -> ContractStore {
        let mut store = ContractStore::new("milvus", "2.4");

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "filter".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "dimension".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/collections/create".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.range_constraints.push(AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
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
            assert!(case.script.contains("{{TESTVDB_AUTH_HEADER}}"), "Milvus script missing auth: {}", case.name);
            assert!(case.script.contains("r.json().get('code')"), "Milvus script missing code check: {}", case.name);
        }
    }

    #[test]
    fn test_qdrant_scripts_no_auth() {
        let store = make_test_store();
        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Qdrant);

        assert!(!cases.is_empty());
        for case in &cases {
            assert!(!case.script.contains("{{TESTVDB_AUTH_HEADER}}"), "Qdrant script should not have auth: {}", case.name);
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
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        let creative = CreativeMutationPrompt::from_store(&store);
        let float_targets: Vec<_> = creative.targets.iter()
            .filter(|t| t.injection_category == InjectionCategory::FloatBoundary)
            .collect();
        assert!(!float_targets.is_empty(), "Should have float boundary targets for float params");
        assert!(creative.prompt.contains("Float Boundary"), "Prompt should mention Float Boundary");
    }

    #[test]
    fn test_milvus_mutation_marker_distribution() {
        use std::collections::HashMap;

        let mut store = ContractStore::new("milvus", "2.4");

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.range_constraints.push(AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "nprobe".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });

        store.range_constraints.push(AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "nprobe".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(65536.0),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });

        store.set_required_params(
            "/v2/vectordb/entities/search",
            vec!["collectionName".to_string(), "limit".to_string()],
        );

        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Milvus);

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for case in &cases {
            *counts.entry(&case.defect_marker).or_insert(0) += 1;
        }
        eprintln!("Mutation marker distribution: {:?}", counts);

        assert!(*counts.get("PERMISSIVE_PARSING").unwrap_or(&0) > 0, "PERMISSIVE_PARSING should exist");
        assert!(*counts.get("PARAM_IGNORED").unwrap_or(&0) > 0, "PARAM_IGNORED should exist");
    }

    #[test]
    fn test_null_injection_optional_param_is_param_ignored() {
        use crate::contract::store::{ContractStore, ConstraintSource, Confidence};

        let mut store = ContractStore::new("milvus", "2.4");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "optionalParam".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "requiredParam".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.set_required_params("search", vec!["requiredParam".to_string()]);

        let cases = MutationTestGenerator::from_store(&store, TargetStyle::Milvus);

        let optional_null: Vec<_> = cases.iter().filter(|c| c.name == "optionalParam_null").collect();
        let required_null: Vec<_> = cases.iter().filter(|c| c.name == "requiredParam_null").collect();

        assert_eq!(optional_null.len(), 1, "should have one optionalParam_null case");
        assert_eq!(optional_null[0].defect_marker, "PARAM_IGNORED",
            "optional param null injection should be PARAM_IGNORED");

        assert_eq!(required_null.len(), 1, "should have one requiredParam_null case");
        assert_eq!(required_null[0].defect_marker, "ILLEGAL_SUCCESS",
            "required param null injection should be ILLEGAL_SUCCESS");
    }
}