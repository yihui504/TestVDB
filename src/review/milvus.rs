use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use crate::agent::classifier::DefectType;
use crate::sandbox::manager::Sandbox;
use super::{IndependentReviewer, ReviewResult};

pub struct MilvusIndependentReviewer;

#[async_trait]
impl IndependentReviewer for MilvusIndependentReviewer {
    fn target_name(&self) -> &str {
        "milvus"
    }

    async fn run_probe(&self, sandbox: &Sandbox, port: u16) -> Result<ReviewResult> {
        let db_url = crate::infra::build_db_url(sandbox.db_host.as_ref().ok_or_else(|| anyhow::anyhow!("sandbox db_host missing"))?, port);
        let probe_script = MILVUS_REVIEW_PROBE_TEMPLATE.replace("__DB_URL__", &db_url);
        let output = sandbox.exec_script(
            &probe_script,
            &[("TESTVDB_DB_URL", &db_url)],
        ).await?;
        if !output.success {
            anyhow::bail!(
                "Independent Milvus probe failed.\nSTDOUT:\n{}\nSTDERR:\n{}",
                output.stdout,
                output.stderr
            );
        }
        let result: Value = serde_json::from_str(output.stdout.trim())?;
        Ok(result)
    }

    fn summarize_findings(&self, probe_json: &ReviewResult) -> Option<(DefectType, Vec<String>)> {
        summarize_milvus_independent_probe(probe_json)
    }
}

const MILVUS_REVIEW_PROBE_TEMPLATE: &str = r#"
import json
import requests
import time

BASE_URL = "__DB_URL__"
HEADERS = {"Authorization": "{{TESTVDB_AUTH_HEADER}}", "Content-Type": "application/json"}

def milvus_post(path, body):
    return requests.post(f"{BASE_URL}{path}", headers=HEADERS, json=body)

def milvus_code(resp):
    return resp.json().get("code", -1)

def create_collection(name, dim=4, metric="COSINE", index_type="AUTOINDEX"):
    return milvus_post("/v2/vectordb/collections/create", {
        "collectionName": name,
        "schema": {
            "autoID": False,
            "enableDynamicField": True,
            "fields": [
                {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
                {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": dim}}
            ]
        },
        "indexParams": [
            {"fieldName": "vector", "metricType": metric, "indexType": index_type}
        ]
    })

def drop_collection(name):
    return milvus_post("/v2/vectordb/collections/drop", {"collectionName": name})

def insert_data(name, data):
    return milvus_post("/v2/vectordb/entities/insert", {"collectionName": name, "data": data})

def search(name, vector, **kwargs):
    body = {"collectionName": name, "data": [vector], "limit": 5}
    body.update(kwargs)
    return milvus_post("/v2/vectordb/entities/search", body)

def get_row_count(name):
    resp = milvus_post("/v2/vectordb/collections/get_stats", {"collectionName": name})
    return resp.json().get("data", {}).get("rowCount", -1)

coll = "review_test_coll"

drop_collection(coll)
create_resp = create_collection(coll)
time.sleep(1)

insert_resp = insert_data(coll, [
    {"id": 1, "vector": [0.1, 0.2, 0.3, 0.4], "color": "red"},
    {"id": 2, "vector": [0.5, 0.6, 0.7, 0.8], "color": "blue"},
    {"id": 3, "vector": [0.9, 1.0, 1.1, 1.2], "color": "red"}
])
time.sleep(1)

wrong_dim_search_resp = search(coll, [0.1, 0.2, 0.3])
limit_zero_resp = search(coll, [0.1, 0.2, 0.3, 0.4], limit=0)
offset_neg_resp = search(coll, [0.1, 0.2, 0.3, 0.4], offset=-1)
nprobe_zero_resp = search(coll, [0.1, 0.2, 0.3, 0.4], searchParams={"params": {"nprobe": 0}})

dim_zero_resp = create_collection("review_dim_zero", dim=0)
bad_metric_resp = create_collection("review_bad_metric", metric="INVALID")
bad_index_resp = create_collection("review_bad_index", index_type="InvalidIndex")

search_no_coll_resp = search("nonexistent_xyz", [0.1, 0.2, 0.3, 0.4])

drop_collection("review_dim_zero")
drop_collection("review_bad_metric")
drop_collection("review_bad_index")

wrong_dim_insert_resp = insert_data(coll, [{"id": 99, "vector": [0.1, 0.2, 0.3]}])
time.sleep(1)
wrong_dim_count = get_row_count(coll)

state_coll = "review_state_coll"
drop_collection(state_coll)
create_collection(state_coll)
time.sleep(1)
insert_data(state_coll, [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(5)])
time.sleep(1)
state_count = get_row_count(state_coll)

milvus_post("/v2/vectordb/entities/delete", {"collectionName": state_coll, "filter": "id <= 2"})
time.sleep(1)
state_count_after_del = get_row_count(state_coll)

semantic_coll = "review_semantic_coll"
drop_collection(semantic_coll)
create_collection(semantic_coll)
time.sleep(1)
insert_data(semantic_coll, [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(10)])
time.sleep(1)
semantic_resp = search(semantic_coll, [0.5, 0.5, 0.5, 0.5], limit=10)
semantic_data = semantic_resp.json().get("data", [])
semantic_distances = [d.get("distance", -999) for d in semantic_data]
semantic_descending = semantic_distances == sorted(semantic_distances, reverse=True)

drop_collection(coll)
drop_collection(state_coll)
drop_collection(semantic_coll)

print(json.dumps({
    "create_code": milvus_code(create_resp),
    "insert_code": milvus_code(insert_resp),
    "wrong_dim_search_code": milvus_code(wrong_dim_search_resp),
    "limit_zero_code": milvus_code(limit_zero_resp),
    "offset_neg_code": milvus_code(offset_neg_resp),
    "nprobe_zero_code": milvus_code(nprobe_zero_resp),
    "dim_zero_code": milvus_code(dim_zero_resp),
    "bad_metric_code": milvus_code(bad_metric_resp),
    "bad_index_code": milvus_code(bad_index_resp),
    "search_no_coll_code": milvus_code(search_no_coll_resp),
    "wrong_dim_insert_code": milvus_code(wrong_dim_insert_resp),
    "wrong_dim_count": wrong_dim_count,
    "state_count": state_count,
    "state_count_after_del": state_count_after_del,
    "semantic_descending": semantic_descending,
    "semantic_distances": semantic_distances
}))
"#;

fn summarize_milvus_independent_probe(probe_value: &Value) -> Option<(DefectType, Vec<String>)> {
    let mut illegal_success_issues: Vec<String> = Vec::new();
    let mut state_logic_issues: Vec<String> = Vec::new();

    let code_val = |key: &str| -> i64 {
        probe_value.get(key).and_then(|v| v.as_i64()).unwrap_or(-1)
    };
    let int_val = |key: &str| -> i64 {
        probe_value.get(key).and_then(|v| v.as_i64()).unwrap_or(-1)
    };
    let bool_val = |key: &str| -> bool {
        probe_value.get(key).and_then(|v| v.as_bool()).unwrap_or(true)
    };

    if code_val("limit_zero_code") == 0 {
        illegal_success_issues.push("limit=0 search accepted (code=0) despite documented limit > 0 constraint".to_string());
    }
    if code_val("offset_neg_code") == 0 {
        illegal_success_issues.push("offset=-1 search accepted (code=0) despite documented offset >= 0 constraint".to_string());
    }
    if code_val("nprobe_zero_code") == 0 {
        illegal_success_issues.push("nprobe=0 search accepted (code=0) despite documented nprobe > 0 constraint".to_string());
    }
    if code_val("dim_zero_code") == 0 {
        illegal_success_issues.push("dim=0 collection creation accepted (code=0) despite documented dim > 0 constraint".to_string());
    }
    if code_val("bad_metric_code") == 0 {
        illegal_success_issues.push("invalid metricType accepted (code=0) despite documented enum constraint".to_string());
    }
    if code_val("bad_index_code") == 0 {
        illegal_success_issues.push("invalid indexType accepted (code=0) despite documented enum constraint".to_string());
    }
    if code_val("search_no_coll_code") == 0 {
        illegal_success_issues.push("search on nonexistent collection returned code=0".to_string());
    }
    if code_val("wrong_dim_search_code") == 0 {
        illegal_success_issues.push("wrong dimension search accepted (code=0) despite dimension mismatch".to_string());
    }
    if code_val("wrong_dim_insert_code") == 0 && int_val("wrong_dim_count") > 3 {
        illegal_success_issues.push("wrong dimension insert accepted and count increased".to_string());
    }

    if int_val("state_count") != 5 {
        state_logic_issues.push(format!("insert 5 entities but rowCount={}", int_val("state_count")));
    }
    if int_val("state_count_after_del") != 3 {
        state_logic_issues.push(format!("delete 2 of 5 but rowCount={}", int_val("state_count_after_del")));
    }
    if !bool_val("semantic_descending") {
        state_logic_issues.push("COSINE search distances not in descending order".to_string());
    }

    if !illegal_success_issues.is_empty() {
        return Some((DefectType::IllegalSuccess, illegal_success_issues));
    }
    if !state_logic_issues.is_empty() {
        return Some((DefectType::StateLogicViolation, state_logic_issues));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_milvus_probe_no_issues() {
        let result = serde_json::json!({
            "create_code": 0,
            "insert_code": 0,
            "wrong_dim_search_code": 1,
            "limit_zero_code": 1,
            "offset_neg_code": 1,
            "nprobe_zero_code": 1,
            "dim_zero_code": 1,
            "bad_metric_code": 1,
            "bad_index_code": 1,
            "search_no_coll_code": 1,
            "wrong_dim_insert_code": 1,
            "wrong_dim_count": 3,
            "state_count": 5,
            "state_count_after_del": 3,
            "semantic_descending": true,
            "semantic_distances": [0.95, 0.85, 0.75]
        });
        assert!(summarize_milvus_independent_probe(&result).is_none());
    }

    #[test]
    fn test_milvus_probe_limit_zero_illegal_success() {
        let result = serde_json::json!({
            "create_code": 0,
            "insert_code": 0,
            "wrong_dim_search_code": 1,
            "limit_zero_code": 0,
            "offset_neg_code": 1,
            "nprobe_zero_code": 1,
            "dim_zero_code": 1,
            "bad_metric_code": 1,
            "bad_index_code": 1,
            "search_no_coll_code": 1,
            "wrong_dim_insert_code": 1,
            "wrong_dim_count": 3,
            "state_count": 5,
            "state_count_after_del": 3,
            "semantic_descending": true,
            "semantic_distances": []
        });
        let summary = summarize_milvus_independent_probe(&result).expect("expected issue");
        assert_eq!(summary.0, DefectType::IllegalSuccess);
        assert!(summary.1.iter().any(|s| s.contains("limit=0")));
    }

    #[test]
    fn test_milvus_probe_state_violation() {
        let result = serde_json::json!({
            "create_code": 0,
            "insert_code": 0,
            "wrong_dim_search_code": 1,
            "limit_zero_code": 1,
            "offset_neg_code": 1,
            "nprobe_zero_code": 1,
            "dim_zero_code": 1,
            "bad_metric_code": 1,
            "bad_index_code": 1,
            "search_no_coll_code": 1,
            "wrong_dim_insert_code": 1,
            "wrong_dim_count": 3,
            "state_count": 3,
            "state_count_after_del": 3,
            "semantic_descending": true,
            "semantic_distances": []
        });
        let summary = summarize_milvus_independent_probe(&result).expect("expected issue");
        assert_eq!(summary.0, DefectType::StateLogicViolation);
        assert!(summary.1.iter().any(|s| s.contains("rowCount=3")));
    }

    #[test]
    fn test_milvus_probe_illegal_success_priority() {
        let result = serde_json::json!({
            "create_code": 0,
            "insert_code": 0,
            "wrong_dim_search_code": 1,
            "limit_zero_code": 0,
            "offset_neg_code": 1,
            "nprobe_zero_code": 1,
            "dim_zero_code": 1,
            "bad_metric_code": 1,
            "bad_index_code": 1,
            "search_no_coll_code": 1,
            "wrong_dim_insert_code": 1,
            "wrong_dim_count": 3,
            "state_count": 3,
            "state_count_after_del": 3,
            "semantic_descending": true,
            "semantic_distances": []
        });
        let summary = summarize_milvus_independent_probe(&result).expect("expected issue");
        assert_eq!(summary.0, DefectType::IllegalSuccess);
    }
}
