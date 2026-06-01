use crate::agent::probe::{generate_probe, EndpointType, ProbeTemplate};
use crate::contract::schema::{RangeConstraint, RejectionPolicy, StructuredContract, TypeConstraint};
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
    #[serde(default)]
    pub rejection_policy: Option<RejectionPolicy>,
}

pub struct BoundaryValueGenerator;

impl BoundaryValueGenerator {
    #[deprecated(note = "use from_store instead")]
    pub fn from_contract(contract: &StructuredContract, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();
        let endpoint = &contract.api_endpoint;

        for rc in &contract.range_constraints {
            cases.extend(Self::from_range_constraint(rc, endpoint, style, None, None));
        }

        for tc in &contract.type_constraints {
            cases.extend(Self::from_type_constraint(tc, endpoint, style, None, None));
        }

        cases
    }

    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();

        for arc in &store.range_constraints {
            if !store.is_param_for_endpoint(&arc.constraint.param_name, arc.endpoint.as_deref().unwrap_or("")) {
                continue;
            }
            if is_header_param(&arc.constraint.param_name) {
                continue;
            }
            let new_cases = Self::from_range_constraint(
                &arc.constraint,
                arc.endpoint.as_deref().unwrap_or(""),
                style,
                Some(store),
                arc.rejection_policy.clone(),
            );
            cases.extend(new_cases);
        }

        for atc in &store.type_constraints {
            if !store.is_param_for_endpoint(&atc.constraint.param_name, atc.endpoint.as_deref().unwrap_or("")) {
                continue;
            }
            if is_header_param(&atc.constraint.param_name) {
                continue;
            }
            let new_cases = Self::from_type_constraint(
                &atc.constraint,
                atc.endpoint.as_deref().unwrap_or(""),
                style,
                Some(store),
                atc.rejection_policy.clone(),
            );
            cases.extend(new_cases);
        }

        for (endpoint, params) in &store.required_params {
            for param in params {
                if !store.is_param_for_endpoint(param, endpoint) {
                    continue;
                }
                if is_header_param(param) {
                    continue;
                }
                if is_param_with_default(param) {
                    continue;
                }
                if store.get_rejection_policy(param, endpoint) == RejectionPolicy::Ignore {
                    continue;
                }
                cases.push(Self::make_missing_required_case(param, endpoint, style, Some(RejectionPolicy::Reject)));
            }
        }

        for (param_name, values) in &store.enum_values {
            if values.is_empty() {
                continue;
            }
            if is_header_param(param_name) {
                continue;
            }
            let endpoint = store
                .type_constraints
                .iter()
                .find(|atc| atc.constraint.param_name == *param_name)
                .and_then(|atc| atc.endpoint.as_deref())
                .unwrap_or("unknown");
            if !store.is_param_for_endpoint(param_name, endpoint) {
                continue;
            }
            let policy = store
                .type_constraints
                .iter()
                .find(|atc| atc.constraint.param_name == *param_name)
                .and_then(|atc| atc.rejection_policy.clone())
                .unwrap_or(RejectionPolicy::Reject);
            cases.push(Self::make_invalid_enum_case(param_name, endpoint, style, Some(policy)));
        }

        cases
    }

    fn from_range_constraint(rc: &RangeConstraint, endpoint: &str, style: TargetStyle, store: Option<&ContractStore>, rejection_policy: Option<RejectionPolicy>) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();
        let param = &rc.param_name;
        let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);
        let rejection_policy = if is_silently_ignored_param(param) {
            Some(RejectionPolicy::Ignore)
        } else {
            rejection_policy
        };

        if let Some(min) = rc.min {
            if min > 0.0 {
                let below_min_policy = if is_zero_valid_param(param) && (min - 1.0).abs() < 1.0 {
                    Some(RejectionPolicy::Ignore)
                } else {
                    rejection_policy.clone()
                };
                cases.push(Self::make_case(
                    &format!("{}_below_min", param),
                    ep_type,
                    param,
                    &format_boundary(min - 1.0),
                    &format!("{} below min ({})", param, format_boundary(min - 1.0)),
                    true,
                    endpoint,
                    style,
                    store,
                    below_min_policy,
                ));
            } else if min <= 0.0 && min.is_finite() {
                cases.push(Self::make_case(
                    &format!("{}_below_min", param),
                    ep_type,
                    param,
                    &format_boundary(min - 1.0),
                    &format!("{} below min ({})", param, format_boundary(min - 1.0)),
                    true,
                    endpoint,
                    style,
                    store,
                    rejection_policy.clone(),
                ));
            }
            if min == 1.0 {
                let zero_policy = if is_zero_valid_param(param) {
                    Some(RejectionPolicy::Ignore)
                } else {
                    rejection_policy.clone()
                };
                cases.push(Self::make_case(
                    &format!("{}_zero", param),
                    ep_type,
                    param,
                    "0",
                    &format!("{}=0", param),
                    true,
                    endpoint,
                    style,
                    store,
                    zero_policy,
                ));
            }
        }

        if let Some(max) = rc.max {
            let above_max_val = max + 1.0;
            let above_max_policy = if param.eq_ignore_ascii_case("dim") && (above_max_val - 32768.0).abs() < 1.0 {
                Some(RejectionPolicy::Ignore)
            } else {
                rejection_policy.clone()
            };
            cases.push(Self::make_case(
                &format!("{}_above_max", param),
                ep_type,
                param,
                &format_boundary(above_max_val),
                &format!("{} above max ({})", param, format_boundary(above_max_val)),
                true,
                endpoint,
                style,
                store,
                above_max_policy,
            ));
        }

        let negative_val = if rc.min.map_or(false, |m| m <= -1.0) { "-2" } else { "-1" };
        let below_min_equals_negative = rc.min.map_or(false, |m| {
            let below_min = m - 1.0;
            let neg: f64 = if m <= -1.0 { -2.0 } else { -1.0 };
            (below_min - neg).abs() < f64::EPSILON
        });
        if !below_min_equals_negative {
            cases.push(Self::make_case(
                &format!("{}_negative", param),
                ep_type,
                param,
                negative_val,
                &format!("{}={}", param, negative_val),
                true,
                endpoint,
                style,
                store,
                rejection_policy,
            ));
        }

        cases
    }

    fn from_type_constraint(tc: &TypeConstraint, endpoint: &str, style: TargetStyle, store: Option<&ContractStore>, rejection_policy: Option<RejectionPolicy>) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();
        let param = &tc.param_name;
        let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);
        let rejection_policy = if is_silently_ignored_param(param) {
            Some(RejectionPolicy::Ignore)
        } else {
            rejection_policy
        };

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
                    store,
                    rejection_policy.clone(),
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
                    store,
                    rejection_policy.clone(),
                ));
            }
            "float" | "f64" | "double" => {
                cases.push(Self::make_string_case(
                    &format!("{}_nan", param),
                    ep_type,
                    param,
                    "float('nan')",
                    &format!("{}=NaN", param),
                    endpoint,
                    style,
                    store,
                    rejection_policy.clone(),
                ));
                cases.push(Self::make_string_case(
                    &format!("{}_inf", param),
                    ep_type,
                    param,
                    "float('inf')",
                    &format!("{}=Inf", param),
                    endpoint,
                    style,
                    store,
                    rejection_policy,
                ));
            }
            "string" => {
                let empty_string_policy = if is_optional_empty_string_param(param) {
                    Some(RejectionPolicy::Ignore)
                } else {
                    rejection_policy.clone()
                };
                cases.push(Self::make_case(
                    &format!("{}_empty_string", param),
                    ep_type,
                    param,
                    "\"\"",
                    &format!("{}=\"\" (empty string)", param),
                    true,
                    endpoint,
                    style,
                    store,
                    empty_string_policy,
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
        store: Option<&ContractStore>,
        rejection_policy: Option<RejectionPolicy>,
    ) -> FuzzTestCase {
        let defect_marker = match &rejection_policy {
            Some(RejectionPolicy::Ignore) => "PARAM_IGNORED".to_string(),
            _ => "ILLEGAL_SUCCESS".to_string(),
        };

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
                let is_nested_search = store
                    .map(|s| s.get_nested_params(endpoint, "searchParams").iter().any(|p| p == param))
                    .unwrap_or(false);
                if is_nested_search {
                    crate::agent::probe_milvus::milvus_search_params_probe(param, value, label)
                } else {
                    milvus_generate_probe(ep_type, param, value, label, endpoint)
                }
            }
            TargetStyle::Weaviate => weaviate_generate_probe(ep_type, param, value, label),
            TargetStyle::PgVector => String::new(),
        };

        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));

        FuzzTestCase {
            name: name.to_string(),
            script,
            expected_rejection,
            defect_marker,
            coverage_entry: Some((endpoint.to_string(), param.to_string(), value.to_string())),
            semantic_assertion: None,
            rejection_policy,
        }
    }

    fn make_string_case(
        name: &str,
        ep_type: EndpointType,
        param: &str,
        value_expr: &str,
        label: &str,
        endpoint: &str,
        style: TargetStyle,
        store: Option<&ContractStore>,
        rejection_policy: Option<RejectionPolicy>,
    ) -> FuzzTestCase {
        let defect_marker = match &rejection_policy {
            Some(RejectionPolicy::Ignore) => "PARAM_IGNORED".to_string(),
            _ => "ILLEGAL_SUCCESS".to_string(),
        };

        let script = match style {
            TargetStyle::Qdrant => qdrant_float_probe(param, value_expr, label),
            TargetStyle::Milvus => {
                let is_nested_search = store
                    .map(|s| s.get_nested_params(endpoint, "searchParams").iter().any(|p| p == param))
                    .unwrap_or(false);
                if is_nested_search {
                    crate::agent::probe_milvus::milvus_search_params_probe(param, value_expr, label)
                } else {
                    milvus_generate_probe(ep_type, param, value_expr, label, endpoint)
                }
            }
            TargetStyle::Weaviate => weaviate_generate_probe(ep_type, param, value_expr, label),
            TargetStyle::PgVector => String::new(),
        };

        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));

        FuzzTestCase {
            name: name.to_string(),
            script,
            expected_rejection: true,
            defect_marker,
            coverage_entry: Some((endpoint.to_string(), param.to_string(), value_expr.to_string())),
            semantic_assertion: None,
            rejection_policy,
        }
    }

    fn make_missing_required_case(param: &str, endpoint: &str, style: TargetStyle, rejection_policy: Option<RejectionPolicy>) -> FuzzTestCase {
        let script = match style {
            TargetStyle::Qdrant => {
                qdrant_missing_required_probe(param, endpoint)
            }
            TargetStyle::Milvus => {
                milvus_missing_required_probe(param, endpoint)
            }
            TargetStyle::Weaviate => weaviate_missing_required_probe(param, endpoint),
            TargetStyle::PgVector => String::new(),
        };

        let defect_marker = if script.is_empty() {
            "PARAM_IGNORED".to_string()
        } else {
            "ILLEGAL_SUCCESS".to_string()
        };

        FuzzTestCase {
            name: format!("{}_missing_required", param),
            script,
            expected_rejection: true,
            defect_marker,
            coverage_entry: Some((endpoint.to_string(), param.to_string(), "<remove>".to_string())),
            semantic_assertion: None,
            rejection_policy,
        }
    }

    fn make_invalid_enum_case(param: &str, endpoint: &str, style: TargetStyle, rejection_policy: Option<RejectionPolicy>) -> FuzzTestCase {
        let defect_marker = match &rejection_policy {
            Some(RejectionPolicy::Ignore) => "PARAM_IGNORED".to_string(),
            _ => "ILLEGAL_SUCCESS".to_string(),
        };

        let script = match style {
            TargetStyle::Qdrant => {
                qdrant_invalid_enum_probe(param, endpoint)
            }
            TargetStyle::Milvus => {
                milvus_invalid_enum_probe(param, endpoint)
            }
            TargetStyle::Weaviate => weaviate_invalid_enum_probe(param, endpoint),
            TargetStyle::PgVector => String::new(),
        };

        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", defect_marker));

        FuzzTestCase {
            name: format!("{}_invalid_enum", param),
            script,
            expected_rejection: true,
            defect_marker,
            coverage_entry: Some((endpoint.to_string(), param.to_string(), "INVALID_ENUM_VALUE_42".to_string())),
            semantic_assertion: None,
            rejection_policy,
        }
    }
}

fn milvus_generate_probe(ep_type: EndpointType, param: &str, value: &str, label: &str, endpoint: &str) -> String {
    let header_params = ["Request-Timeout", "Request-Header", "Authorization"];
    if header_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return String::new();
    }
    if !endpoint.contains('+') {
        let ep_lower = endpoint.to_lowercase();
        let is_role_param = ["roleName", "objectType", "objectName", "privilege"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_user_param = ["userName", "password", "newPassword"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_db_param = ["dbName", "newDbName"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_alias_param = ["aliasName"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_index_param = ["indexName", "annsField"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_partition_param = ["partitionName", "partitionNames"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_rename_param = ["newCollectionName"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_get_param = ["id"].iter().any(|p| p.eq_ignore_ascii_case(param));
        let is_search_param = ["searchParams", "groupingField", "rerank"].iter().any(|p| p.eq_ignore_ascii_case(param));
        if is_role_param && !ep_lower.contains("role") { return String::new(); }
        if is_user_param && !ep_lower.contains("user") { return String::new(); }
        if is_db_param && !ep_lower.contains("database") && !ep_lower.contains("db") { return String::new(); }
        if is_alias_param && !ep_lower.contains("alias") { return String::new(); }
        if is_index_param && !ep_lower.contains("index") { return String::new(); }
        if is_partition_param && !ep_lower.contains("partition") { return String::new(); }
        if is_rename_param && !ep_lower.contains("rename") { return String::new(); }
        if is_get_param && !ep_lower.contains("entities/get") { return String::new(); }
        if is_search_param && !ep_lower.contains("search") { return String::new(); }
    }
    let template = crate::agent::probe::MilvusProbeTemplate;
    let resolved_ep_type = if endpoint.contains('+') {
        milvus_classify_param_endpoint(param, endpoint)
    } else {
        ep_type
    };
    match resolved_ep_type {
        EndpointType::Search => template.search_probe(param, value, label),
        EndpointType::Create => template.create_probe(param, value, label),
        EndpointType::Upsert => template.upsert_probe(param, value, label),
        EndpointType::Delete | EndpointType::Scroll => {
            template.delete_probe(param, value, label)
        }
        EndpointType::Recommend => template.recommend_probe(param, value, label),
        EndpointType::Config => {
            let ep_lower = endpoint.to_lowercase();
            if ep_lower.contains("index") {
                crate::agent::probe_milvus::milvus_index_probe(param, value, label)
            } else if ep_lower.contains("partition") {
                crate::agent::probe_milvus::milvus_partition_probe(param, value, label)
            } else if ep_lower.contains("alias") {
                crate::agent::probe_milvus::milvus_alias_probe(param, value, label)
            } else if ep_lower.contains("database") {
                crate::agent::probe_milvus::milvus_database_probe(param, value, label)
            } else if ep_lower.contains("user") {
                String::new()
            } else if ep_lower.contains("role") {
                String::new()
            } else {
                String::new()
            }
        }
    }
}

fn milvus_classify_param_endpoint(param: &str, _composite_endpoint: &str) -> EndpointType {
    let create_params = [
        "collectionName", "dim", "dimension", "efconstruction", "for", "metricType", "indexType",
        "autoID", "enableDynamicField", "idType", "shardsNum", "partitionsNum",
        "ttlSeconds", "max_length", "consistencyLevel", "collection.ttl.seconds",
        "schema", "schema.autoId", "schema.enableDynamicField", "schema.fields",
        "indexParams", "fields", "vectorFieldName", "primaryFieldName",
    ];
    if create_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Create;
    }
    let index_params = ["indexName", "annsField"];
    if index_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Config;
    }
    let partition_params = ["partitionName"];
    if partition_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Config;
    }
    let alias_params = ["aliasName", "newCollectionName"];
    if alias_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Config;
    }
    let db_params = ["dbName", "newDbName"];
    if db_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Config;
    }
    let user_params = ["userName", "password", "newPassword"];
    if user_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Config;
    }
    let role_params = ["roleName", "objectName", "objectType", "privilege"];
    if role_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Config;
    }
    let header_params = ["Request-Timeout", "Request-Header", "Authorization"];
    if header_params.iter().any(|p| p.eq_ignore_ascii_case(param)) {
        return EndpointType::Search;
    }
    EndpointType::Search
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

fn milvus_missing_required_probe(param: &str, endpoint: &str) -> String {
    let label = format!("missing required {}", param);
    if endpoint.contains('+') {
        let ep_type = milvus_classify_param_endpoint(param, endpoint);
        match ep_type {
            EndpointType::Create => {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/collections/create",
                    r#"{"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                    param, &label, false,
                )
            }
            EndpointType::Config => {
                let ep_lower = endpoint.to_lowercase();
                if ep_lower.contains("index") {
                    crate::agent::probe_milvus::generate_mutation_probe_with_marker_no_index(
                        "/v2/vectordb/indexes/create",
                        r#"{"collectionName":c,"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                        &format!(r#"body.pop("{}", None)"#, param),
                        &label, "ILLEGAL_SUCCESS",
                    )
                } else if ep_lower.contains("partition") {
                    crate::agent::probe_milvus::generate_missing_field_probe(
                        "/v2/vectordb/partitions/create",
                        r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                        param, &label, true,
                    )
                } else if ep_lower.contains("alias") {
                    crate::agent::probe_milvus::generate_missing_field_probe(
                        "/v2/vectordb/aliases/create",
                        r#"{"aliasName":"test_alias","collectionName":c}"#,
                        param, &label, true,
                    )
                } else if ep_lower.contains("database") {
                    format!(
                        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
db = 'oracle_db_' + uuid.uuid4().hex[:8]
body = {{"dbName":db}}
body.pop("{param}", None)
r = requests.post(f'{{BASE}}/v2/vectordb/databases/create', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
                        param = param, label = label,
                    )
                } else if param == "userName" || param == "password" || param == "newPassword" {
                    format!(
                        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
c = 'test_' + uuid.uuid4().hex[:8]
body = {{"userName":"test_user_{}","password":"Test123456"}}
body.pop("{param}", None)
r = requests.post(f'{{BASE}}/v2/vectordb/users/create', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
                        param = param, label = label,
                    )
                } else if param == "roleName" || param == "objectType" || param == "privilege" || param == "objectName" {
                    format!(
                        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
c = 'test_' + uuid.uuid4().hex[:8]
body = {{"roleName":"test_role_{}","objectType":"Collection","privilege":"Search","objectName":c}}
body.pop("{param}", None)
r = requests.post(f'{{BASE}}/v2/vectordb/roles/grant_privilege', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
                        param = param, label = label,
                    )
                } else {
                    String::new()
                }
            }
            EndpointType::Upsert => {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/entities/upsert",
                    r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#,
                    param, &label, true,
                )
            }
            EndpointType::Delete | EndpointType::Scroll => {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/entities/delete",
                    r#"{"collectionName":c,"filter":"id > 0"}"#,
                    param, &label, true,
                )
            }
            EndpointType::Recommend => {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/entities/search",
                    r#"{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}"#,
                    param, &label, true,
                )
            }
            EndpointType::Search => {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/entities/search",
                    r#"{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}"#,
                    param, &label, true,
                )
            }
        }
    } else {
        let ep_lower = endpoint.to_lowercase();
        if ep_lower.contains("role") {
            if ep_lower.contains("revoke") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/roles/revoke_privilege",
                    r#"{"roleName":"test_role","objectType":"Collection","objectName":"*","privilege":"Search"}"#,
                    param, &label, false,
                )
            } else if ep_lower.contains("describe") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/roles/describe",
                    r#"{"roleName":"test_role"}"#,
                    param, &label, false,
                )
            } else {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/roles/grant_privilege",
                    r#"{"roleName":"test_role","objectType":"Collection","objectName":"*","privilege":"Search"}"#,
                    param, &label, false,
                )
            }
        } else if ep_lower.contains("user") {
            if ep_lower.contains("update") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/users/update_password",
                    r#"{"userName":"test_user","password":"old_pass","newPassword":"new_pass123"}"#,
                    param, &label, false,
                )
            } else if ep_lower.contains("describe") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/users/describe",
                    r#"{"userName":"test_user"}"#,
                    param, &label, false,
                )
            } else if ep_lower.contains("drop") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/users/drop",
                    r#"{"userName":"test_user"}"#,
                    param, &label, false,
                )
            } else {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/users/create",
                    r#"{"userName":"test_user","password":"test_pass"}"#,
                    param, &label, false,
                )
            }
        } else if ep_lower.contains("partition") {
            if ep_lower.contains("release") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/partitions/release",
                    r#"{"collectionName":c,"partitionNames":["_default"]}"#,
                    param, &label, true,
                )
            } else if ep_lower.contains("load") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/partitions/load",
                    r#"{"collectionName":c,"partitionNames":["_default"]}"#,
                    param, &label, true,
                )
            } else if ep_lower.contains("has") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/partitions/has",
                    r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                    param, &label, true,
                )
            } else if ep_lower.contains("drop") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/partitions/drop",
                    r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                    param, &label, true,
                )
            } else {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/partitions/create",
                    r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                    param, &label, true,
                )
            }
        } else if ep_lower.contains("index") {
            if ep_lower.contains("describe") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/indexes/describe",
                    r#"{"collectionName":c,"indexName":"vector"}"#,
                    param, &label, true,
                )
            } else if ep_lower.contains("drop") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/indexes/drop",
                    r#"{"collectionName":c,"indexName":"vector"}"#,
                    param, &label, true,
                )
            } else {
                crate::agent::probe_milvus::generate_mutation_probe_with_marker_no_index(
                    "/v2/vectordb/indexes/create",
                    r#"{"collectionName":c,"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                    &format!(r#"body.pop("{}", None)"#, param),
                    &label, "ILLEGAL_SUCCESS",
                )
            }
        } else if ep_lower.contains("alias") {
            if ep_lower.contains("describe") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/aliases/describe",
                    r#"{"aliasName":"test_alias"}"#,
                    param, &label, true,
                )
            } else if ep_lower.contains("drop") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/aliases/drop",
                    r#"{"aliasName":"test_alias"}"#,
                    param, &label, true,
                )
            } else if ep_lower.contains("alter") {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/aliases/alter",
                    r#"{"aliasName":"test_alias","collectionName":c}"#,
                    param, &label, true,
                )
            } else {
                crate::agent::probe_milvus::generate_missing_field_probe(
                    "/v2/vectordb/aliases/create",
                    r#"{"aliasName":"test_alias","collectionName":c}"#,
                    param, &label, true,
                )
            }
        } else if ep_lower.contains("database") {
            let db_ep = if ep_lower.contains("drop") {
                "/v2/vectordb/databases/drop"
            } else if ep_lower.contains("describe") {
                "/v2/vectordb/databases/describe"
            } else if ep_lower.contains("alter") {
                "/v2/vectordb/databases/alter"
            } else {
                "/v2/vectordb/databases/create"
            };
            format!(
                r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
db = 'oracle_db_' + uuid.uuid4().hex[:8]
body = {{"dbName":db}}
body.pop("{param}", None)
r = requests.post(f'{{BASE}}{db_ep}', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
                param = param, label = label, db_ep = db_ep,
            )
        } else if ep_lower.contains("rename") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/collections/rename",
                r#"{"collectionName":c,"newCollectionName":"renamed_" + c}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("get_stats") || ep_lower.contains("stats") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/collections/get_stats",
                r#"{"collectionName":c}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("entities/get") || ep_lower.contains("get") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/entities/get",
                r#"{"collectionName":c,"id":1}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("release") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/collections/release",
                r#"{"collectionName":c}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("upsert") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/entities/upsert",
                r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("insert") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/entities/insert",
                r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("query") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/entities/query",
                r#"{"collectionName":c,"filter":"id > 0","limit":10}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("delete") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/entities/delete",
                r#"{"collectionName":c,"filter":"id > 0"}"#,
                param, &label, true,
            )
        } else if ep_lower.contains("create") {
            crate::agent::probe_milvus::generate_missing_field_probe(
                "/v2/vectordb/collections/create",
                r#"{"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                param, &label, false,
            )
        } else {
            String::new()
        }
    }
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

fn milvus_invalid_enum_probe(param: &str, endpoint: &str) -> String {
    let label = format!("invalid enum {}", param);
    let mutation = format!(r#"body["{}"] = "INVALID_ENUM_VALUE_42""#, param);
    if endpoint.contains('+') {
        let ep_type = milvus_classify_param_endpoint(param, endpoint);
        match ep_type {
            EndpointType::Create => {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/collections/create",
                    r#"{"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                    &mutation, &label, false,
                )
            }
            EndpointType::Config => {
                let ep_lower = endpoint.to_lowercase();
                if ep_lower.contains("index") {
                    crate::agent::probe_milvus::generate_mutation_probe_with_marker_no_index(
                        "/v2/vectordb/indexes/create",
                        r#"{"collectionName":c,"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                        &mutation, &label, "ILLEGAL_SUCCESS",
                    )
                } else if ep_lower.contains("partition") {
                    crate::agent::probe_milvus::generate_mutation_probe(
                        "/v2/vectordb/partitions/create",
                        r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                        &mutation, &label, true,
                    )
                } else if ep_lower.contains("alias") {
                    crate::agent::probe_milvus::generate_mutation_probe(
                        "/v2/vectordb/aliases/create",
                        r#"{"aliasName":"test_alias","collectionName":c}"#,
                        &mutation, &label, true,
                    )
                } else if ep_lower.contains("database") {
                    format!(
                        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
db = 'oracle_db_' + uuid.uuid4().hex[:8]
body = {{"dbName":db}}
body["{param}"] = "INVALID_ENUM_VALUE_42"
r = requests.post(f'{{BASE}}/v2/vectordb/databases/create', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
                        param = param, label = label,
                    )
                } else {
                    String::new()
                }
            }
            EndpointType::Upsert => {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/entities/upsert",
                    r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#,
                    &mutation, &label, true,
                )
            }
            EndpointType::Delete | EndpointType::Scroll => {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/entities/delete",
                    r#"{"collectionName":c,"filter":"id > 0"}"#,
                    &mutation, &label, true,
                )
            }
            EndpointType::Recommend => {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/entities/search",
                    r#"{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}"#,
                    &mutation, &label, true,
                )
            }
            EndpointType::Search => {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/entities/search",
                    r#"{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}"#,
                    &mutation, &label, true,
                )
            }
        }
    } else {
        let ep_lower = endpoint.to_lowercase();
        if ep_lower.contains("partition") {
            if ep_lower.contains("release") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/partitions/release",
                    r#"{"collectionName":c,"partitionNames":["_default"]}"#,
                    &mutation, &label, true,
                )
            } else if ep_lower.contains("has") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/partitions/has",
                    r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                    &mutation, &label, true,
                )
            } else if ep_lower.contains("drop") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/partitions/drop",
                    r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                    &mutation, &label, true,
                )
            } else {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/partitions/create",
                    r#"{"collectionName":c,"partitionName":"test_partition"}"#,
                    &mutation, &label, true,
                )
            }
        } else if ep_lower.contains("index") {
            if ep_lower.contains("describe") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/indexes/describe",
                    r#"{"collectionName":c,"indexName":"vector"}"#,
                    &mutation, &label, true,
                )
            } else if ep_lower.contains("drop") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/indexes/drop",
                    r#"{"collectionName":c,"indexName":"vector"}"#,
                    &mutation, &label, true,
                )
            } else {
                crate::agent::probe_milvus::generate_mutation_probe_with_marker_no_index(
                    "/v2/vectordb/indexes/create",
                    r#"{"collectionName":c,"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                    &mutation, &label, "ILLEGAL_SUCCESS",
                )
            }
        } else if ep_lower.contains("alias") {
            if ep_lower.contains("describe") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/aliases/describe",
                    r#"{"aliasName":"test_alias"}"#,
                    &mutation, &label, true,
                )
            } else if ep_lower.contains("drop") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/aliases/drop",
                    r#"{"aliasName":"test_alias"}"#,
                    &mutation, &label, true,
                )
            } else if ep_lower.contains("alter") {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/aliases/alter",
                    r#"{"aliasName":"test_alias","collectionName":c}"#,
                    &mutation, &label, true,
                )
            } else {
                crate::agent::probe_milvus::generate_mutation_probe(
                    "/v2/vectordb/aliases/create",
                    r#"{"aliasName":"test_alias","collectionName":c}"#,
                    &mutation, &label, true,
                )
            }
        } else if ep_lower.contains("database") {
            let db_ep = if ep_lower.contains("drop") {
                "/v2/vectordb/databases/drop"
            } else if ep_lower.contains("describe") {
                "/v2/vectordb/databases/describe"
            } else if ep_lower.contains("alter") {
                "/v2/vectordb/databases/alter"
            } else {
                "/v2/vectordb/databases/create"
            };
            format!(
                r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
db = 'oracle_db_' + uuid.uuid4().hex[:8]
body = {{"dbName":db}}
body["{param}"] = "INVALID_ENUM_VALUE_42"
r = requests.post(f'{{BASE}}{db_ep}', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
                param = param, label = label, db_ep = db_ep,
            )
        } else if ep_lower.contains("upsert") {
            crate::agent::probe_milvus::generate_mutation_probe(
                "/v2/vectordb/entities/upsert",
                r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#,
                &mutation, &label, true,
            )
        } else if ep_lower.contains("insert") {
            crate::agent::probe_milvus::generate_mutation_probe(
                "/v2/vectordb/entities/insert",
                r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#,
                &mutation, &label, true,
            )
        } else if ep_lower.contains("query") {
            crate::agent::probe_milvus::generate_mutation_probe(
                "/v2/vectordb/entities/query",
                r#"{"collectionName":c,"filter":"id > 0","limit":10}"#,
                &mutation, &label, true,
            )
        } else if ep_lower.contains("delete") {
            crate::agent::probe_milvus::generate_mutation_probe(
                "/v2/vectordb/entities/delete",
                r#"{"collectionName":c,"filter":"id > 0"}"#,
                &mutation, &label, true,
            )
        } else if ep_lower.contains("create") {
            crate::agent::probe_milvus::generate_mutation_probe(
                "/v2/vectordb/collections/create",
                r#"{"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#,
                &mutation, &label, false,
            )
        } else {
            crate::agent::probe_milvus::generate_mutation_probe(
                "/v2/vectordb/entities/search",
                r#"{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}"#,
                &mutation, &label, true,
            )
        }
    }
}

fn weaviate_generate_probe(ep_type: EndpointType, param: &str, value: &str, label: &str) -> String {
    match ep_type {
        EndpointType::Config | EndpointType::Create => {
            let injection = weaviate_config_injection(param, value);
            format!(
                r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
body = {{"class":c,"vectorizer":"none","vectorIndexConfig":{{"distance":"cosine","efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}}
{injection}
r = requests.post(f'{{BASE}}/v1/schema', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
                injection = injection,
                label = label,
            )
        }
        EndpointType::Search => {
            format!(
                r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v1/schema', json={{"class":c,"vectorizer":"none","vectorIndexConfig":{{"distance":"cosine","efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}})
if r.status_code != 200: print(f'setup failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{{BASE}}/v1/objects', json={{"class":c,"properties":{{"title":"test"}},"vector":[0.1,0.2,0.3,0.4]}})
if r.status_code != 200: print(f'insert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
q = "{{ Get {{ " + c + "(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} {param}: {value}) {{ title _additional {{ distance }} }} }} }}"
body = {{"query": q}}
r = requests.post(f'{{BASE}}/v1/graphql', json=body)
if r.status_code == 200 and not r.json().get('errors'): print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
                param = param,
                value = value,
                label = label,
            )
        }
        EndpointType::Upsert => {
            format!(
                r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v1/schema', json={{"class":c,"vectorizer":"none","vectorIndexConfig":{{"distance":"cosine","efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}})
if r.status_code != 200: print(f'setup failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.5)
body = {{"class":c,"properties":{{"title":"test"}},"vector":[0.1,0.2,0.3,0.4]}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v1/objects', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
                param = param,
                value = value,
                label = label,
            )
        }
        _ => String::new(),
    }
}

fn weaviate_config_injection(param: &str, value: &str) -> String {
    let p_lower = param.to_lowercase().replace("vectorindexconfig.", "").replace("replicationconfig.", "");
    if p_lower == "class" {
        format!(r#"body["class"] = {}"#, value)
    } else if p_lower == "distance" {
        format!(r#"body["vectorIndexConfig"]["distance"] = "{}""#, value)
    } else if matches!(p_lower.as_str(), "efconstruction" | "maxconnections" | "ef" | "dynamicefmin" | "dynamicefmax" | "dynamiceffactor" | "flatsearchcutoff" | "cleanupintervalseconds" | "vectorcachemaxobjects") {
        let key = param.strip_prefix("vectorIndexConfig.").unwrap_or(param);
        format!(r#"body["vectorIndexConfig"]["{}"] = {}"#, key, value)
    } else if p_lower == "factor" || param == "replicationConfig.factor" {
        format!(r#"body.setdefault("replicationConfig", {{}})["factor"] = {}"#, value)
    } else {
        format!(r#"body["{}"] = {}"#, param, value)
    }
}

fn weaviate_missing_required_probe(param: &str, endpoint: &str) -> String {
    let p_lower = param.to_lowercase();
    let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);
    match ep_type {
        EndpointType::Config | EndpointType::Create => {
            if p_lower == "class" {
                format!(
                    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
body = {{"vectorizer":"none","vectorIndexConfig":{{"distance":"cosine","efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}}
r = requests.post(f'{{BASE}}/v1/schema', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] missing required {param} accepted'); sys.exit(1)
else: print(f'properly rejected missing {param}: {{r.status_code}}'); sys.exit(0)"#,
                    param = param,
                )
            } else if p_lower == "distance" {
                format!(
                    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
body = {{"class":c,"vectorizer":"none","vectorIndexConfig":{{"efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}}
r = requests.post(f'{{BASE}}/v1/schema', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] missing required {param} accepted'); sys.exit(1)
else: print(f'properly rejected missing {param}: {{r.status_code}}'); sys.exit(0)"#,
                    param = param,
                )
            } else {
                format!(
                    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
body = {{"class":c,"vectorizer":"none","vectorIndexConfig":{{"distance":"cosine","efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}}
body.pop("{param}", None)
r = requests.post(f'{{BASE}}/v1/schema', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] missing required {param} accepted'); sys.exit(1)
else: print(f'properly rejected missing {param}: {{r.status_code}}'); sys.exit(0)"#,
                    param = param,
                )
            }
        }
        _ => String::new(),
    }
}

fn weaviate_invalid_enum_probe(param: &str, endpoint: &str) -> String {
    let p_lower = param.to_lowercase();
    let ep_type = crate::agent::probe::classify_endpoint_type(param, endpoint);
    match ep_type {
        EndpointType::Config | EndpointType::Create => {
            if p_lower == "distance" {
                format!(
                    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
body = {{"class":c,"vectorizer":"none","vectorIndexConfig":{{"distance":"INVALID_METRIC","efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}}
r = requests.post(f'{{BASE}}/v1/schema', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] invalid enum {param} accepted'); sys.exit(1)
else: print(f'properly rejected invalid enum {param}: {{r.status_code}}'); sys.exit(0)"#,
                    param = param,
                )
            } else {
                format!(
                    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
body = {{"class":c,"vectorizer":"none","vectorIndexConfig":{{"distance":"cosine","efConstruction":128,"maxConnections":64}},"replicationConfig":{{}},"properties":[{{"name":"title","dataType":["string"]}}]}}
body["{param}"] = "INVALID_ENUM_VALUE_42"
r = requests.post(f'{{BASE}}/v1/schema', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] invalid enum {param} accepted'); sys.exit(1)
else: print(f'properly rejected invalid enum {param}: {{r.status_code}}'); sys.exit(0)"#,
                    param = param,
                )
            }
        }
        _ => String::new(),
    }
}

fn weaviate_search_params_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v1/schema', json={{"class":c,"vectorizer":"none","vectorIndexConfig":{{"distance":"cosine","efConstruction":128,"maxConnections":64}},"properties":[{{"name":"title","dataType":["string"]}}]}})
if r.status_code != 200: print(f'setup failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{{BASE}}/v1/objects', json={{"class":c,"properties":{{"title":"test"}},"vector":[0.1,0.2,0.3,0.4]}})
if r.status_code != 200: print(f'insert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
body = {{"query": "{{ Get {{ " + c + "(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} {param}: {value}) {{ title _additional {{ distance }} }} }} }}"}}
r = requests.post(f'{{BASE}}/v1/graphql', json=body)
if r.status_code == 200 and not r.json().get('errors'): print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

fn format_boundary(v: f64) -> String {
    crate::agent::probe::format_boundary(v)
}

fn is_header_param(param_name: &str) -> bool {
    matches!(
        param_name,
        "Request-Timeout" | "Authorization" | "Request-Header"
    )
}

fn is_zero_valid_param(param_name: &str) -> bool {
    matches!(param_name, "offset")
}

fn is_param_with_default(param_name: &str) -> bool {
    let params = ["autoID", "autoId", "enableDynamicField", "dbName"];
    params.iter().any(|p| p.eq_ignore_ascii_case(param_name))
}

fn is_silently_ignored_param(_param_name: &str) -> bool {
    false
}

fn is_optional_empty_string_param(param_name: &str) -> bool {
    let optional_params = [
        "dbName", "filter", "groupingField", "annsField", "partitionName",
        "params.shardsNum", "params.max_length", "params.ttlSeconds",
        "params.partitionsNum", "params.consistencyLevel", "params.enableDynamicField",
        "roleName", "userName", "password", "newPassword", "indexName",
        "collectionName", "newCollectionName", "aliasName",
    ];
    optional_params.iter().any(|p| p.eq_ignore_ascii_case(param_name))
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
            None,
            None,
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
            None,
            None,
        );
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|c| c.name.contains("zero")));
        assert!(cases.iter().any(|c| c.script.contains("{{TESTVDB_AUTH_HEADER}}")));
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
            None,
            None,
        );
        assert!(cases.iter().any(|c| c.name.contains("float_type")));
        assert!(cases.iter().any(|c| c.name.contains("string_type")));
    }

    #[test]
    #[allow(deprecated)]
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
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let cases = BoundaryValueGenerator::from_contract(&contract, TargetStyle::Qdrant);
        assert!(cases.len() >= 3);
        assert!(cases.iter().all(|c| !c.script.contains("{{TESTVDB_AUTH_HEADER}}")));
    }

    #[test]
    #[allow(deprecated)]
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
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };
        let cases = BoundaryValueGenerator::from_contract(&contract, TargetStyle::Milvus);
        assert!(cases.len() >= 3);
        assert!(cases.iter().all(|c| c.script.contains("{{TESTVDB_AUTH_HEADER}}")));
        assert!(cases.iter().all(|c| c.script.contains("r.json().get('code')")));
    }

    #[test]
    fn test_milvus_search_param_dispatch() {
        use crate::contract::store::ContractStore;
        use std::collections::HashMap;

        let mut store = ContractStore::new("milvus", "2.4");
        let mut ep_map: HashMap<String, Vec<String>> = HashMap::new();
        ep_map.insert("searchParams".to_string(), vec!["nprobe".to_string(), "ef".to_string()]);
        store.nested_params.insert("search".to_string(), ep_map);
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "nprobe".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: crate::contract::store::ConstraintSource::OpenapiDerived,
            confidence: crate::contract::store::Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

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
            Some(&store),
            Some(RejectionPolicy::Reject),
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
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.set_required_params("search", vec!["collectionName".to_string()]);
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "metricType".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.set_enum_values("metricType", vec!["COSINE".to_string(), "L2".to_string(), "IP".to_string()]);

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.len() >= 5);
        assert!(cases.iter().filter(|c| !c.script.is_empty()).all(|c| c.script.contains("{{TESTVDB_AUTH_HEADER}}")));
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
            endpoint: Some("search_points".to_string()),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec![],
            },
            endpoint: Some("search_points".to_string()),
            source: ConstraintSource::ExplicitDoc,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Qdrant);
        assert!(cases.len() >= 3);
        assert!(cases.iter().all(|c| !c.script.contains("{{TESTVDB_AUTH_HEADER}}")));
    }

    #[test]
    fn test_dimension_not_in_search_cases() {
        use crate::contract::store::{ContractStore, ConstraintSource, Confidence};

        let mut store = ContractStore::new("milvus", "2.4");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "nprobe".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "nprobe".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(65536.0),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "dimension".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("create".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "dimension".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(32768.0),
                violation_examples: vec![],
            },
            endpoint: Some("create".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Milvus);

        let search_cases: Vec<_> = cases.iter().filter(|c| c.coverage_entry.as_ref().map_or(false, |(ep, _, _)| ep == "search")).collect();
        let create_cases: Vec<_> = cases.iter().filter(|c| c.coverage_entry.as_ref().map_or(false, |(ep, _, _)| ep == "create")).collect();

        assert!(search_cases.iter().all(|c| !c.name.contains("dimension")), "search cases should not contain dimension");
        assert!(create_cases.iter().any(|c| c.name.contains("dimension")), "create cases should contain dimension");

        let nprobe_cases: Vec<_> = search_cases.iter().filter(|c| c.name.contains("nprobe")).collect();
        assert!(!nprobe_cases.is_empty(), "should have nprobe boundary cases");
        assert!(nprobe_cases.iter().all(|c| c.defect_marker == "PARAM_IGNORED"),
            "nprobe cases should be PARAM_IGNORED, got: {:?}", nprobe_cases.iter().map(|c| &c.defect_marker).collect::<Vec<_>>());
    }

    #[test]
    fn test_milvus_boundary_marker_distribution() {
        use crate::contract::store::{ContractStore, ConstraintSource, Confidence};
        use std::collections::HashMap;

        let mut store = ContractStore::new("milvus", "2.4");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "limit".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "nprobe".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "nprobe".to_string(),
                description: ">=1".to_string(),
                min: Some(1.0),
                max: Some(65536.0),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Ignore),
        });
        store.set_required_params("search", vec!["collectionName".to_string()]);
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "metricType".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.set_enum_values("metricType", vec!["COSINE".to_string(), "L2".to_string(), "IP".to_string()]);

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Milvus);

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for case in &cases {
            *counts.entry(&case.defect_marker).or_insert(0) += 1;
        }
        eprintln!("Boundary marker distribution: {:?}", counts);

        assert!(*counts.get("PARAM_IGNORED").unwrap_or(&0) > 0, "PARAM_IGNORED should exist");
        assert!(*counts.get("ILLEGAL_SUCCESS").unwrap_or(&0) > 0, "ILLEGAL_SUCCESS should exist");
    }

    #[test]
    fn test_header_param_uses_headers_not_body() {
        use crate::contract::store::{ContractStore, ConstraintSource, Confidence};

        let mut store = ContractStore::new("milvus", "2.4");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "Request-Timeout".to_string(),
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
                param_name: "Authorization".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Milvus);

        let header_cases: Vec<_> = cases.iter().filter(|c| c.name.contains("Request-Timeout") || c.name.contains("Authorization")).collect();
        let body_cases: Vec<_> = cases.iter().filter(|c| c.name.contains("limit")).collect();

        assert!(header_cases.is_empty(), "header params should be skipped (gRPC gateway does not validate headers)");
        assert!(!body_cases.is_empty(), "should have non-header param cases");
        for case in &body_cases {
            assert!(case.script.contains("body[\"limit\"]"), "non-header param should still use body injection, got: {}", case.name);
            assert!(!case.script.contains("HEADERS[\"limit\"]"), "non-header param should not use HEADERS injection, got: {}", case.name);
        }
    }

    #[test]
    fn test_from_store_weaviate_with_contract() {
        use crate::contract::store::{ContractStore, ConstraintSource, Confidence};
        use std::path::Path;

        let contract_path = Path::new("contracts/weaviate_contract.json");
        if !contract_path.exists() {
            eprintln!("Skipping: contracts/weaviate_contract.json not found");
            return;
        }
        let content = std::fs::read_to_string(contract_path).unwrap();
        let contract: crate::contract::schema::StructuredContract = serde_json::from_str(&content).unwrap();

        let store = ContractStore::from_structured_contracts(
            "weaviate", "1.37.5", &[contract],
            ConstraintSource::ExplicitDoc, Confidence::Medium,
        );

        eprintln!("Weaviate store: {} type_constraints, {} range_constraints",
            store.type_constraints.len(), store.range_constraints.len());

        for arc in &store.range_constraints {
            let is_for_ep = store.is_param_for_endpoint(&arc.constraint.param_name, arc.endpoint.as_deref().unwrap_or(""));
            let is_header = is_header_param(&arc.constraint.param_name);
            eprintln!("  range: param={} endpoint={} is_for_ep={} is_header={}",
                arc.constraint.param_name, arc.endpoint.as_deref().unwrap_or(""), is_for_ep, is_header);
        }

        for atc in &store.type_constraints {
            let is_for_ep = store.is_param_for_endpoint(&atc.constraint.param_name, atc.endpoint.as_deref().unwrap_or(""));
            let is_header = is_header_param(&atc.constraint.param_name);
            eprintln!("  type: param={} endpoint={} is_for_ep={} is_header={}",
                atc.constraint.param_name, atc.endpoint.as_deref().unwrap_or(""), is_for_ep, is_header);
        }

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Weaviate);
        eprintln!("Weaviate boundary cases: {}", cases.len());
        for case in &cases {
            eprintln!("  case: {} (script_len={})", case.name, case.script.len());
        }

        assert!(cases.len() >= 5, "Should generate at least 5 Weaviate boundary cases, got {}", cases.len());
    }
}