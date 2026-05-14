use super::{SafetyNet, TargetPlugin};
use crate::agent::oracle::{InvariantCheck, InvariantSource};
use crate::agent::probe::{
    classify_endpoint_type, count_consistency_check, create_probe, delete_probe,
    duplicate_collection_check, empty_vector_search_check, format_boundary,
    inf_vector_search_check, invalid_distance_check, is_search_param, nan_vector_check,
    recommend_probe, scroll_probe, search_nonexistent_collection, search_params_probe,
    search_probe, search_string_probe, upsert_inf_vector_check, upsert_nan_vector_check,
    upsert_probe, EndpointType, SimpleSafetyNet,
};
use crate::contract::schema::{BehaviorCategory, CheckType, StructuredContract};
use crate::review::qdrant::QdrantIndependentReviewer;
use crate::review::IndependentReviewer;
use std::collections::HashSet;

pub struct QdrantPlugin;

impl TargetPlugin for QdrantPlugin {
    fn name(&self) -> &str {
        "qdrant"
    }

    fn target_image(&self, version: &str) -> String {
        if version.starts_with('v') {
            format!("qdrant/qdrant:{}", version)
        } else {
            format!("qdrant/qdrant:v{}", version)
        }
    }

    fn pip_packages(&self) -> Vec<String> {
        vec!["qdrant-client".to_string(), "httpx".to_string(), "requests".to_string()]
    }

    fn db_port(&self) -> u16 {
        6333
    }

    fn safety_nets(&self) -> Vec<SafetyNet> {
        let boundary_nets: Vec<SafetyNet> = vec![
            SimpleSafetyNet { name: "hnsw_ef=0".into(), endpoint_type: EndpointType::Search, param: "hnsw_ef".into(), value: "0".into(), label: "hnsw_ef=0".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "negative_limit".into(), endpoint_type: EndpointType::Search, param: "limit".into(), value: "-1".into(), label: "negative limit".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "oversampling=0".into(), endpoint_type: EndpointType::Search, param: "oversampling".into(), value: "0".into(), label: "oversampling=0".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "replication_factor=0".into(), endpoint_type: EndpointType::Create, param: "replication_factor".into(), value: "0".into(), label: "replication_factor=0".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "write_consistency_factor=0".into(), endpoint_type: EndpointType::Create, param: "write_consistency_factor".into(), value: "0".into(), label: "write_consistency_factor=0".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "score_threshold_negative".into(), endpoint_type: EndpointType::Search, param: "score_threshold".into(), value: "-0.5".into(), label: "negative score_threshold".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "score_threshold_above_one".into(), endpoint_type: EndpointType::Search, param: "score_threshold".into(), value: "2.0".into(), label: "score_threshold=2.0 (above 1.0)".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "create_collection_size_zero".into(), endpoint_type: EndpointType::Create, param: "size".into(), value: "0".into(), label: "vectors.size=0".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "create_collection_negative_size".into(), endpoint_type: EndpointType::Create, param: "size".into(), value: "-1".into(), label: "vectors.size=-1".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "shard_number_zero".into(), endpoint_type: EndpointType::Create, param: "shard_number".into(), value: "0".into(), label: "shard_number=0".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "shard_number_negative".into(), endpoint_type: EndpointType::Create, param: "shard_number".into(), value: "-1".into(), label: "shard_number=-1".into(), redundant_with_mutation: false },
            SimpleSafetyNet { name: "limit_float_type".into(), endpoint_type: EndpointType::Search, param: "limit".into(), value: "3.5".into(), label: "float limit".into(), redundant_with_mutation: false },
        ].into_iter().map(|s| s.to_safety_net()).collect();

        let special_nets = vec![
            SafetyNet { name: "nan_vector_search".into(), script: nan_vector_check(), ..Default::default() },
            SafetyNet { name: "inf_vector_search".into(), script: inf_vector_search_check(), ..Default::default() },
            SafetyNet { name: "empty_vector_search".into(), script: empty_vector_search_check(), ..Default::default() },
            SafetyNet { name: "upsert_nan_vector".into(), script: upsert_nan_vector_check(), ..Default::default() },
            SafetyNet { name: "upsert_infinity_vector".into(), script: upsert_inf_vector_check(), ..Default::default() },
            SafetyNet { name: "search_nonexistent_collection".into(), script: search_nonexistent_collection(), ..Default::default() },
            SafetyNet { name: "duplicate_collection".into(), script: duplicate_collection_check(), ..Default::default() },
            SafetyNet { name: "invalid_distance".into(), script: invalid_distance_check(), ..Default::default() },
        ];

        let state_nets = vec![
            SafetyNet {
                name: "upsert_count_consistency".into(),
                script: count_consistency_check(),
                ..Default::default()
            },
            SafetyNet {
                name: "delete_count_consistency".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_del_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(5)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points": [1, 2]})
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result', {}).get('points_count', -1)
if count != 3: print(f'[DEFECT: STATE_LOGIC_VIOLATION] delete 2 of 5 points but count={count}'); sys.exit(1)
else: print(f'count consistent after delete: {count}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "score_threshold_filtering".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_thresh_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i]} for i in range(10)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5],"limit":10,"score_threshold":0.5})
results = r.json().get('result', [])
for h in results:
    if h['score'] < 0.5: print(f'[DEFECT: STATE_LOGIC_VIOLATION] score_threshold=0.5 but got score={h["score"]}'); sys.exit(1)
print(f'score_threshold filtering correct: {len(results)} results above 0.5'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "upsert_idempotency".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_idem_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
time.sleep(0.3)
count1 = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('points_count',-1)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.5,0.6,0.7,0.8]}]})
time.sleep(0.3)
count2 = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('points_count',-1)
if count2 != count1: print(f'[DEFECT: STATE_LOGIC_VIOLATION] upsert same id not idempotent: {count1}->{count2}'); sys.exit(1)
else: print(f'upsert idempotent: count stayed {count1}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "recommend_no_positive".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_recnopos_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i]} for i in range(5)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/recommend', json={"positive": [], "negative": [], "limit": 5})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] recommend with empty positive and negative accepted'); sys.exit(1)
else: print(f'recommend with empty pos/neg properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "upsert_wrong_dimension".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_wrongdim_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r_wait = requests.put(f'{BASE}/collections/{c}/points?wait=true', json={"points":[{"id":1,"vector":[0.1,0.2,0.3]}]})
r_nowait = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":2,"vector":[0.1,0.2,0.3]}]})
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result',{}).get('points_count',-1)
if r_wait.status_code == 400 and r_nowait.status_code == 200 and count == 0:
    print(f'[DEFECT: POOR_DIAGNOSTICS] wait=true correctly rejects (400) but wait=false returns 200+acknowledged while silently discarding data (count={count})')
    sys.exit(1)
elif r_wait.status_code == 200 and r_nowait.status_code == 200:
    print(f'[DEFECT: ILLEGAL_SUCCESS] both wait=true and wait=false accepted wrong dimension')
    sys.exit(1)
else:
    print(f'wrong dimension properly handled: wait={r_wait.status_code} nowait={r_nowait.status_code} count={count}')
    sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "batch_upsert_partial_failure".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_batchpartial_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id":1,"vector":[0.1,0.2,0.3,0.4]}, {"id":2,"vector":[0.5,0.6]}, {"id":3,"vector":[0.7,0.8,0.9,1.0]}]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result', {}).get('points_count', -1)
if count == 3: print(f'[DEFECT: STATE_LOGIC_VIOLATION] batch with invalid vector fully accepted (count=3)'); sys.exit(1)
else: print(f'batch upsert with mixed valid/invalid: count={count} (acceptable)'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "payload_filter_consistency".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_payloadfilter_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i], "payload": {"color": "red" if i%2==0 else "blue"}} for i in range(10)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5], "limit":10, "filter":{"must":[{"key":"color","match":{"value":"red"}}]}})
results = r.json().get('result', [])
for h in results:
    if h.get('payload', {}).get('color') != 'red':
        print(f'[DEFECT: STATE_LOGIC_VIOLATION] filter color=red returned point with color={h.get("payload",{}).get("color")}'); sys.exit(1)
print(f'payload filter correct: {len(results)} results all with color=red'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "collection_info_consistency".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_infocons_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
info1 = requests.get(f'{BASE}/collections/{c}').json()
status1 = info1.get('result', {}).get('status', '')
if status1 != 'green': print(f'[DEFECT: STATE_LOGIC_VIOLATION] empty collection status={status1}'); sys.exit(1)
points = [{"id": i+1, "vector": [0.1*i, 0.2*i, 0.3*i, 0.4*i]} for i in range(5)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
time.sleep(0.5)
info2 = requests.get(f'{BASE}/collections/{c}').json()
count2 = info2.get('result', {}).get('points_count', -1)
if count2 != 5: print(f'[DEFECT: STATE_LOGIC_VIOLATION] after upsert 5 points, count={count2}'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points": [1, 2, 3]})
time.sleep(0.5)
info3 = requests.get(f'{BASE}/collections/{c}').json()
count3 = info3.get('result', {}).get('points_count', -1)
if count3 != 2: print(f'[DEFECT: STATE_LOGIC_VIOLATION] after delete 3 of 5, count={count3}'); sys.exit(1)
print(f'collection info consistent: 0->5->2 points'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "search_descending_scores".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_descscores_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i]} for i in range(10)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5], "limit":10})
results = r.json().get('result', [])
prev_score = 2.0
for h in results:
    if h['score'] > prev_score + 0.001:
        print(f'[DEFECT: STATE_LOGIC_VIOLATION] scores not descending: {prev_score:.4f} -> {h["score"]:.4f}'); sys.exit(1)
    prev_score = h['score']
print(f'scores descending correct: {len(results)} results'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "offset_beyond_total".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_offsetbeyond_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i]} for i in range(5)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5], "limit":3, "offset":100})
results = r.json().get('result', [])
if len(results) > 0: print(f'[DEFECT: STATE_LOGIC_VIOLATION] offset=100 on 5 points returned {len(results)} results'); sys.exit(1)
print(f'offset beyond total correctly returns empty'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "upsert_readback_vector".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_rbvec_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
written = [0.1, 0.2, 0.3, 0.4]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":written}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/scroll', json={"limit":1,"with_vector":True,"with_payload":False})
points = r.json().get('result',{}).get('points',[])
if not points: print(f'readback failed: no points returned'); sys.exit(0)
read_vec = points[0].get('vector',[])
for i in range(len(written)):
    if abs(read_vec[i] - written[i]) > 0.001:
        print(f'[DEFECT: DATA_CORRUPTION] vector mismatch at index {i}: written={written[i]}, read={read_vec[i]}'); sys.exit(1)
print(f'vector readback correct: {read_vec}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "upsert_readback_payload".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_rbpay_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
written = {"color": "red", "size": 42}
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4],"payload":written}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/scroll', json={"limit":1,"with_vector":False,"with_payload":True})
points = r.json().get('result',{}).get('points',[])
if not points: print(f'readback failed: no points returned'); sys.exit(0)
read_payload = points[0].get('payload',{})
for k, v in written.items():
    if k not in read_payload:
        print(f'[DEFECT: DATA_CORRUPTION] payload key missing: {k}'); sys.exit(1)
    if read_payload[k] != v:
        print(f'[DEFECT: DATA_CORRUPTION] payload mismatch for key {k}: written={v}, read={read_payload[k]}'); sys.exit(1)
print(f'payload readback correct: {read_payload}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "upsert_readback_overwrite".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_rbowr_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
vec_a = [0.1, 0.2, 0.3, 0.4]
vec_b = [0.5, 0.6, 0.7, 0.8]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":vec_a}]})
if r.status_code != 200: print(f'upsert A failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":vec_b}]})
if r.status_code != 200: print(f'upsert B failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/scroll', json={"limit":1,"with_vector":True,"with_payload":False})
points = r.json().get('result',{}).get('points',[])
if not points: print(f'readback failed: no points returned'); sys.exit(0)
read_vec = points[0].get('vector',[])
for i in range(len(vec_b)):
    if abs(read_vec[i] - vec_b[i]) > 0.001:
        print(f'[DEFECT: DATA_CORRUPTION] overwrite vector mismatch at index {i}: expected={vec_b[i]}, read={read_vec[i]}'); sys.exit(1)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result',{}).get('points_count',-1)
if count != 1:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] overwrite should keep count=1 but got {count}'); sys.exit(1)
print(f'overwrite readback correct: vector={read_vec}, count=1'); sys.exit(0)"#.to_string(),
                redundant_with_mutation: true,
            },
            SafetyNet {
                name: "update_payload_readback".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_rbupd_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
orig_vec = [0.1, 0.2, 0.3, 0.4]
orig_payload = {"color": "red"}
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":orig_vec,"payload":orig_payload}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
new_payload = {"color": "blue", "size": 99}
r = requests.post(f'{BASE}/collections/{c}/points/payload', json={"payload":new_payload,"points":[1]})
if r.status_code not in (200, 201): print(f'payload update failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/scroll', json={"limit":1,"with_vector":True,"with_payload":True})
points = r.json().get('result',{}).get('points',[])
if not points: print(f'readback failed: no points returned'); sys.exit(0)
read_vec = points[0].get('vector',[])
read_payload = points[0].get('payload',{})
for i in range(len(orig_vec)):
    if abs(read_vec[i] - orig_vec[i]) > 0.001:
        print(f'[DEFECT: DATA_CORRUPTION] vector changed after payload update at index {i}: original={orig_vec[i]}, read={read_vec[i]}'); sys.exit(1)
for k, v in new_payload.items():
    if k not in read_payload or read_payload[k] != v:
        print(f'[DEFECT: DATA_CORRUPTION] payload not updated for key {k}: expected={v}, read={read_payload.get(k)}'); sys.exit(1)
print(f'update payload readback correct: vector unchanged, payload={read_payload}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "delete_readback".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_rbdel_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4],"payload":{"x":1}}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points":[1]})
if r.status_code not in (200, 201): print(f'delete failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/scroll', json={"limit":10,"with_vector":True,"with_payload":True})
points = r.json().get('result',{}).get('points',[])
if len(points) > 0:
    print(f'[DEFECT: DATA_CORRUPTION] deleted point still readable: {points[0]}'); sys.exit(1)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result',{}).get('points_count',-1)
if count != 0:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] after delete, count={count} instead of 0'); sys.exit(1)
print(f'delete readback correct: point gone, count=0'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "batch_upsert_readback".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_rbbat_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
N = 10
written = []
for i in range(N):
    written.append({"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)], "payload": {"idx": i}})
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": written})
if r.status_code != 200: print(f'batch upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/scroll', json={"limit":N+5,"with_vector":True,"with_payload":True})
points = r.json().get('result',{}).get('points',[])
by_id = {p['id']: p for p in points}
for w in written:
    if w['id'] not in by_id:
        print(f'[DEFECT: DATA_CORRUPTION] point id={w["id"]} missing after batch upsert'); sys.exit(1)
    p = by_id[w['id']]
    read_vec = p.get('vector',[])
    for i in range(len(w['vector'])):
        if abs(read_vec[i] - w['vector'][i]) > 0.001:
            print(f'[DEFECT: DATA_CORRUPTION] batch point id={w["id"]} vector mismatch at {i}: written={w["vector"][i]}, read={read_vec[i]}'); sys.exit(1)
    read_payload = p.get('payload',{})
    if read_payload.get('idx') != w['payload']['idx']:
        print(f'[DEFECT: DATA_CORRUPTION] batch point id={w["id"]} payload mismatch: written={w["payload"]}, read={read_payload}'); sys.exit(1)
print(f'batch readback correct: {len(points)} points verified'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "cross_step_upsert_delete_count".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_csudc_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(5)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count1 = info.get('result',{}).get('points_count',-1)
if count1 != 5: print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_count] after upsert 5, count={count1}'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points": [1, 2]})
if r.status_code not in (200, 201): print(f'delete failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count2 = info.get('result',{}).get('points_count',-1)
if count2 != 3: print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_count] after delete 2 of 5, count={count2}'); sys.exit(1)
print(f'cross-step upsert-delete count: 5 -> 3 OK'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "cross_step_upsert_overwrite_count".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_csuoc_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'first upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
count1 = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('points_count',-1)
if count1 != 1: print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_count] after first upsert, count={count1}'); sys.exit(1)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.5,0.6,0.7,0.8]}]})
if r.status_code != 200: print(f'overwrite upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
count2 = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('points_count',-1)
if count2 != 1: print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_count] overwrite same id not idempotent: {count1}->{count2}'); sys.exit(1)
print(f'cross-step overwrite count: stayed 1 OK'); sys.exit(0)"#.to_string(),
                redundant_with_mutation: true,
            },
            SafetyNet {
                name: "cross_step_delete_nonexistent_count".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_csdne_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(3)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
count_before = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('points_count',-1)
if count_before != 3: print(f'[DEFECT: STATE_LOGIC_VIOLATION] after upsert 3, count={count_before}'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points": [999]})
time.sleep(0.5)
count_after = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('points_count',-1)
if count_after != 3: print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_count] delete nonexistent id changed count: {count_before}->{count_after}'); sys.exit(1)
print(f'cross-step delete nonexistent: count stayed 3 OK'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "cross_step_delete_collection_then_search".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_csdcs_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.delete(f'{BASE}/collections/{c}')
if r.status_code not in (200, 201): print(f'delete collection failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":3})
if r.status_code == 200: print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_lifecycle] search on deleted collection returned 200'); sys.exit(1)
print(f'cross-step delete collection then search: properly rejected ({r.status_code}) OK'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "cross_step_create_delete_recreate".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_cscdr_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'first create failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.delete(f'{BASE}/collections/{c}')
if r.status_code not in (200, 201): print(f'delete failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'recreate failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result',{}).get('points_count',-1)
if count != 0: print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_lifecycle] recreated collection has stale data: count={count}'); sys.exit(1)
status = info.get('result',{}).get('status','')
if status != 'green': print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_status] recreated collection status={status}'); sys.exit(1)
print(f'cross-step recreate: clean state (count=0, status=green) OK'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "cross_step_collection_status_after_ops".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_cscsa_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
status1 = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('status','')
if status1 != 'green': print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_status] empty collection status={status1}'); sys.exit(1)
points = [{"id": i+1, "vector": [0.1*i, 0.2*i, 0.3*i, 0.4*i]} for i in range(5)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
status2 = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('status','')
if status2 != 'green': print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_status] after upsert status={status2}'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points": [1, 2, 3]})
time.sleep(0.5)
status3 = requests.get(f'{BASE}/collections/{c}').json().get('result',{}).get('status','')
if status3 != 'green': print(f'[DEFECT: STATE_LOGIC_VIOLATION] [cross_step_status] after delete status={status3}'); sys.exit(1)
print(f'cross-step status: green throughout (empty->upsert->delete) OK'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "search_baseline_perf".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_perfsrch_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i]} for i in range(10)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
start = time.time()
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5],"limit":10})
elapsed_ms = (time.time() - start) * 1000
if r.status_code != 200: print(f'search failed: {r.status_code}'); sys.exit(0)
threshold_ms = 500
if elapsed_ms > threshold_ms:
    print(f'[DEFECT: PERFORMANCE_REGRESSION] search baseline took {elapsed_ms:.0f}ms (threshold={threshold_ms}ms)'); sys.exit(1)
print(f'search baseline perf OK: {elapsed_ms:.0f}ms < {threshold_ms}ms'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "upsert_baseline_perf".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_perfups_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.01*(i+1), 0.02*(i+1), 0.03*(i+1), 0.04*(i+1)]} for i in range(100)]
start = time.time()
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
elapsed_ms = (time.time() - start) * 1000
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
threshold_ms = 2000
if elapsed_ms > threshold_ms:
    print(f'[DEFECT: PERFORMANCE_REGRESSION] upsert 100 points took {elapsed_ms:.0f}ms (threshold={threshold_ms}ms)'); sys.exit(1)
print(f'upsert baseline perf OK: {elapsed_ms:.0f}ms < {threshold_ms}ms'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "search_hnsw_ef_comparison".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_perfef_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i]} for i in range(50)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
start = time.time()
r1 = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5],"limit":10,"params":{"hnsw_ef":128}})
elapsed_128 = (time.time() - start) * 1000
if r1.status_code != 200: print(f'search hnsw_ef=128 failed: {r1.status_code}'); sys.exit(0)
start = time.time()
r2 = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5],"limit":10,"params":{"hnsw_ef":1}})
elapsed_1 = (time.time() - start) * 1000
if r2.status_code != 200: print(f'search hnsw_ef=1 failed: {r2.status_code}'); sys.exit(0)
if elapsed_1 > elapsed_128 * 10 and elapsed_1 > 100:
    print(f'[DEFECT: PERFORMANCE_REGRESSION] hnsw_ef=1 ({elapsed_1:.0f}ms) is >10x slower than hnsw_ef=128 ({elapsed_128:.0f}ms)'); sys.exit(1)
print(f'hnsw_ef comparison OK: ef=128={elapsed_128:.0f}ms, ef=1={elapsed_1:.0f}ms'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "large_payload_perf".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_perfpld_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
big_payload = {"data": "x" * 10000}
start = time.time()
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4],"payload":big_payload}]})
elapsed_ms = (time.time() - start) * 1000
if r.status_code != 200: print(f'upsert with large payload failed: {r.status_code}'); sys.exit(0)
threshold_ms = 1000
if elapsed_ms > threshold_ms:
    print(f'[DEFECT: PERFORMANCE_REGRESSION] upsert 10KB payload took {elapsed_ms:.0f}ms (threshold={threshold_ms}ms)'); sys.exit(1)
print(f'large payload perf OK: {elapsed_ms:.0f}ms < {threshold_ms}ms'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "clear_points_count".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_clear_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.2*i, 0.3*i, 0.4*i]} for i in range(5)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/clear', json={})
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result',{}).get('points_count',-1)
if count != 0: print(f'[DEFECT: STATE_LOGIC_VIOLATION] after clear, count={count}, expected 0'); sys.exit(1)
print(f'clear points correct: count=0'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "alias_nonexistent_collection".into(),
                script: r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
alias = 'safety_alias_ne_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/collections/aliases', json={"actions":[{"create_alias":{"alias_name":alias,"collection_name":"nonexistent_collection_xyz"}}]})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] alias pointing to nonexistent collection accepted'); sys.exit(1)
else: print(f'alias to nonexistent collection properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "count_with_filter".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_cntfilt_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i], "payload": {"color": "red" if i%2==0 else "blue"}} for i in range(6)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/count', json={})
total = r.json().get('result',{}).get('count',-1)
if total != 6: print(f'[DEFECT: STATE_LOGIC_VIOLATION] total count={total}, expected 6'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/count', json={"filter":{"must":[{"key":"color","match":{"value":"red"}}]}})
red_count = r.json().get('result',{}).get('count',-1)
if red_count != 3: print(f'[DEFECT: STATE_LOGIC_VIOLATION] red count={red_count}, expected 3'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/count', json={"filter":{"must":[{"key":"nonexistent_xyz","match":{"value":"abc"}}]}})
ne_count = r.json().get('result',{}).get('count',-1)
if ne_count != 0: print(f'[DEFECT: STATE_LOGIC_VIOLATION] nonexistent filter count={ne_count}, expected 0'); sys.exit(1)
print(f'count with filter OK: total={total}, red={red_count}, nonexistent=0'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "batch_mixed_valid_invalid".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_batchmix_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id":1,"vector":[0.1,0.2,0.3,0.4]},{"id":2,"vector":[0.5,0.6,0.7]},{"id":3,"vector":[0.9,0.8,0.7,0.6]}]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result',{}).get('points_count',-1)
if count == 3: print(f'[DEFECT: ILLEGAL_SUCCESS] batch with mixed valid/invalid vectors accepted all 3 (count={count})'); sys.exit(1)
if count == 2: print(f'[DEFECT: POOR_DIAGNOSTICS] batch partially accepted: valid point stored but invalid silently discarded (count=2, no error reported)'); sys.exit(1)
if count == 0: print(f'batch correctly rejected entirely: count=0'); sys.exit(0)
print(f'batch mixed result: count={count}, status={r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
        ];

        let mut all_nets = Vec::with_capacity(boundary_nets.len() + special_nets.len() + state_nets.len());
        all_nets.extend(boundary_nets);
        all_nets.extend(special_nets);
        all_nets.extend(state_nets);

        let deep_nets = vec![
            SafetyNet {
                name: "search_empty_collection_returns_empty".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_empty_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":10})
if r.status_code != 200: print(f'search on empty collection failed: {r.status_code}'); sys.exit(0)
results = r.json().get('result',[])
if len(results) != 0: print(f'[DEFECT: STATE_LOGIC_VIOLATION] empty collection search returned {len(results)} results'); sys.exit(1)
print(f'empty collection search OK: 0 results'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "negative_point_id".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_negid_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":-1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] negative point id accepted'); sys.exit(1)
else: print(f'negative id properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "zero_point_id".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_zeroid_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":0,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] zero point id accepted'); sys.exit(1)
else: print(f'zero id properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "very_large_dimension".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_bigdim_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":65536,"distance":"Cosine"}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] dimension=65536 accepted'); sys.exit(1)
else: print(f'large dimension properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "dimension_one".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_dim1_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":1,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'dim=1 create rejected: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.5]}]})
if r.status_code != 200: print(f'dim=1 upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5],"limit":1})
if r.status_code != 200: print(f'dim=1 search failed: {r.status_code}'); sys.exit(0)
hits = r.json().get('result',[])
if not hits: print('[DEFECT: STATE_LOGIC_VIOLATION] dim=1 search returned no results'); sys.exit(1)
if hits[0].get('score',0) < 0.99: print(f'[DEFECT: DATA_CORRUPTION] dim=1 same vector search score={hits[0]["score"]:.4f} expected ~1.0'); sys.exit(1)
print(f'dim=1 OK: score={hits[0]["score"]:.4f}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "float_point_id".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_floatid_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1.5,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] float point id accepted'); sys.exit(1)
else: print(f'float id properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "string_point_id_when_integer_expected".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_strid_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":"not_a_number","vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] string point id accepted'); sys.exit(1)
else: print(f'string id properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "empty_vector_values".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_emptyvec_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[]}]})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] empty vector accepted'); sys.exit(1)
else: print(f'empty vector properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "wrong_distance_metric_change".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_distchg_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.patch(f'{BASE}/collections/{c}', json={"vectors":{"distance":"Euclid"}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] distance metric change after creation accepted'); sys.exit(1)
else: print(f'distance change properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "search_with_all_zeros_vector".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_zeroq_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.0,0.0,0.0,0.0],"limit":1})
if r.status_code != 200: print(f'zero vector search failed: {r.status_code}'); sys.exit(0)
results = r.json().get('result',[])
if not results: print('[DEFECT: STATE_LOGIC_VIOLATION] zero vector search returned no results'); sys.exit(1)
print(f'zero vector search OK: {len(results)} results'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "update_vector_then_filter_search".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_updflt_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4],"payload":{"color":"red"}}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
new_vec = [0.9,0.8,0.7,0.6]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":new_vec}]})
if r.status_code != 200: print(f'update failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":new_vec,"limit":5,"with_payload":True,"filter":{"must":[{"key":"color","match":{"value":"red"}}]}})
if r.status_code != 200: print(f'filtered search failed: {r.status_code}'); sys.exit(0)
hits = r.json().get('result',[])
if not hits: print('[DEFECT: DATA_CORRUPTION] updated point not found by filter after vector update'); sys.exit(1)
if hits[0].get('payload',{}).get('color') != 'red': print(f'[DEFECT: DATA_CORRUPTION] payload lost after vector update: {hits[0].get("payload")}'); sys.exit(1)
ret_vec = hits[0].get('vector',[])
for a,b in zip(new_vec,ret_vec):
    if abs(a-b) > 1e-6: print(f'[DEFECT: DATA_CORRUPTION] vector mismatch after update+filter'); sys.exit(1)
print(f'update+filter OK'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "duplicate_point_id_batch".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_dupbatch_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id":1,"vector":[0.1,0.2,0.3,0.4]},{"id":1,"vector":[0.5,0.6,0.7,0.8]}]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":points})
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count = info.get('result',{}).get('points_count',-1)
if count != 1: print(f'[DEFECT: STATE_LOGIC_VIOLATION] duplicate id in batch: count={count}, expected 1'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/scroll', json={"limit":10,"with_vector":True})
pts = r.json().get('result',{}).get('points',[])
if len(pts) != 1: print(f'[DEFECT: STATE_LOGIC_VIOLATION] duplicate id: scroll returned {len(pts)}'); sys.exit(1)
print(f'duplicate id batch OK: count=1'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
            SafetyNet {
                name: "search_limit_exceeds_total".into(),
                script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'deep_biglimit_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id":i+1,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(3)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5],"limit":10000})
if r.status_code != 200: print(f'large limit search failed: {r.status_code}'); sys.exit(0)
results = r.json().get('result',[])
if len(results) > 3: print(f'[DEFECT: STATE_LOGIC_VIOLATION] limit=10000 on 3 points returned {len(results)}'); sys.exit(1)
print(f'large limit OK: {len(results)} <= 3'); sys.exit(0)"#.to_string(),
                ..Default::default()
            },
        ];
        all_nets.extend(deep_nets);

        all_nets
    }

    fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>> {
        Some(Box::new(QdrantIndependentReviewer))
    }

    fn derive_oracle_checks(&self, contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        let superseded_names: HashSet<String> = contract
            .behavioral_contracts
            .iter()
            .filter_map(|bc| bc.supersedes.clone())
            .collect();

        let behavioral = OracleCheckDeriver::from_behavioral_contracts(contract);
        for check in behavioral {
            let key = check_key(&check);
            seen.insert(key);
            checks.push(check);
        }

        let range_checks = OracleCheckDeriver::from_range_constraints(contract);
        for check in range_checks {
            let key = check_key(&check);
            if seen.insert(key) {
                checks.push(check);
            }
        }

        let state_checks = OracleCheckDeriver::from_state_constraints(contract);
        for check in state_checks {
            let key = check_key(&check);
            if seen.insert(key) {
                checks.push(check);
            }
        }

        let type_checks = OracleCheckDeriver::from_type_constraints(contract);
        for check in type_checks {
            let key = check_key(&check);
            if seen.insert(key) {
                checks.push(check);
            }
        }

        let explicit = OracleCheckDeriver::from_explicit_invariants(contract, &superseded_names);
        for check in explicit {
            let key = check_key(&check);
            if seen.insert(key) {
                checks.push(check);
            }
        }

        let assertion_checks = OracleCheckDeriver::from_assertions(contract);
        for check in assertion_checks {
            let key = check_key(&check);
            if seen.insert(key) {
                checks.push(check);
            }
        }

        checks
    }
}

fn check_key(check: &InvariantCheck) -> String {
    check.name.to_lowercase()
}

struct OracleCheckDeriver;

impl OracleCheckDeriver {
    fn from_range_constraints(contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();

        for rc in &contract.range_constraints {
            let has_structured = rc.min.is_some() || rc.max.is_some();

            if has_structured {
                if let Some(min_val) = rc.min {
                    let boundary = if min_val > 0.0 { min_val - 1.0 } else { 0.0 };
                    let label = format!("{}={}", rc.param_name, format_boundary(boundary));
                    let et = classify_endpoint_type(&rc.param_name, &contract.api_endpoint);

                    let script = if is_search_param(&rc.param_name) {
                        search_params_probe(&rc.param_name, &format_boundary(boundary), &label)
                    } else {
                        match et {
                            EndpointType::Search => search_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Create => create_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Upsert => upsert_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Delete => delete_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Scroll => scroll_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Recommend => recommend_probe(&rc.param_name, &format_boundary(boundary), &label),
                        }
                    };

                    checks.push(InvariantCheck {
                        name: format!("oracle_range_{}_below_min", rc.param_name),
                        check_type: CheckType::ValueRange,
                        script,
                        source: InvariantSource::DerivedFromRange,
                    });
                }

                if let Some(max_val) = rc.max {
                    let boundary = max_val + 1.0;
                    let label = format!("{}={}", rc.param_name, format_boundary(boundary));
                    let et = classify_endpoint_type(&rc.param_name, &contract.api_endpoint);

                    let script = if is_search_param(&rc.param_name) {
                        search_params_probe(&rc.param_name, &format_boundary(boundary), &label)
                    } else {
                        match et {
                            EndpointType::Search => search_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Create => create_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Upsert => upsert_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Delete => delete_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Scroll => scroll_probe(&rc.param_name, &format_boundary(boundary), &label),
                            EndpointType::Recommend => recommend_probe(&rc.param_name, &format_boundary(boundary), &label),
                        }
                    };

                    checks.push(InvariantCheck {
                        name: format!("oracle_range_{}_above_max", rc.param_name),
                        check_type: CheckType::ValueRange,
                        script,
                        source: InvariantSource::DerivedFromRange,
                    });
                }
            } else {
                let param = rc.param_name.to_lowercase();
                let desc = rc.description.to_lowercase();

                if param == "limit" || desc.contains("limit") && desc.contains("> 0") {
                    checks.push(InvariantCheck {
                        name: "oracle_limit_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: search_probe("limit", "0", "limit=0"),
                        source: InvariantSource::DerivedFromRange,
                    });
                } else if param == "offset" || desc.contains("offset") {
                    checks.push(InvariantCheck {
                        name: "oracle_offset_negative".to_string(),
                        check_type: CheckType::ValueRange,
                        script: search_probe("offset", "-1", "offset=-1"),
                        source: InvariantSource::DerivedFromRange,
                    });
                } else if param == "hnsw_ef" || desc.contains("hnsw_ef") {
                    checks.push(InvariantCheck {
                        name: "oracle_hnsw_ef_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: search_params_probe("hnsw_ef", "0", "hnsw_ef=0"),
                        source: InvariantSource::DerivedFromRange,
                    });
                } else if param == "vectors.size" || desc.contains("size") && desc.contains("> 0") {
                    checks.push(InvariantCheck {
                        name: "oracle_vectors_size_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: create_probe("vectors.size", "0", "vectors.size=0"),
                        source: InvariantSource::DerivedFromRange,
                    });
                } else if param == "shard_number" || desc.contains("shard") {
                    checks.push(InvariantCheck {
                        name: "oracle_shard_number_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: create_probe("shard_number", "0", "shard_number=0"),
                        source: InvariantSource::DerivedFromRange,
                    });
                } else if param == "replication_factor" || desc.contains("replication") {
                    checks.push(InvariantCheck {
                        name: "oracle_replication_factor_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: create_probe("replication_factor", "0", "replication_factor=0"),
                        source: InvariantSource::DerivedFromRange,
                    });
                } else if param == "write_consistency_factor" || desc.contains("write_consistency") {
                    checks.push(InvariantCheck {
                        name: "oracle_write_consistency_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: create_probe("write_consistency_factor", "0", "write_consistency_factor=0"),
                        source: InvariantSource::DerivedFromRange,
                    });
                }
            }
        }

        checks
    }

    fn from_state_constraints(contract: &StructuredContract) -> Vec<InvariantCheck> {
        contract
            .state_constraints
            .iter()
            .filter_map(|sc| {
                let desc = sc.description.to_lowercase();
                if desc.contains("create") && desc.contains("before")
                    || desc.contains("exist") && desc.contains("before")
                    || desc.contains("collection") && desc.contains("before")
                {
                    Some(InvariantCheck {
                        name: format!("oracle_state_{}", sc.description.len()),
                        check_type: CheckType::ExistenceCheck,
                        script: search_nonexistent_collection(),
                        source: InvariantSource::DerivedFromState,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn from_type_constraints(contract: &StructuredContract) -> Vec<InvariantCheck> {
        contract
            .type_constraints
            .iter()
            .filter_map(|tc| {
                let expected = tc.expected_type.to_lowercase();
                if expected == "integer" || expected == "int" {
                    Some(InvariantCheck {
                        name: format!("oracle_type_{}_as_string", tc.param_name),
                        check_type: CheckType::ValueRange,
                        script: search_string_probe(&tc.param_name, "abc", &format!("{}=abc", tc.param_name)),
                        source: InvariantSource::DerivedFromType,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn from_explicit_invariants(contract: &StructuredContract, superseded: &HashSet<String>) -> Vec<InvariantCheck> {
        crate::agent::oracle::Oracle::from_explicit_invariants(&contract.state_invariants)
            .into_iter()
            .filter(|check| !superseded.contains(&check.name))
            .collect()
    }

    fn from_behavioral_contracts(contract: &StructuredContract) -> Vec<InvariantCheck> {
        contract
            .behavioral_contracts
            .iter()
            .filter(|bc| !bc.verification_script.is_empty())
            .map(|bc| {
                let check_type = match bc.category {
                    BehaviorCategory::StateConsistency => CheckType::CountConsistency,
                    BehaviorCategory::SemanticCorrectness => CheckType::ValueRange,
                    BehaviorCategory::InterfaceConsistency => CheckType::Idempotency,
                    BehaviorCategory::DiagnosticQuality => CheckType::ValueRange,
                };
                InvariantCheck {
                    name: format!("behavior_{}", bc.name),
                    check_type,
                    script: bc.verification_script.clone(),
                    source: InvariantSource::DerivedFromBehavior,
                }
            })
            .collect()
    }

    fn from_assertions(contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();
        let mut seen = HashSet::new();

        for assertion in &contract.assertions {
            let lower = assertion.to_lowercase();

            if lower.contains("count") && lower.contains("must") && seen.insert("count_consistency".to_string()) {
                checks.push(InvariantCheck {
                    name: "oracle_assert_count_consistency".to_string(),
                    check_type: CheckType::CountConsistency,
                    script: count_consistency_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if (lower.contains("limit") && lower.contains("> 0") || lower.contains("limit") && lower.contains("must be positive"))
                && seen.insert("limit_zero".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_limit_zero".to_string(),
                    check_type: CheckType::ValueRange,
                    script: search_probe("limit", "0", "limit=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("offset") && lower.contains(">=") && lower.contains("0")
                && seen.insert("offset_negative".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_offset_negative".to_string(),
                    check_type: CheckType::ValueRange,
                    script: search_probe("offset", "-1", "offset=-1"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("hnsw_ef") && (lower.contains(">=") || lower.contains("> 0") || lower.contains("must not be 0"))
                && seen.insert("hnsw_ef_zero".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_hnsw_ef_zero".to_string(),
                    check_type: CheckType::ValueRange,
                    script: search_params_probe("hnsw_ef", "0", "hnsw_ef=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if (lower.contains("non-existent") || lower.contains("nonexistent") || lower.contains("not exist"))
                && lower.contains("collection")
                && seen.insert("search_nonexistent".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_search_nonexistent".to_string(),
                    check_type: CheckType::ExistenceCheck,
                    script: search_nonexistent_collection(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("score_threshold") && lower.contains("between")
                && seen.insert("score_threshold_out_of_range".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_score_threshold_2".to_string(),
                    check_type: CheckType::ValueRange,
                    script: search_probe("score_threshold", "2.0", "score_threshold=2.0 (out of 0-1)"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("nan") && lower.contains("vector") && lower.contains("must")
                && seen.insert("nan_vector".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_nan_vector".to_string(),
                    check_type: CheckType::ValueRange,
                    script: nan_vector_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("duplicate") && lower.contains("collection") && lower.contains("conflict")
                && seen.insert("duplicate_collection".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_duplicate_collection".to_string(),
                    check_type: CheckType::ExistenceCheck,
                    script: duplicate_collection_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("distance") && lower.contains("invalid") && lower.contains("reject")
                && seen.insert("invalid_distance".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_invalid_distance".to_string(),
                    check_type: CheckType::ValueRange,
                    script: invalid_distance_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }
        }

        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{RangeConstraint, BehavioralContract, BehaviorCategory};

    #[test]
    fn test_structured_range_derivation_with_min() {
        let contract = StructuredContract {
            api_endpoint: "search_points".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit must be > 0".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };

        let checks = OracleCheckDeriver::from_range_constraints(&contract);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].name.contains("limit"));
        assert!(checks[0].name.contains("below_min"));
        assert!(checks[0].script.contains("limit"));
    }

    #[test]
    fn test_structured_range_derivation_with_min_and_max() {
        let contract = StructuredContract {
            api_endpoint: "search_points".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![RangeConstraint {
                param_name: "score_threshold".to_string(),
                description: "score_threshold between 0 and 1".to_string(),
                min: Some(0.0),
                max: Some(1.0),
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };

        let checks = OracleCheckDeriver::from_range_constraints(&contract);
        assert_eq!(checks.len(), 2);
        assert!(checks.iter().any(|c| c.name.contains("below_min")));
        assert!(checks.iter().any(|c| c.name.contains("above_max")));
    }

    #[test]
    fn test_keyword_fallback_when_no_min_max() {
        let contract = StructuredContract {
            api_endpoint: "search_points".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit must be > 0".to_string(),
                min: None,
                max: None,
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };

        let checks = OracleCheckDeriver::from_range_constraints(&contract);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].name, "oracle_limit_zero");
    }

    #[test]
    fn test_behavioral_contracts_derivation() {
        let contract = StructuredContract {
            api_endpoint: "upsert_points".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![BehavioralContract {
                name: "upsert_count_consistency".to_string(),
                category: BehaviorCategory::StateConsistency,
                endpoints: vec!["upsert_points".to_string()],
                precondition_script: "collection exists".to_string(),
                verification_script: "print('verify')".to_string(),
                expected_outcome: "count == N".to_string(),
                supersedes: None,
                mutation_rules: vec![],
            }],

        };

        let checks = OracleCheckDeriver::from_behavioral_contracts(&contract);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].name, "behavior_upsert_count_consistency");
        assert_eq!(checks[0].check_type, CheckType::CountConsistency);
        assert_eq!(checks[0].source, InvariantSource::DerivedFromBehavior);
    }

    #[test]
    fn test_dedup_in_derive_oracle_checks() {
        let plugin = QdrantPlugin;
        let contract = StructuredContract {
            api_endpoint: "search_points".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec!["limit must be > 0".to_string()],
            type_constraints: vec![],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit must be > 0".to_string(),
                min: Some(1.0),
                max: None,
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
        };

        let checks = plugin.derive_oracle_checks(&contract);
        let limit_checks: Vec<_> = checks.iter().filter(|c| c.name.contains("limit")).collect();
        assert!(limit_checks.len() <= 2, "should not have excessive duplicates, got {:?}", limit_checks);
    }

    #[test]
    fn test_supersedes_skips_state_invariant() {
        let plugin = QdrantPlugin;
        let contract = StructuredContract {
            api_endpoint: "search_points".to_string(),
            doc_url: "https://example.com".to_string(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![crate::contract::schema::StateInvariant {
                name: "count_check".to_string(),
                check_type: CheckType::CountConsistency,
                endpoint: "/collections/test/points".to_string(),
                precondition: "collection exists".to_string(),
                assertion_script: "print('check')".to_string(),
            }],
            behavioral_contracts: vec![BehavioralContract {
                name: "upsert_count_consistency".to_string(),
                category: BehaviorCategory::StateConsistency,
                endpoints: vec!["upsert_points".to_string()],
                precondition_script: "collection exists".to_string(),
                verification_script: "print('verify')".to_string(),
                expected_outcome: "count == N".to_string(),
                supersedes: Some("count_check".to_string()),
                mutation_rules: vec![],
            }],
        };

        let checks = plugin.derive_oracle_checks(&contract);
        assert!(checks.iter().any(|c| c.name == "behavior_upsert_count_consistency"));
        assert!(!checks.iter().any(|c| c.name == "count_check"));
    }

    #[test]
    fn test_create_probe_generates_valid_script() {
        let script = create_probe("shard_number", "0", "shard_number=0");
        assert!(script.contains("shard_number"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_search_probe_generates_valid_script() {
        let script = search_probe("limit", "0", "limit=0");
        assert!(script.contains("limit"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_classify_endpoint_type() {
        assert_eq!(classify_endpoint_type("limit", "search_points"), EndpointType::Search);
        assert_eq!(classify_endpoint_type("size", "create_collection"), EndpointType::Create);
        assert_eq!(classify_endpoint_type("shard_number", "create_collection"), EndpointType::Create);
        assert_eq!(classify_endpoint_type("hnsw_ef", "search_points"), EndpointType::Search);
    }

    #[test]
    fn test_format_boundary() {
        assert_eq!(format_boundary(0.0), "0");
        assert_eq!(format_boundary(1.0), "1");
        assert_eq!(format_boundary(-1.0), "-1");
        assert_eq!(format_boundary(2.5), "2.5");
    }
}
