use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use crate::agent::classifier::DefectType;
use crate::sandbox::manager::Sandbox;
use super::{IndependentReviewer, ReviewResult};

pub struct WeaviateIndependentReviewer;

#[async_trait]
impl IndependentReviewer for WeaviateIndependentReviewer {
    fn target_name(&self) -> &str {
        "weaviate"
    }

    async fn run_probe(&self, sandbox: &Sandbox, port: u16) -> Result<ReviewResult> {
        let db_url = crate::infra::build_db_url(sandbox.db_host.as_ref().ok_or_else(|| anyhow::anyhow!("sandbox db_host missing"))?, port);
        let probe_script = WEAVIATE_REVIEW_PROBE_TEMPLATE.replace("__DB_URL__", &db_url);
        let output = sandbox.exec_script(
            &probe_script,
            &[("TESTVDB_DB_URL", &db_url)],
        ).await?;
        if !output.success {
            anyhow::bail!(
                "Independent Weaviate probe failed.\nSTDOUT:\n{}\nSTDERR:\n{}",
                output.stdout,
                output.stderr
            );
        }
        let result: Value = serde_json::from_str(output.stdout.trim())?;
        Ok(result)
    }

    fn summarize_findings(&self, probe_json: &ReviewResult) -> Option<(DefectType, Vec<String>)> {
        summarize_weaviate_probe(probe_json)
    }
}

const WEAVIATE_REVIEW_PROBE_TEMPLATE: &str = r#"
import json, requests, time, uuid

BASE_URL = "__DB_URL__"
col = "review_wv_test"

requests.delete(f"{BASE_URL}/v1/schema/{col}")
create_resp = requests.post(f"{BASE_URL}/v1/schema", json={
    "class": col, "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "ef": 128, "maxConnections": 32},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
time.sleep(1.0)

# Insert objects
oid1 = str(uuid.uuid4())
oid2 = str(uuid.uuid4())
obj1_resp = requests.post(f"{BASE_URL}/v1/objects", json={
    "class": col, "id": oid1, "vector": [0.1, 0.2, 0.3, 0.4], "properties": {"text": "hello"}})
obj2_resp = requests.post(f"{BASE_URL}/v1/objects", json={
    "class": col, "id": oid2, "vector": [0.5, 0.6, 0.7, 0.8], "properties": {"text": "world"}})
time.sleep(0.5)

# Search with nearVector
search_resp = requests.post(f"{BASE_URL}/v1/graphql", json={"query": f"{{Get {{{col}(nearVector: {{vector: [0.1,0.2,0.3,0.4]}}, limit: 5) {{text}}}}}}"})
bad_dim_resp = requests.post(f"{BASE_URL}/v1/objects", json={
    "class": col, "id": str(uuid.uuid4()), "vector": [0.1, 0.2, 0.3], "properties": {"text": "bad"}})

# Boundary: ef=-1
ef_neg_coll = "review_wv_efneg"
requests.delete(f"{BASE_URL}/v1/schema/{ef_neg_coll}")
ef_neg_resp = requests.post(f"{BASE_URL}/v1/schema", json={
    "class": ef_neg_coll, "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "ef": -1},
    "properties": [{"name": "text", "dataType": ["text"]}]})

# Boundary: maxConnections=0
mc_zero_coll = "review_wv_mc0"
requests.delete(f"{BASE_URL}/v1/schema/{mc_zero_coll}")
mc_zero_resp = requests.post(f"{BASE_URL}/v1/schema", json={
    "class": mc_zero_coll, "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "maxConnections": 0},
    "properties": [{"name": "text", "dataType": ["text"]}]})

# Boundary: invalid distance
bad_dist_coll = "review_wv_baddist"
requests.delete(f"{BASE_URL}/v1/schema/{bad_dist_coll}")
bad_dist_resp = requests.post(f"{BASE_URL}/v1/schema", json={
    "class": bad_dist_coll, "vectorizer": "none",
    "vectorIndexConfig": {"distance": "InvalidDistance"},
    "properties": [{"name": "text", "dataType": ["text"]}]})

# State: count consistency
state_coll = "review_wv_state"
requests.delete(f"{BASE_URL}/v1/schema/{state_coll}")
requests.post(f"{BASE_URL}/v1/schema", json={
    "class": state_coll, "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine"},
    "properties": [{"name": "text", "dataType": ["text"]}]})
time.sleep(0.5)
for i in range(5):
    requests.post(f"{BASE_URL}/v1/objects", json={
        "class": state_coll, "id": str(uuid.uuid4()),
        "vector": [0.1*i, 0.2*i, 0.3*i, 0.4*i], "properties": {"text": f"item{i}"}})
time.sleep(1.0)
state_count_resp = requests.get(f"{BASE_URL}/v1/objects?class={state_coll}&limit=1")
state_count = state_count_resp.json().get("totalResults", -1)

# Cleanup
requests.delete(f"{BASE_URL}/v1/schema/{col}")
requests.delete(f"{BASE_URL}/v1/schema/{ef_neg_coll}")
requests.delete(f"{BASE_URL}/v1/schema/{mc_zero_coll}")
requests.delete(f"{BASE_URL}/v1/schema/{bad_dist_coll}")
requests.delete(f"{BASE_URL}/v1/schema/{state_coll}")

print(json.dumps({
    "create_status": create_resp.status_code,
    "obj1_status": obj1_resp.status_code,
    "obj2_status": obj2_resp.status_code,
    "search_status": search_resp.status_code,
    "bad_dim_status": bad_dim_resp.status_code,
    "ef_neg_status": ef_neg_resp.status_code,
    "mc_zero_status": mc_zero_resp.status_code,
    "bad_dist_status": bad_dist_resp.status_code,
    "state_count": state_count,
}))
"#;

fn summarize_weaviate_probe(probe_value: &Value) -> Option<(DefectType, Vec<String>)> {
    let mut illegal_issues: Vec<String> = Vec::new();

    let status = |key: &str| -> i64 {
        probe_value.get(key).and_then(|v| v.as_i64()).unwrap_or(-1)
    };

    if status("bad_dim_status") == 200 {
        illegal_issues.push("Dimension mismatch insert accepted (status=200)".to_string());
    }
    if status("ef_neg_status") == 200 {
        illegal_issues.push("ef=-1 accepted in vectorIndexConfig (status=200)".to_string());
    }
    if status("mc_zero_status") == 200 {
        illegal_issues.push("maxConnections=0 accepted in vectorIndexConfig (status=200)".to_string());
    }
    if status("bad_dist_status") == 200 {
        illegal_issues.push("Invalid distance metric accepted (status=200)".to_string());
    }

    if !illegal_issues.is_empty() {
        return Some((DefectType::IllegalSuccess, illegal_issues));
    }

    let count = probe_value.get("state_count").and_then(|v| v.as_i64()).unwrap_or(-1);
    if count >= 0 && count != 5 {
        return Some((DefectType::StateLogicViolation,
            vec![format!("Insert 5 objects but count={}", count)]));
    }

    None
}
