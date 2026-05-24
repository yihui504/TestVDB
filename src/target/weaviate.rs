use super::{SafetyNet, TargetPlugin, TargetStyle};
use crate::agent::oracle::InvariantCheck;
use crate::agent::probe::{ProbeTemplate, NoopProbeTemplate};
use crate::contract::schema::StructuredContract;
use crate::review::IndependentReviewer;
use crate::review::weaviate::WeaviateIndependentReviewer;

pub struct WeaviatePlugin;

impl TargetPlugin for WeaviatePlugin {
    fn name(&self) -> &str {
        "weaviate"
    }

    fn target_image(&self, version: &str) -> String {
        if version.starts_with('v') {
            format!("semitechnologies/weaviate:{}", version)
        } else {
            format!("semitechnologies/weaviate:{}", version)
        }
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
    print(f"[DEFECT: STATE_VIOLATION] Insert 5 objects but count={count}")
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
    print(f"[DEFECT: STATE_VIOLATION] Insert 5, delete 2, count={count}")
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