use super::{SafetyNet, TargetPlugin};
use crate::agent::oracle::{InvariantCheck, InvariantSource};
use crate::contract::schema::{CheckType, StructuredContract};
use crate::review::qdrant::QdrantIndependentReviewer;
use crate::review::IndependentReviewer;

pub struct QdrantPlugin;

impl TargetPlugin for QdrantPlugin {
    fn name(&self) -> &str {
        "qdrant"
    }

    fn target_image(&self, version: &str) -> String {
        format!("qdrant/qdrant:v{}", version)
    }

    fn pip_packages(&self) -> Vec<String> {
        vec!["qdrant-client".to_string(), "httpx".to_string(), "requests".to_string()]
    }

    fn db_port(&self) -> u16 {
        6333
    }

    fn safety_nets(&self) -> Vec<SafetyNet> {
        vec![
            SafetyNet {
                name: "hnsw_ef=0".to_string(),
                script: r#"import requests, json, sys
BASE = '{{TESTVDB_DB_URL}}'
c = 'test_hnsw_safety'
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
import time; time.sleep(0.5)
requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":3,"params":{"hnsw_ef":0}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted'); sys.exit(1)
else: print(f'properly rejected hnsw_ef=0: {r.status_code}'); sys.exit(0)"#.to_string(),
            },
            SafetyNet {
                name: "negative_limit".to_string(),
                script: r#"import requests, json, sys
BASE = '{{TESTVDB_DB_URL}}'
c = 'test_neg_limit'
requests.delete(f'{BASE}/collections/{c}')
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
import time; time.sleep(0.5)
requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":-1})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] negative limit accepted'); sys.exit(1)
else: print(f'properly rejected negative limit: {r.status_code}'); sys.exit(0)"#.to_string(),
            },
            SafetyNet {
                name: "oversampling=0".to_string(),
                script: r#"import requests, json, sys
BASE = '{{TESTVDB_DB_URL}}'
c = 'test_oversampling0'
requests.delete(f'{BASE}/collections/{c}')
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
import time; time.sleep(0.5)
requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":3,"params":{"quantization":{"oversampling":0}}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] oversampling=0 accepted'); sys.exit(1)
else: print(f'properly rejected oversampling=0: {r.status_code}'); sys.exit(0)"#.to_string(),
            },
            SafetyNet {
                name: "empty_vector_search".to_string(),
                script: r#"import requests, json, sys
BASE = '{{TESTVDB_DB_URL}}'
c = 'test_empty_vec'
requests.delete(f'{BASE}/collections/{c}')
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
import time; time.sleep(0.5)
requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[],"limit":3})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] empty vector accepted'); sys.exit(1)
else: print(f'properly rejected empty vector: {r.status_code}'); sys.exit(0)"#.to_string(),
            },
            SafetyNet {
                name: "replication_factor=0".to_string(),
                script: r#"import requests, json, sys
BASE = '{{TESTVDB_DB_URL}}'
c = 'test_repl0'
requests.delete(f'{BASE}/collections/{c}')
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"},"replication_factor":0})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] replication_factor=0 accepted'); sys.exit(1)
else: print(f'properly rejected replication_factor=0: {r.status_code}'); sys.exit(0)"#.to_string(),
            },
            SafetyNet {
                name: "write_consistency_factor=0".to_string(),
                script: r#"import requests, json, sys
BASE = '{{TESTVDB_DB_URL}}'
c = 'test_wcf0'
requests.delete(f'{BASE}/collections/{c}')
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"},"write_consistency_factor":0})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] write_consistency_factor=0 accepted'); sys.exit(1)
else: print(f'properly rejected write_consistency_factor=0: {r.status_code}'); sys.exit(0)"#.to_string(),
            },
        ]
    }

    fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>> {
        Some(Box::new(QdrantIndependentReviewer))
    }

    fn derive_oracle_checks(&self, contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();

        checks.extend(OracleCheckDeriver::from_range_constraints(contract));
        checks.extend(OracleCheckDeriver::from_state_constraints(contract));
        checks.extend(OracleCheckDeriver::from_type_constraints(contract));
        checks.extend(OracleCheckDeriver::from_explicit_invariants(contract));
        checks.extend(OracleCheckDeriver::from_assertions(contract));

        checks
    }
}

struct OracleCheckDeriver;

impl OracleCheckDeriver {
    fn from_range_constraints(contract: &StructuredContract) -> Vec<InvariantCheck> {
        contract
            .range_constraints
            .iter()
            .filter_map(|rc| {
                let param = rc.param_name.to_lowercase();
                let desc = rc.description.to_lowercase();

                if param == "limit" || desc.contains("limit") && desc.contains("> 0") {
                    Some(InvariantCheck {
                        name: "oracle_limit_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: qdrant_search_probe("limit", 0, "limit=0"),
                        source: InvariantSource::DerivedFromRange,
                    })
                } else if param == "offset" || desc.contains("offset") {
                    Some(InvariantCheck {
                        name: "oracle_offset_negative".to_string(),
                        check_type: CheckType::ValueRange,
                        script: qdrant_search_probe("offset", -1, "offset=-1"),
                        source: InvariantSource::DerivedFromRange,
                    })
                } else if param == "hnsw_ef" || desc.contains("hnsw_ef") {
                    Some(InvariantCheck {
                        name: "oracle_hnsw_ef_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: qdrant_search_params_probe("hnsw_ef", 0, "hnsw_ef=0"),
                        source: InvariantSource::DerivedFromRange,
                    })
                } else if param == "vectors.size" || desc.contains("size") && desc.contains("> 0") {
                    Some(InvariantCheck {
                        name: "oracle_vectors_size_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: qdrant_create_size_probe(0, "vectors.size=0"),
                        source: InvariantSource::DerivedFromRange,
                    })
                } else if param == "shard_number" || desc.contains("shard") {
                    Some(InvariantCheck {
                        name: "oracle_shard_number_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: qdrant_create_shard_probe(0, "shard_number=0"),
                        source: InvariantSource::DerivedFromRange,
                    })
                } else if param == "replication_factor" || desc.contains("replication") {
                    Some(InvariantCheck {
                        name: "oracle_replication_factor_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: qdrant_create_replication_probe(0, "replication_factor=0"),
                        source: InvariantSource::DerivedFromRange,
                    })
                } else if param == "write_consistency_factor" || desc.contains("write_consistency") {
                    Some(InvariantCheck {
                        name: "oracle_write_consistency_zero".to_string(),
                        check_type: CheckType::ValueRange,
                        script: qdrant_create_wcf_probe(0, "write_consistency_factor=0"),
                        source: InvariantSource::DerivedFromRange,
                    })
                } else {
                    None
                }
            })
            .collect()
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
                        script: qdrant_search_nonexistent_collection(),
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
                let param = tc.param_name.to_lowercase();
                let expected = tc.expected_type.to_lowercase();
                if expected == "integer" || expected == "int" {
                    Some(InvariantCheck {
                        name: format!("oracle_type_{}_as_string", param),
                        check_type: CheckType::ValueRange,
                        script: qdrant_search_string_probe(&param, "abc", &format!("{}='abc'", param)),
                        source: InvariantSource::DerivedFromType,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn from_explicit_invariants(contract: &StructuredContract) -> Vec<InvariantCheck> {
        crate::agent::oracle::Oracle::from_explicit_invariants(&contract.state_invariants)
    }

    fn from_assertions(contract: &StructuredContract) -> Vec<InvariantCheck> {
        let mut checks = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for assertion in &contract.assertions {
            let lower = assertion.to_lowercase();

            if lower.contains("count") && lower.contains("must") && seen.insert("count_consistency".to_string()) {
                checks.push(InvariantCheck {
                    name: "oracle_assert_count_consistency".to_string(),
                    check_type: CheckType::CountConsistency,
                    script: qdrant_count_consistency_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if (lower.contains("limit") && lower.contains("> 0") || lower.contains("limit") && lower.contains("must be positive"))
                && seen.insert("limit_zero".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_limit_zero".to_string(),
                    check_type: CheckType::ValueRange,
                    script: qdrant_search_probe("limit", 0, "limit=0"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("offset") && lower.contains(">=") && lower.contains("0")
                && seen.insert("offset_negative".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_offset_negative".to_string(),
                    check_type: CheckType::ValueRange,
                    script: qdrant_search_probe("offset", -1, "offset=-1"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("hnsw_ef") && (lower.contains(">=") || lower.contains("> 0") || lower.contains("must not be 0"))
                && seen.insert("hnsw_ef_zero".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_hnsw_ef_zero".to_string(),
                    check_type: CheckType::ValueRange,
                    script: qdrant_search_params_probe("hnsw_ef", 0, "hnsw_ef=0"),
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
                    script: qdrant_search_nonexistent_collection(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("score_threshold") && lower.contains("between")
                && seen.insert("score_threshold_out_of_range".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_score_threshold_2".to_string(),
                    check_type: CheckType::ValueRange,
                    script: qdrant_search_float_probe("score_threshold", 2.0, "score_threshold=2.0 (out of 0-1)"),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("nan") && lower.contains("vector") && lower.contains("must")
                && seen.insert("nan_vector".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_nan_vector".to_string(),
                    check_type: CheckType::ValueRange,
                    script: qdrant_nan_vector_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("duplicate") && lower.contains("collection") && lower.contains("conflict")
                && seen.insert("duplicate_collection".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_duplicate_collection".to_string(),
                    check_type: CheckType::ExistenceCheck,
                    script: qdrant_duplicate_collection_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }

            if lower.contains("distance") && lower.contains("invalid") && lower.contains("reject")
                && seen.insert("invalid_distance".to_string())
            {
                checks.push(InvariantCheck {
                    name: "oracle_assert_invalid_distance".to_string(),
                    check_type: CheckType::ValueRange,
                    script: qdrant_invalid_distance_check(),
                    source: InvariantSource::DerivedFromAssertion,
                });
            }
        }

        checks
    }
}

fn qdrant_search_probe(param: &str, value: i64, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
import time; time.sleep(0.5)
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{"points":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
body = {{"vector":[0.1,0.2,0.3,0.4],"limit":3}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json=body)
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

fn qdrant_search_params_probe(param: &str, value: i64, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
import time; time.sleep(0.5)
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{"points":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
body = {{"vector":[0.1,0.2,0.3,0.4],"limit":3,"params":{{"{param}":{value}}}}}
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json=body)
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

fn qdrant_search_float_probe(param: &str, value: f64, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
import time; time.sleep(0.5)
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{"points":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
body = {{"vector":[0.1,0.2,0.3,0.4],"limit":3}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json=body)
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

fn qdrant_search_string_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
import time; time.sleep(0.5)
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{"points":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
body = {{"vector":[0.1,0.2,0.3,0.4],"limit":3}}
body["{param}"] = "{value}"
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json=body)
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

fn qdrant_create_size_probe(size: i64, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_size_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":{size},"distance":"Cosine"}}}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        size = size,
        label = label,
    )
}

fn qdrant_create_shard_probe(shard: i64, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_shard_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}},"shard_number":{shard}}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        shard = shard,
        label = label,
    )
}

fn qdrant_create_replication_probe(factor: i64, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_repl_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}},"replication_factor":{factor}}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        factor = factor,
        label = label,
    )
}

fn qdrant_create_wcf_probe(factor: i64, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_wcf_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}},"write_consistency_factor":{factor}}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        factor = factor,
        label = label,
    )
}

fn qdrant_search_nonexistent_collection() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'nonexistent_' + uuid.uuid4().hex
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":3})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] search on nonexistent collection returned 200'); sys.exit(1)
else: print(f'properly rejected search on nonexistent collection: {r.status_code}'); sys.exit(0)"#.to_string()
}

fn qdrant_count_consistency_check() -> String {
    r#"import requests, sys, json, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_count_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
import time; time.sleep(0.5)
N = 5
points = [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(N)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200:
    print(f'upsert failed: {r.status_code} {r.text[:200]}'); sys.exit(0)
time.sleep(0.5)
r = requests.get(f'{BASE}/collections/{c}')
info = r.json().get('result', {})
count = info.get('points_count', -1)
if count != N:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] count mismatch: expected {N}, got {count}'); sys.exit(1)
else:
    print(f'count consistent: {count} == {N}'); sys.exit(0)"#.to_string()
}

fn qdrant_nan_vector_check() -> String {
    r#"import requests, sys, json, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_nan_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
import time; time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[float('nan'),0.2,0.3,0.4],"limit":3})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] NaN vector accepted'); sys.exit(1)
else: print(f'properly rejected NaN vector: {r.status_code}'); sys.exit(0)"#.to_string()
}

fn qdrant_duplicate_collection_check() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_dup_' + uuid.uuid4().hex[:8]
r1 = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r1.status_code not in (200, 201): print(f'setup failed: {r1.status_code}'); sys.exit(0)
import time; time.sleep(0.3)
r2 = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r2.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] duplicate collection name accepted (200)'); sys.exit(1)
else: print(f'properly rejected duplicate collection: {r2.status_code}'); sys.exit(0)"#.to_string()
}

fn qdrant_invalid_distance_check() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_dist_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"InvalidMetric"}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] invalid distance metric accepted'); sys.exit(1)
else: print(f'properly rejected invalid distance metric: {r.status_code}'); sys.exit(0)"#.to_string()
}
