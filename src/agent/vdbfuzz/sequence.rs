use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzSequenceCase {
    pub name: String,
    pub script: String,
    pub sequence_type: String,
    pub expected_defect: Option<String>,
}

pub struct APISequenceExplorer;

impl APISequenceExplorer {
    pub fn generate_sequences() -> Vec<FuzzSequenceCase> {
        let mut cases = Vec::new();

        cases.push(Self::missing_step_no_create_then_upsert());
        cases.push(Self::missing_step_no_create_then_search());
        cases.push(Self::redundant_op_duplicate_create());
        cases.push(Self::wrong_order_delete_then_search());
        cases.push(Self::state_transition_create_delete_recreate());
        cases.push(Self::async_comparison_wrong_dimension());
        cases.push(Self::data_integrity_upsert_search_readback());
        cases.push(Self::data_integrity_update_payload_search_filter());
        cases.push(Self::data_integrity_delete_count_consistency());
        cases.push(Self::combo_hnsw_ef_zero_with_limit_zero());

        cases
    }

    fn missing_step_no_create_then_upsert() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "missing_step_no_create_then_upsert".into(),
            sequence_type: "missing_step".into(),
            expected_defect: Some("ILLEGAL_SUCCESS".into()),
            script: r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_nocreate_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] upsert to nonexistent collection accepted'); sys.exit(1)
else: print(f'upsert to nonexistent collection properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
        }
    }

    fn missing_step_no_create_then_search() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "missing_step_no_create_then_search".into(),
            sequence_type: "missing_step".into(),
            expected_defect: Some("ILLEGAL_SUCCESS".into()),
            script: r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_nocreate_srch_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":3})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] search on nonexistent collection accepted'); sys.exit(1)
else: print(f'search on nonexistent collection properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
        }
    }

    fn redundant_op_duplicate_create() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "redundant_op_duplicate_create".into(),
            sequence_type: "redundant_op".into(),
            expected_defect: Some("ILLEGAL_SUCCESS".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_dupcreate_' + uuid.uuid4().hex[:8]
r1 = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r1.status_code not in (200, 201): print(f'first create failed: {r1.status_code}'); sys.exit(0)
time.sleep(0.3)
r2 = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r2.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] duplicate collection name accepted (200)'); sys.exit(1)
else: print(f'duplicate collection properly rejected: {r2.status_code}'); sys.exit(0)"#.to_string(),
        }
    }

    fn wrong_order_delete_then_search() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "wrong_order_delete_then_search".into(),
            sequence_type: "wrong_order".into(),
            expected_defect: Some("STATE_LOGIC_VIOLATION".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_delthenrch_' + uuid.uuid4().hex[:8]
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
if r.status_code == 200: print('[DEFECT: STATE_LOGIC_VIOLATION] search on deleted collection returned 200'); sys.exit(1)
else: print(f'search on deleted collection properly rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
        }
    }

    fn state_transition_create_delete_recreate() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "state_transition_create_delete_recreate".into(),
            sequence_type: "state_transition".into(),
            expected_defect: Some("STATE_LOGIC_VIOLATION".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_recreate_' + uuid.uuid4().hex[:8]
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
if count != 0: print(f'[DEFECT: STATE_LOGIC_VIOLATION] recreated collection has stale data: count={count}'); sys.exit(1)
status = info.get('result',{}).get('status','')
if status != 'green': print(f'[DEFECT: STATE_LOGIC_VIOLATION] recreated collection status={status}'); sys.exit(1)
print(f'state transition correct: clean state (count=0, status=green)'); sys.exit(0)"#.to_string(),
        }
    }

    fn async_comparison_wrong_dimension() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "async_comparison_wrong_dimension".into(),
            sequence_type: "state_transition".into(),
            expected_defect: Some("POOR_DIAGNOSTICS".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_asyncdim_' + uuid.uuid4().hex[:8]
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
    print(f'async dimension check: wait={r_wait.status_code} nowait={r_nowait.status_code} count={count}')
    sys.exit(0)"#.to_string(),
        }
    }

    fn data_integrity_upsert_search_readback() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "data_integrity_upsert_search_readback".into(),
            sequence_type: "state_transition".into(),
            expected_defect: Some("DATA_CORRUPTION".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_readback_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
written = [0.1, 0.2, 0.3, 0.4]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":written}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":written,"limit":1,"with_vector":True})
if r.status_code != 200: print(f'search failed: {r.status_code}'); sys.exit(0)
hits = r.json().get('result',[])
if not hits: print('[DEFECT: DATA_CORRUPTION] no results'); sys.exit(1)
ret = hits[0].get('vector',[])
for a,b in zip(written,ret):
    if abs(a-b) > 1e-6: print(f'[DEFECT: DATA_CORRUPTION] vector mismatch: {written} vs {ret}'); sys.exit(1)
print(f'readback OK'); sys.exit(0)"#.to_string(),
        }
    }

    fn data_integrity_update_payload_search_filter() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "data_integrity_update_payload_search_filter".into(),
            sequence_type: "state_transition".into(),
            expected_defect: Some("DATA_CORRUPTION".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_updflt_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4],"payload":{"color":"red"}}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
new_payload = {"color":"blue","size":99}
r = requests.post(f'{BASE}/collections/{c}/points/payload', json={"payload":new_payload,"points":[1]})
if r.status_code not in (200, 201): print(f'payload update failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":5,"with_payload":True,"filter":{"must":[{"key":"color","match":{"value":"blue"}}]}})
if r.status_code != 200: print(f'filtered search failed: {r.status_code}'); sys.exit(0)
hits = r.json().get('result',[])
if not hits: print('[DEFECT: DATA_CORRUPTION] updated point not found by new filter value'); sys.exit(1)
if hits[0].get('payload',{}).get('color') != 'blue': print(f'[DEFECT: DATA_CORRUPTION] payload not updated: {hits[0].get("payload")}'); sys.exit(1)
print(f'update+filter OK'); sys.exit(0)"#.to_string(),
        }
    }

    fn data_integrity_delete_count_consistency() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "data_integrity_delete_count_consistency".into(),
            sequence_type: "state_transition".into(),
            expected_defect: Some("STATE_LOGIC_VIOLATION".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_delcnt_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
N = 5
points = [{"id":i+1,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(N)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count1 = info.get('result',{}).get('points_count',-1)
if count1 != N: print(f'[DEFECT: STATE_LOGIC_VIOLATION] after upsert {N}, count={count1}'); sys.exit(1)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points":[1,2]})
if r.status_code not in (200, 201): print(f'delete failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
info = requests.get(f'{BASE}/collections/{c}').json()
count2 = info.get('result',{}).get('points_count',-1)
if count2 != N-2: print(f'[DEFECT: STATE_LOGIC_VIOLATION] after delete 2 of {N}, count={count2} expected {N-2}'); sys.exit(1)
print(f'delete count OK: {N} -> {N-2}'); sys.exit(0)"#.to_string(),
        }
    }

    fn combo_hnsw_ef_zero_with_limit_zero() -> FuzzSequenceCase {
        FuzzSequenceCase {
            name: "combo_hnsw_ef_zero_with_limit_zero".into(),
            sequence_type: "state_transition".into(),
            expected_defect: Some("ILLEGAL_SUCCESS".into()),
            script: r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'seq_combo_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":0,"params":{"hnsw_ef":0}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] limit=0 + hnsw_ef=0 combo accepted'); sys.exit(1)
else: print(f'combo rejected: {r.status_code}'); sys.exit(0)"#.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_sequences() {
        let cases = APISequenceExplorer::generate_sequences();
        assert!(cases.len() >= 9);
        assert!(cases.iter().any(|c| c.sequence_type == "missing_step"));
        assert!(cases.iter().any(|c| c.sequence_type == "redundant_op"));
        assert!(cases.iter().any(|c| c.sequence_type == "wrong_order"));
        assert!(cases.iter().any(|c| c.sequence_type == "state_transition"));
    }

    #[test]
    fn test_sequence_scripts_contain_defect_markers() {
        let cases = APISequenceExplorer::generate_sequences();
        for case in &cases {
            assert!(case.script.contains("[DEFECT:"), "Sequence '{}' missing DEFECT marker", case.name);
        }
    }
}
