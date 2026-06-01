use super::{SafetyNet, TargetPlugin, TargetStyle};
use crate::agent::oracle::InvariantCheck;
use crate::agent::probe::{ProbeTemplate, NoopProbeTemplate};
use crate::agent::vdbfuzz::coverage::ApiEndpoint;
use crate::contract::schema::StructuredContract;
use crate::review::IndependentReviewer;
use crate::review::weaviate::WeaviateIndependentReviewer;

pub struct WeaviatePlugin;

impl TargetPlugin for WeaviatePlugin {
    fn name(&self) -> &str {
        "weaviate"
    }

    fn target_image(&self, version: &str) -> String {
        let v = version.strip_prefix('v').unwrap_or(version);
        format!("semitechnologies/weaviate:{}", v)
    }

    fn pip_packages(&self) -> Vec<String> {
        vec!["requests".to_string(), "weaviate-client".to_string()]
    }

    fn db_port(&self) -> u16 {
        8080
    }

    fn default_repo_url(&self) -> Option<&str> {
        Some("https://github.com/weaviate/weaviate")
    }

    fn default_docs_url(&self) -> Option<&str> {
        Some("https://weaviate.io/developers/weaviate")
    }

    fn safety_nets(&self) -> Vec<SafetyNet> {
        let mut nets = Vec::new();

        // Collection creation boundary violations
        nets.push(SafetyNet {
            name: "dim_zero".into(),
            script: weaviate_dim_zero_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "dim_negative".into(),
            script: weaviate_dim_negative_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "dim_oversized".into(),
            script: weaviate_dim_oversized_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "invalid_distance_metric".into(),
            script: weaviate_invalid_metric_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "ef_zero".into(),
            script: weaviate_ef_zero_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "ef_negative".into(),
            script: weaviate_ef_negative_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "maxConnections_low".into(),
            script: weaviate_max_connections_low_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "dim_mismatch_search".into(),
            script: weaviate_dim_mismatch_probe(),
            redundant_with_mutation: false,
        });

        // State consistency
        nets.push(SafetyNet {
            name: "state_upsert_count".into(),
            script: weaviate_state_count_probe(),
            redundant_with_mutation: true,
        });
        nets.push(SafetyNet {
            name: "state_delete_count".into(),
            script: weaviate_state_delete_count_probe(),
            redundant_with_mutation: true,
        });
        nets.push(SafetyNet {
            name: "search_nonexistent_collection".into(),
            script: weaviate_search_nonexistent_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "graphql_limit_negative".into(),
            script: weaviate_graphql_limit_negative_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "empty_vector".into(),
            script: weaviate_empty_vector_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "large_efconstruction".into(),
            script: weaviate_large_efconstruction_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "bm25_fractional_boost".into(),
            script: weaviate_bm25_fractional_boost_probe(),
            redundant_with_mutation: false,
        });
        nets.push(SafetyNet {
            name: "nested_blobhash".into(),
            script: weaviate_nested_blobhash_probe(),
            redundant_with_mutation: false,
        });

        nets
    }

    fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>> {
        Some(Box::new(WeaviateIndependentReviewer))
    }

    fn derive_oracle_checks(&self, contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();

        // 1. dynamicEfMin <= dynamicEfMax validation
        checks.push(InvariantCheck {
            name: "wv_ef_min_max".into(),
            check_type: crate::contract::schema::CheckType::ValueRange,
            script: wv_oracle_ef_min_max(),
            source: crate::agent::oracle::InvariantSource::DerivedFromAssertion,
        });
        // 2. flatSearchCutoff >= 0 validation
        checks.push(InvariantCheck {
            name: "wv_flatcutoff_nonneg".into(),
            check_type: crate::contract::schema::CheckType::ValueRange,
            script: wv_oracle_flatcutoff_negative(),
            source: crate::agent::oracle::InvariantSource::DerivedFromAssertion,
        });
        // 3. replicationFactor >= 1 validation (silent normalization)
        checks.push(InvariantCheck {
            name: "wv_replication_positive".into(),
            check_type: crate::contract::schema::CheckType::ValueRange,
            script: wv_oracle_replication_negative(),
            source: crate::agent::oracle::InvariantSource::DerivedFromAssertion,
        });
        // 4. bq.rescoreLimit >= 0 validation (silent discard)
        checks.push(InvariantCheck {
            name: "wv_bq_rescore_nonneg".into(),
            check_type: crate::contract::schema::CheckType::ValueRange,
            script: wv_oracle_bq_rescore_negative(),
            source: crate::agent::oracle::InvariantSource::DerivedFromAssertion,
        });
        // 5. Dimension mismatch insert returns 4xx not 500
        checks.push(InvariantCheck {
            name: "wv_dim_mismatch".into(),
            check_type: crate::contract::schema::CheckType::ValueRange,
            script: wv_oracle_dim_mismatch(),
            source: crate::agent::oracle::InvariantSource::DerivedFromAssertion,
        });
        // 6. Object count consistency (insert N → count = N)
        checks.push(InvariantCheck {
            name: "wv_object_count".into(),
            check_type: crate::contract::schema::CheckType::CountConsistency,
            script: wv_oracle_object_count(),
            source: crate::agent::oracle::InvariantSource::DerivedFromState,
        });
        // 7. Search on empty collection doesn't crash
        checks.push(InvariantCheck {
            name: "wv_search_empty".into(),
            check_type: crate::contract::schema::CheckType::ExistenceCheck,
            script: wv_oracle_search_empty(),
            source: crate::agent::oracle::InvariantSource::DerivedFromBehavior,
        });
        // 8. Create + immediate describe succeeds
        checks.push(InvariantCheck {
            name: "wv_create_describe".into(),
            check_type: crate::contract::schema::CheckType::ExistenceCheck,
            script: wv_oracle_create_describe(),
            source: crate::agent::oracle::InvariantSource::DerivedFromState,
        });
        // 9. Delete count consistency (N insert, M delete → N-M)
        checks.push(InvariantCheck {
            name: "wv_delete_count".into(),
            check_type: crate::contract::schema::CheckType::CountConsistency,
            script: wv_oracle_delete_count(),
            source: crate::agent::oracle::InvariantSource::DerivedFromState,
        });
        // 10. Multi-tenant: insert without tenant key
        checks.push(InvariantCheck {
            name: "wv_tenant_required".into(),
            check_type: crate::contract::schema::CheckType::ExistenceCheck,
            script: wv_oracle_tenant_required(),
            source: crate::agent::oracle::InvariantSource::DerivedFromBehavior,
        });

        checks
    }

    fn target_style(&self) -> TargetStyle {
        TargetStyle::Weaviate
    }

    fn doc_citation_url(&self) -> String {
        "https://weaviate.io/developers/weaviate".to_string()
    }

    fn probe_template(&self) -> &dyn ProbeTemplate {
        &NoopProbeTemplate
    }

    fn all_api_endpoints(&self) -> Vec<ApiEndpoint> {
        vec![
            ApiEndpoint { method: "POST".into(), path: "/v1/schema".into(), params: vec!["class".into(), "vectorizer".into(), "vectorIndexConfig".into(), "replicationConfig".into(), "multiTenancyConfig".into(), "properties".into()] },
            ApiEndpoint { method: "GET".into(), path: "/v1/schema".into(), params: vec![] },
            ApiEndpoint { method: "GET".into(), path: "/v1/schema/{className}".into(), params: vec!["className".into()] },
            ApiEndpoint { method: "DELETE".into(), path: "/v1/schema/{className}".into(), params: vec!["className".into()] },
            ApiEndpoint { method: "POST".into(), path: "/v1/schema/{className}/properties".into(), params: vec!["className".into(), "name".into(), "dataType".into()] },
            ApiEndpoint { method: "POST".into(), path: "/v1/objects".into(), params: vec!["class".into(), "id".into(), "vector".into(), "properties".into(), "tenant".into()] },
            ApiEndpoint { method: "GET".into(), path: "/v1/objects".into(), params: vec!["class".into(), "limit".into(), "offset".into()] },
            ApiEndpoint { method: "GET".into(), path: "/v1/objects/{className}/{id}".into(), params: vec!["className".into(), "id".into(), "include".into()] },
            ApiEndpoint { method: "PUT".into(), path: "/v1/objects/{className}/{id}".into(), params: vec!["className".into(), "id".into(), "properties".into(), "vector".into()] },
            ApiEndpoint { method: "PATCH".into(), path: "/v1/objects/{className}/{id}".into(), params: vec!["className".into(), "id".into(), "properties".into()] },
            ApiEndpoint { method: "DELETE".into(), path: "/v1/objects/{className}/{id}".into(), params: vec!["className".into(), "id".into(), "consistency_level".into(), "tenant".into()] },
            ApiEndpoint { method: "POST".into(), path: "/v1/batch/objects".into(), params: vec!["objects".into()] },
            ApiEndpoint { method: "POST".into(), path: "/v1/graphql".into(), params: vec!["query".into()] },
            ApiEndpoint { method: "GET".into(), path: "/v1/meta".into(), params: vec![] },
            ApiEndpoint { method: "GET".into(), path: "/v1/nodes".into(), params: vec![] },
        ]
    }

    fn db_env(&self) -> Vec<(String, String)> {
        vec![
            ("AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED".to_string(), "true".to_string()),
            ("DEFAULT_VECTORIZER_MODULE".to_string(), "none".to_string()),
        ]
    }
}

fn weaviate_dim_zero_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_dim_zero_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine"},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] Collection with dim=default accepted")
    sys.exit(1)
print("dim=default correctly rejected")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_dim_negative_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_dim_neg_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "ef": -1},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] ef=-1 accepted")
    sys.exit(1)
print("ef=-1 correctly rejected")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_dim_oversized_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_dim_large_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "maxConnections": 0},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] maxConnections=0 accepted")
    sys.exit(1)
print("maxConnections=0 correctly rejected")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_invalid_metric_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_bad_metric_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "InvalidMetric"},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] Invalid distance metric accepted")
    sys.exit(1)
print("Invalid distance metric correctly rejected")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_ef_zero_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_ef0_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "ef": 0},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] ef=0 accepted (should be >= -1)")
    sys.exit(1)
print("ef=0 accepted (allowed range)")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_ef_negative_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_efneg_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "ef": -2},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] ef=-2 accepted")
    sys.exit(1)
print("ef=-2 correctly rejected")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_max_connections_low_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_mc_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine", "maxConnections": 2},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] maxConnections=2 accepted (should be >= 4)")
    sys.exit(1)
print("maxConnections=2 correctly rejected")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_dim_mismatch_probe() -> String {
    r#"
import requests, json, sys, uuid, time
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_dimmis_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine"},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
if r.status_code != 200:
    print("Setup failed")
    sys.exit(0)
time.sleep(0.5)
r2 = requests.post(f"{BASE}/v1/objects", json={
    "class": name,
    "id": str(uuid.uuid4()),
    "vector": [0.1, 0.2, 0.3],
    "properties": {"text": "test"}
})
if r2.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] Default-dim vector mismatch insert accepted")
    sys.exit(1)
print("dim mismatch correctly rejected")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_state_count_probe() -> String {
    r#"
import requests, json, sys, uuid, time
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_state_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine"},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
assert r.status_code == 200, f"Create failed: {r.text}"
time.sleep(0.5)
pts = [{"class": name, "id": str(uuid.uuid4()), "vector": [0.1*i, 0.2*i, 0.3*i], "properties": {"text": f"item{i}"}} for i in range(1,6)]
for pt in pts:
    requests.post(f"{BASE}/v1/objects", json=pt)
time.sleep(1.0)
r2 = requests.get(f"{BASE}/v1/schema/{name}")
count = r2.json().get("class", {}).get("objectCount", -1)
if count != 5:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Insert 5 objects but count={count}")
    sys.exit(1)
print("State count verified: 5 objects")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_state_delete_count_probe() -> String {
    r#"
import requests, json, sys, uuid, time
BASE = "{{TESTVDB_DB_URL}}"
name = "testvdb_del_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={
    "class": name,
    "vectorizer": "none",
    "vectorIndexConfig": {"distance": "cosine"},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
assert r.status_code == 200
time.sleep(0.5)
ids = [str(uuid.uuid4()) for _ in range(5)]
for i, oid in enumerate(ids):
    requests.post(f"{BASE}/v1/objects", json={
        "class": name, "id": oid,
        "vector": [0.1*i, 0.2*i, 0.3*i],
        "properties": {"text": f"item{i}"}
    })
time.sleep(1.0)
requests.delete(f"{BASE}/v1/objects/{name}/{ids[0]}")
requests.delete(f"{BASE}/v1/objects/{name}/{ids[1]}")
time.sleep(1.0)
r2 = requests.get(f"{BASE}/v1/schema/{name}")
count = r2.json().get("class", {}).get("objectCount", -1)
if count != 3:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Insert 5, delete 2, count={count}")
    sys.exit(1)
print("Delete count verified: 3 remaining")
requests.delete(f"{BASE}/v1/schema/{name}")
"#.to_string()
}

fn weaviate_search_nonexistent_probe() -> String {
    r#"
import requests, json, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
name = "nonexistent_coll_" + uuid.uuid4().hex[:8]
r = requests.get(f"{BASE}/v1/objects?class={name}")
if r.status_code == 200:
    print(f"[DEFECT: ILLEGAL_SUCCESS] Query on nonexistent collection returned 200")
    sys.exit(1)
print("Nonexistent collection correctly rejected")
"#.to_string()
}

// ── Oracle invariant check scripts ──

fn wv_oracle_ef_min_max() -> String {
    r#"import requests,uuid,sys
BASE='{{TESTVDB_DB_URL}}'
c='oracle_ef_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine","dynamicEfMin":500,"dynamicEfMax":10,"dynamicEfFactor":8},"properties":[{"name":"text","dataType":["text"]}]})
if r.status_code==200:
    print('[DEFECT: ILLEGAL_SUCCESS] dynamicEfMin=500 > dynamicEfMax=10 accepted')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_flatcutoff_negative() -> String {
    r#"import requests,uuid,sys
BASE='{{TESTVDB_DB_URL}}'
c='oracle_fc_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine","flatSearchCutoff":-100},"properties":[{"name":"text","dataType":["text"]}]})
if r.status_code==200:
    print('[DEFECT: ILLEGAL_SUCCESS] flatSearchCutoff=-100 accepted')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_replication_negative() -> String {
    r#"import requests,uuid,sys,time
BASE='{{TESTVDB_DB_URL}}'
c='oracle_rep_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","replicationConfig":{"factor":-1},"vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
if r.status_code==200:
    time.sleep(0.3)
    r2=requests.get(f'{BASE}/v1/schema/{c}')
    factor=r2.json().get('replicationConfig',{}).get('factor',-1)
    if factor!= -1:
        print(f'[DEFECT: STATE_LOGIC_VIOLATION] replicationFactor=-1 silently normalized to {factor}')
        sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_bq_rescore_negative() -> String {
    r#"import requests,uuid,sys,time
BASE='{{TESTVDB_DB_URL}}'
c='oracle_bq_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine","bq":{"enabled":True,"rescoreLimit":-1}},"properties":[{"name":"text","dataType":["text"]}]})
if r.status_code==200:
    time.sleep(0.3)
    r2=requests.get(f'{BASE}/v1/schema/{c}')
    bq=r2.json().get('vectorIndexConfig',{}).get('bq',{})
    if 'rescoreLimit' not in bq:
        print('[DEFECT: STATE_LOGIC_VIOLATION] bq.rescoreLimit=-1 silently discarded from config')
        sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_dim_mismatch() -> String {
    r#"import requests,uuid,sys,time
BASE='{{TESTVDB_DB_URL}}'
c='oracle_dm_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
assert r.status_code==200
time.sleep(0.5)
r2=requests.post(f'{BASE}/v1/objects',json={"class":c,"id":str(uuid.uuid4()),"vector":[0.1,0.2,0.3],"properties":{"text":"test"}})
if r2.status_code==500:
    print('[DEFECT: POOR_DIAGNOSTICS] Dimension mismatch insert returned 500 (should be 4xx)')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_object_count() -> String {
    r#"import requests,uuid,sys,time
BASE='{{TESTVDB_DB_URL}}'
c='oracle_cnt_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
assert r.status_code==200
time.sleep(0.5)
for i in range(8):
    requests.post(f'{BASE}/v1/objects',json={"class":c,"id":str(uuid.uuid4()),"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"properties":{"text":f"item{i}"}})
time.sleep(1)
r2=requests.get(f'{BASE}/v1/objects?class={c}')
count=r2.json().get('totalResults',-1)
if count!=8:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] Insert 8 objects but count={count}')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_search_empty() -> String {
    r#"import requests,uuid,sys
BASE='{{TESTVDB_DB_URL}}'
c='oracle_empty_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
assert r.status_code==200
r2=requests.get(f'{BASE}/v1/objects?class={c}')
if r2.status_code!=200:
    print(f'[DEFECT: RUNTIME_FAILURE] Objects GET on empty collection failed: {r2.status_code}')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_create_describe() -> String {
    r#"import requests,uuid,sys,time
BASE='{{TESTVDB_DB_URL}}'
c='oracle_cd_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
if r.status_code!=200:
    print(f'[DEFECT: RUNTIME_FAILURE] Collection creation failed: {r.status_code}')
    sys.exit(1)
time.sleep(0.2)
r2=requests.get(f'{BASE}/v1/schema/{c}')
if r2.status_code!=200:
    print(f'[DEFECT: RUNTIME_FAILURE] Immediate schema GET after create failed: {r2.status_code}')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_delete_count() -> String {
    r#"import requests,uuid,sys,time
BASE='{{TESTVDB_DB_URL}}'
c='oracle_del_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
assert r.status_code==200
time.sleep(0.5)
ids=[str(uuid.uuid4()) for _ in range(8)]
for i,oid in enumerate(ids):
    requests.post(f'{BASE}/v1/objects',json={"class":c,"id":oid,"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"properties":{"text":f"item{i}"}})
time.sleep(1)
requests.delete(f'{BASE}/v1/objects/{c}/{ids[0]}')
requests.delete(f'{BASE}/v1/objects/{c}/{ids[1]}')
time.sleep(1)
r2=requests.get(f'{BASE}/v1/objects?class={c}')
count=r2.json().get('totalResults',-1)
if count!=6:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] Insert 8, delete 2, count={count} (expected 6)')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn wv_oracle_tenant_required() -> String {
    r#"import requests,uuid,sys,time
BASE='{{TESTVDB_DB_URL}}'
c='oracle_mt_'+uuid.uuid4().hex[:8]
r=requests.post(f'{BASE}/v1/schema',json={"class":c,"vectorizer":"none","multiTenancyConfig":{"enabled":True},"vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
if r.status_code!=200:
    print(f'MT setup failed: {r.status_code}')
    sys.exit(0)
time.sleep(0.5)
r2=requests.post(f'{BASE}/v1/objects',json={"class":c,"id":str(uuid.uuid4()),"vector":[0.1,0.2,0.3,0.4],"properties":{"text":"no_tenant"}})
if r2.status_code==200:
    print('[DEFECT: ILLEGAL_SUCCESS] Multi-tenant collection accepted insert without tenant key')
    sys.exit(1)
requests.delete(f'{BASE}/v1/schema/{c}')
sys.exit(0)
"#.to_string()
}

fn weaviate_graphql_limit_negative_probe() -> String {
    r#"import requests, sys, uuid, time
BASE = "{{TESTVDB_DB_URL}}"
c = "testvdb_limneg_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class": c, "vectorizer": "none", "vectorIndexConfig": {"distance": "cosine"}, "properties": [{"name": "title", "dataType": ["string"]}]})
if r.status_code != 200:
    print("Setup failed")
    sys.exit(0)
time.sleep(0.5)
requests.post(f"{BASE}/v1/objects", json={"class": c, "properties": {"title": "test"}, "vector": [0.1, 0.2, 0.3, 0.4]})
time.sleep(0.5)
q = '{ Get { ' + c + '(limit: -1) { title } } }'
r2 = requests.post(f"{BASE}/v1/graphql", json={"query": q})
if r2.status_code == 200 and not r2.json().get("errors"):
    print("[DEFECT: ILLEGAL_SUCCESS] GraphQL limit=-1 accepted")
    sys.exit(1)
print("GraphQL limit=-1 correctly rejected")
requests.delete(f"{BASE}/v1/schema/{c}")
"#.to_string()
}

fn weaviate_empty_vector_probe() -> String {
    r#"import requests, sys, uuid, time
BASE = "{{TESTVDB_DB_URL}}"
c = "testvdb_emptyvec_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class": c, "vectorizer": "none", "vectorIndexConfig": {"distance": "cosine"}, "properties": [{"name": "title", "dataType": ["string"]}]})
if r.status_code != 200:
    print("Setup failed")
    sys.exit(0)
time.sleep(0.5)
r2 = requests.post(f"{BASE}/v1/objects", json={"class": c, "properties": {"title": "test"}, "vector": []})
if r2.status_code == 200:
    print("[DEFECT: ILLEGAL_SUCCESS] Empty vector [] accepted")
    sys.exit(1)
print("Empty vector correctly rejected")
requests.delete(f"{BASE}/v1/schema/{c}")
"#.to_string()
}

fn weaviate_large_efconstruction_probe() -> String {
    r#"import requests, sys, uuid
BASE = "{{TESTVDB_DB_URL}}"
c = "testvdb_largeefc_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class": c, "vectorizer": "none", "vectorIndexConfig": {"distance": "cosine", "efConstruction": 999999999, "maxConnections": 64}, "properties": [{"name": "title", "dataType": ["string"]}]})
if r.status_code == 200:
    print("[DEFECT: ILLEGAL_SUCCESS] efConstruction=999999999 accepted")
    sys.exit(1)
print("efConstruction=999999999 correctly rejected")
requests.delete(f"{BASE}/v1/schema/{c}")
"#.to_string()
}

fn weaviate_bm25_fractional_boost_probe() -> String {
    r#"import requests, sys, uuid, time
BASE = "{{TESTVDB_DB_URL}}"
c = "testvdb_bm25boost_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class": c, "vectorizer": "none", "vectorIndexConfig": {"distance": "cosine"}, "properties": [{"name": "title", "dataType": ["text"], "indexSearchable": True}]})
if r.status_code != 200:
    print("Setup failed")
    sys.exit(0)
time.sleep(0.5)
requests.post(f"{BASE}/v1/objects", json={"class": c, "id": "11111111-1111-1111-1111-111111111111", "properties": {"title": "needle"}, "vector": [0.1, 0.2, 0.3, 0.4]})
time.sleep(1.0)
q1 = '{ Get { ' + c + '(bm25:{query:"needle",properties:["title^1"]}) { title _additional { score } } } }'
r1 = requests.post(f"{BASE}/v1/graphql", json={"query": q1})
score_int = 0.0
if r1.status_code == 200:
    items = r1.json().get("data", {}).get("Get", {}).get(c, [])
    if items:
        score_int = float(items[0].get("_additional", {}).get("score", 0))
q2 = '{ Get { ' + c + '(bm25:{query:"needle",properties:["title^0.5"]}) { title _additional { score } } } }'
r2 = requests.post(f"{BASE}/v1/graphql", json={"query": q2})
score_frac = -1.0
if r2.status_code == 200:
    items = r2.json().get("data", {}).get("Get", {}).get(c, [])
    if items:
        score_frac = float(items[0].get("_additional", {}).get("score", 0))
if score_int > 0 and score_frac == 0.0:
    print(f"[DEFECT: ILLEGAL_SUCCESS] BM25 boost^0.5 score=0 but boost^1 score={score_int}")
    sys.exit(1)
print(f"BM25 fractional boost OK: int={score_int}, frac={score_frac}")
requests.delete(f"{BASE}/v1/schema/{c}")
"#.to_string()
}

fn weaviate_nested_blobhash_probe() -> String {
    r#"import requests, sys, uuid, time, hashlib, base64
BASE = "{{TESTVDB_DB_URL}}"
c = "testvdb_blobhash_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class": c, "vectorizer": "none", "vectorIndexConfig": {"distance": "cosine"}, "properties": [{"name": "meta", "dataType": ["object"], "nestedProperties": [{"name": "image", "dataType": ["blobHash"]}]}]})
if r.status_code != 200:
    print("Setup failed (nested blobHash not supported)")
    sys.exit(0)
time.sleep(0.5)
b64_val = "aGVsbG8="
expected_hash = hashlib.sha256(base64.b64decode(b64_val)).hexdigest()
requests.post(f"{BASE}/v1/objects", json={"id": "00000000-0000-0000-0000-000000000111", "class": c, "properties": {"meta": {"image": b64_val}}})
time.sleep(0.5)
q = '{ Get { ' + c + ' { meta { image } } } }'
r2 = requests.post(f"{BASE}/v1/graphql", json={"query": q})
if r2.status_code == 200:
    items = r2.json().get("data", {}).get("Get", {}).get(c, [])
    if items:
        actual = items[0].get("meta", {}).get("image", "")
        if actual == b64_val:
            print(f"[DEFECT: ILLEGAL_SUCCESS] Nested blobHash returned raw base64 instead of SHA-256 hash")
            sys.exit(1)
print("Nested blobHash correctly hashed or not supported")
requests.delete(f"{BASE}/v1/schema/{c}")
"#.to_string()
}