use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use crate::agent::classifier::DefectType;
use crate::sandbox::manager::Sandbox;
use super::{IndependentReviewer, ReviewResult};

pub struct QdrantIndependentReviewer;

#[async_trait]
impl IndependentReviewer for QdrantIndependentReviewer {
    fn target_name(&self) -> &str {
        "qdrant"
    }

    async fn run_probe(&self, sandbox: &Sandbox, port: u16) -> Result<ReviewResult> {
        let db_url = format!("http://{}:{}", sandbox.db_host.as_ref().unwrap(), port);
        let probe_script = QDRANT_REVIEW_PROBE_TEMPLATE.replace("__DB_URL__", &db_url);
        let output = sandbox.exec_command_with_env(
            &["python", "-c", &probe_script],
            &[("TESTVDB_DB_URL", &db_url)],
        ).await?;
        if !output.success {
            anyhow::bail!(
                "Independent Qdrant probe failed.\nSTDOUT:\n{}\nSTDERR:\n{}",
                output.stdout,
                output.stderr
            );
        }
        let result: Value = serde_json::from_str(output.stdout.trim())?;
        Ok(result)
    }

    fn summarize_findings(&self, probe_json: &ReviewResult) -> Option<(DefectType, Vec<String>)> {
        summarize_qdrant_independent_probe_from_value(probe_json)
    }
}

const QDRANT_REVIEW_PROBE_TEMPLATE: &str = r#"
import json
import requests

BASE_URL = "__DB_URL__"
collection_name = "test_collection"

requests.delete(f"{BASE_URL}/collections/{collection_name}")
create_payload = {
    "vectors": {"size": 4, "distance": "Cosine"}
}
create_resp = requests.put(f"{BASE_URL}/collections/{collection_name}", json=create_payload)

point = {
    "id": 1,
    "vector": [0.1, 0.2, 0.3, 0.4],
    "payload": {"city": "Berlin"}
}
upsert_resp = requests.put(f"{BASE_URL}/collections/{collection_name}/points", json={"points": [point]})

vector_resp = requests.post(
    f"{BASE_URL}/collections/{collection_name}/points/search",
    json={"vector": [0.1, 0.2, 0.3], "limit": 5, "offset": 0, "with_payload": True, "with_vector": False}
)
limit_resp = requests.post(
    f"{BASE_URL}/collections/{collection_name}/points/search",
    json={"vector": [0.1, 0.2, 0.3, 0.4], "limit": 0, "offset": 0, "with_payload": True, "with_vector": False}
)
offset_resp = requests.post(
    f"{BASE_URL}/collections/{collection_name}/points/search",
    json={"vector": [0.1, 0.2, 0.3, 0.4], "limit": 5, "offset": -1, "with_payload": True, "with_vector": False}
)
hnsw_ef_resp = requests.post(
    f"{BASE_URL}/collections/{collection_name}/points/search",
    json={"vector": [0.1, 0.2, 0.3, 0.4], "limit": 5, "offset": 0, "params": {"hnsw_ef": 0}, "with_payload": True, "with_vector": False}
)
score_threshold_high_resp = requests.post(
    f"{BASE_URL}/collections/{collection_name}/points/search",
    json={"vector": [0.1, 0.2, 0.3, 0.4], "limit": 5, "score_threshold": 2.0, "with_payload": True, "with_vector": False}
)
score_threshold_neg_resp = requests.post(
    f"{BASE_URL}/collections/{collection_name}/points/search",
    json={"vector": [0.1, 0.2, 0.3, 0.4], "limit": 5, "score_threshold": -0.5, "with_payload": True, "with_vector": False}
)

cc_size0_resp = requests.put(
    f"{BASE_URL}/collections/test_cc_size0",
    json={"vectors": {"size": 0, "distance": "Cosine"}}
)
cc_bad_metric_resp = requests.put(
    f"{BASE_URL}/collections/test_cc_bad_metric",
    json={"vectors": {"size": 4, "distance": "InvalidMetric"}}
)
cc_shard0_resp = requests.put(
    f"{BASE_URL}/collections/test_cc_shard0",
    json={"vectors": {"size": 4, "distance": "Cosine"}, "shard_number": 0}
)
requests.delete(f"{BASE_URL}/collections/test_cc_size0")
requests.delete(f"{BASE_URL}/collections/test_cc_bad_metric")
requests.delete(f"{BASE_URL}/collections/test_cc_shard0")

search_no_coll_resp = requests.post(
    f"{BASE_URL}/collections/nonexistent_coll_xyz/points/search",
    json={"vector": [0.1, 0.2, 0.3, 0.4], "limit": 5}
)

requests.delete(f"{BASE_URL}/collections/{collection_name}")

upsert_empty_resp = requests.put(
    f"{BASE_URL}/collections/{collection_name}/points",
    json={"points": []}
)
upsert_missing_id_resp = requests.put(
    f"{BASE_URL}/collections/{collection_name}/points",
    json={"points": [{"vector": [0.1, 0.2, 0.3, 0.4]}]}
)
upsert_no_vector_resp = requests.put(
    f"{BASE_URL}/collections/{collection_name}/points",
    json={"points": [{"id": 999}]}
)

print(json.dumps({
    "create_status": create_resp.status_code,
    "create_body": create_resp.text,
    "upsert_status": upsert_resp.status_code,
    "upsert_body": upsert_resp.text,
    "vector_status": vector_resp.status_code,
    "vector_body": vector_resp.text,
    "limit_status": limit_resp.status_code,
    "limit_body": limit_resp.text,
    "offset_status": offset_resp.status_code,
    "offset_body": offset_resp.text,
    "hnsw_ef_status": hnsw_ef_resp.status_code,
    "hnsw_ef_body": hnsw_ef_resp.text,
    "score_threshold_high_status": score_threshold_high_resp.status_code,
    "score_threshold_high_body": score_threshold_high_resp.text,
    "score_threshold_neg_status": score_threshold_neg_resp.status_code,
    "score_threshold_neg_body": score_threshold_neg_resp.text,
    "cc_size0_status": cc_size0_resp.status_code,
    "cc_size0_body": cc_size0_resp.text,
    "cc_bad_metric_status": cc_bad_metric_resp.status_code,
    "cc_bad_metric_body": cc_bad_metric_resp.text,
    "cc_shard0_status": cc_shard0_resp.status_code,
    "cc_shard0_body": cc_shard0_resp.text,
    "search_no_collection_status": search_no_coll_resp.status_code,
    "search_no_collection_body": search_no_coll_resp.text,
    "upsert_empty_status": upsert_empty_resp.status_code,
    "upsert_empty_body": upsert_empty_resp.text,
    "upsert_missing_id_status": upsert_missing_id_resp.status_code,
    "upsert_missing_id_body": upsert_missing_id_resp.text,
    "upsert_no_vector_status": upsert_no_vector_resp.status_code,
    "upsert_no_vector_body": upsert_no_vector_resp.text
}))
"#;

fn is_expected_validation_failure(status: u16) -> bool {
    status == 400 || status == 422
}

fn summarize_qdrant_independent_probe_from_value(probe_value: &Value) -> Option<(DefectType, Vec<String>)> {
    let mut illegal_success_issues: Vec<String> = Vec::new();
    let mut poor_diagnostics_issues: Vec<String> = Vec::new();

    let g = |key: &str| -> u16 {
        probe_value.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u16
    };
    let b = |key: &str| -> String {
        probe_value.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    };

    let vector_status = g("vector_status");
    let limit_status = g("limit_status");
    let offset_status = g("offset_status");
    let hnsw_ef_status = g("hnsw_ef_status");
    let cc_size0_status = g("cc_size0_status");
    let cc_bad_metric_status = g("cc_bad_metric_status");
    let cc_shard0_status = g("cc_shard0_status");
    let search_no_collection_status = g("search_no_collection_status");

    if vector_status == 200 {
        illegal_success_issues.push("vector length request succeeded despite documented dimension constraint".to_string());
    }
    if limit_status == 200 {
        illegal_success_issues.push("limit=0 request succeeded despite documented positive limit constraint".to_string());
    }
    if offset_status == 200 {
        illegal_success_issues.push("negative offset request succeeded despite documented offset constraint".to_string());
    }
    if hnsw_ef_status == 200 {
        illegal_success_issues.push("hnsw_ef=0 request succeeded despite documented positive hnsw_ef constraint".to_string());
    }
    let score_threshold_high_status = g("score_threshold_high_status");
    let score_threshold_neg_status = g("score_threshold_neg_status");
    if score_threshold_high_status == 200 {
        illegal_success_issues.push("score_threshold=2.0 request succeeded despite documented 0.0-1.0 range constraint".to_string());
    }
    if score_threshold_neg_status == 200 {
        illegal_success_issues.push("score_threshold=-0.5 request succeeded despite documented 0.0-1.0 range constraint".to_string());
    }
    if cc_size0_status == 200 {
        illegal_success_issues.push("create_collection with vectors.size=0 succeeded despite documented positive size constraint".to_string());
    }
    if cc_bad_metric_status == 200 {
        illegal_success_issues.push("create_collection with invalid distance metric succeeded despite documented enum constraint".to_string());
    }
    if cc_shard0_status == 200 {
        illegal_success_issues.push("create_collection with shard_number=0 succeeded despite documented >=1 constraint".to_string());
    }
    if search_no_collection_status == 200 {
        illegal_success_issues.push("search on non-existent collection returned 200 instead of error (STATE_VIOLATION)".to_string());
    }
    if g("upsert_empty_status") == 200 {
        illegal_success_issues.push("upsert with empty points array succeeded despite documented constraint".to_string());
    }
    if g("upsert_missing_id_status") == 200 {
        illegal_success_issues.push("upsert with missing id succeeded despite documented constraint".to_string());
    }
    if g("upsert_no_vector_status") == 200 {
        illegal_success_issues.push("upsert with missing vector succeeded despite documented constraint".to_string());
    }

    if !illegal_success_issues.is_empty() {
        return Some((DefectType::IllegalSuccess, illegal_success_issues));
    }

    let vector_body = b("vector_body").to_lowercase();
    if is_expected_validation_failure(vector_status) {
        if !["vector", "dim", "dimension", "size"].iter().any(|t| vector_body.contains(t)) {
            poor_diagnostics_issues.push("vector length error does not mention vector/dimension".to_string());
        }
    }
    let limit_body = b("limit_body").to_lowercase();
    if is_expected_validation_failure(limit_status) {
        if !["limit", "larger", "positive"].iter().any(|t| limit_body.contains(t)) {
            poor_diagnostics_issues.push("limit error does not mention limit constraint".to_string());
        }
    }
    let offset_body = b("offset_body").to_lowercase();
    if is_expected_validation_failure(offset_status) {
        if !offset_body.contains("offset") {
            poor_diagnostics_issues.push("offset error does not mention offset".to_string());
        }
    }
    let cc_size0_body = b("cc_size0_body").to_lowercase();
    if is_expected_validation_failure(cc_size0_status) {
        if !["size", "vector", "dimension"].iter().any(|t| cc_size0_body.contains(t)) {
            poor_diagnostics_issues.push("collection vectors.size=0 error does not mention size/vector".to_string());
        }
    }
    let cc_bad_metric_body = b("cc_bad_metric_body").to_lowercase();
    if is_expected_validation_failure(cc_bad_metric_status) {
        if !["distance", "metric", "cosine", "dot", "euclid", "manhattan"].iter().any(|t| cc_bad_metric_body.contains(t)) {
            poor_diagnostics_issues.push("collection bad distance error does not mention distance/metric".to_string());
        }
    }
    let cc_shard0_body = b("cc_shard0_body").to_lowercase();
    if is_expected_validation_failure(cc_shard0_status) {
        if !["shard", "number", "integer"].iter().any(|t| cc_shard0_body.contains(t)) {
            poor_diagnostics_issues.push("collection shard_number=0 error does not mention shard".to_string());
        }
    }
    let search_no_coll_body = b("search_no_collection_body").to_lowercase();
    if is_expected_validation_failure(search_no_collection_status) {
        if !["collection", "not found", "exist", "missing"].iter().any(|t| search_no_coll_body.contains(t)) {
            poor_diagnostics_issues.push("search on non-existent collection error does not mention collection".to_string());
        }
    }

    let upsert_empty_body = b("upsert_empty_body").to_lowercase();
    if is_expected_validation_failure(g("upsert_empty_status")) {
        if !["point", "empty", "array", "at least"].iter().any(|t| upsert_empty_body.contains(t)) {
            poor_diagnostics_issues.push("upsert empty points error does not mention 'point' or 'empty'".to_string());
        }
    }
    let upsert_missing_id_body = b("upsert_missing_id_body").to_lowercase();
    if is_expected_validation_failure(g("upsert_missing_id_status")) {
        if !["id", "required", "missing", "identifier"].iter().any(|t| upsert_missing_id_body.contains(t)) {
            poor_diagnostics_issues.push("upsert missing id error does not mention 'id' or 'required'".to_string());
        }
    }
    let upsert_no_vector_body = b("upsert_no_vector_body").to_lowercase();
    if is_expected_validation_failure(g("upsert_no_vector_status")) {
        if !["vector", "required", "missing"].iter().any(|t| upsert_no_vector_body.contains(t)) {
            poor_diagnostics_issues.push("upsert missing vector error does not mention 'vector' or 'required'".to_string());
        }
    }

    if !poor_diagnostics_issues.is_empty() {
        return Some((DefectType::PoorDiagnostics, poor_diagnostics_issues));
    }

    None
}

/// Legacy struct for test compatibility.
#[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
pub struct IndependentProbeResult {
    pub create_status: u16,
    pub create_body: String,
    pub upsert_status: u16,
    pub upsert_body: String,
    pub vector_status: u16,
    pub vector_body: String,
    pub limit_status: u16,
    pub limit_body: String,
    pub offset_status: u16,
    pub offset_body: String,
    pub hnsw_ef_status: u16,
    pub hnsw_ef_body: String,
    #[serde(default)]
    pub score_threshold_high_status: u16,
    #[serde(default)]
    pub score_threshold_high_body: String,
    #[serde(default)]
    pub score_threshold_neg_status: u16,
    #[serde(default)]
    pub score_threshold_neg_body: String,
    #[serde(default)]
    pub cc_size0_status: u16,
    #[serde(default)]
    pub cc_size0_body: String,
    #[serde(default)]
    pub cc_bad_metric_status: u16,
    #[serde(default)]
    pub cc_bad_metric_body: String,
    #[serde(default)]
    pub cc_shard0_status: u16,
    #[serde(default)]
    pub cc_shard0_body: String,
    #[serde(default)]
    pub search_no_collection_status: u16,
    #[serde(default)]
    pub search_no_collection_body: String,
    #[serde(default)]
    pub upsert_empty_status: u16,
    #[serde(default)]
    pub upsert_empty_body: String,
    #[serde(default)]
    pub upsert_missing_id_status: u16,
    #[serde(default)]
    pub upsert_missing_id_body: String,
    #[serde(default)]
    pub upsert_no_vector_status: u16,
    #[serde(default)]
    pub upsert_no_vector_body: String,
}

/// Legacy wrapper for test compatibility — delegates to summarize_qdrant_independent_probe_from_value.
pub fn summarize_qdrant_independent_probe(result: &IndependentProbeResult) -> Option<(DefectType, Vec<String>)> {
    let value = serde_json::to_value(result).ok()?;
    summarize_qdrant_independent_probe_from_value(&value)
}




pub fn build_qdrant_search_poor_diagnostics_mre(validated_issues: &[String]) -> String {
    let mut mre = String::from(r#"import requests, json

BASE = '{{TESTVDB_DB_URL}}'
collection_name = "test_poor_diag"

requests.delete(f"{BASE}/collections/{collection_name}")
create_resp = requests.put(
    f"{BASE}/collections/{collection_name}",
    json={"vectors": {"size": 4, "distance": "Cosine"}}
)
if create_resp.status_code not in (200, 201):
    print(f"Setup failed: {create_resp.status_code}")
    exit(1)

point = {"id": 1, "vector": [0.1, 0.2, 0.3, 0.4], "payload": {"city": "Berlin"}}
upsert_resp = requests.put(
    f"{BASE}/collections/{collection_name}/points",
    json={"points": [point]}
)

"#);

    for issue in validated_issues {
        if issue.contains("limit") {
            mre.push_str(&format!(r#"
# Test: {} — send limit=0
r = requests.post(
    f"{{BASE}}/collections/{{collection_name}}/points/search",
    json={{"vector": [0.1, 0.2, 0.3, 0.4], "limit": 0}}
)
print(f"limit=0 search returned status {{r.status_code}}, body preview: {{r.text[:300]}}")
if r.status_code == 400 or r.status_code == 422:
    if "limit" not in r.text.lower() and "positive" not in r.text.lower() and "larger" not in r.text.lower():
        print("[DEFECT: POOR_DIAGNOSTICS] limit error does not clearly mention the limit constraint")
    else:
        print("[PASS: LIMIT_DIAGNOSTICS] limit error message acceptable")
"#, issue.trim()));
        } else if issue.contains("offset") {
            mre.push_str(&format!(r#"
# Test: {} — send offset=-1
r = requests.post(
    f"{{BASE}}/collections/{{collection_name}}/points/search",
    json={{"vector": [0.1, 0.2, 0.3, 0.4], "limit": 5, "offset": -1}}
)
print(f"negative offset search returned status {{r.status_code}}, body preview: {{r.text[:300]}}")
if r.status_code == 400 or r.status_code == 422:
    if "offset" not in r.text.lower():
        print("[DEFECT: POOR_DIAGNOSTICS] offset error does not mention the offset parameter")
    else:
        print("[PASS: OFFSET_DIAGNOSTICS] offset error message acceptable")
"#, issue.trim()));
        } else {
            mre.push_str(&format!(r#"
# Test: {}
print(f"Test skipped — unsupported issue type: {}", issue)
"#, issue.trim(), issue.trim()));
        }
    }

    mre.push_str(r#"
requests.delete(f"{BASE}/collections/{collection_name}")
"#);

    mre
}
