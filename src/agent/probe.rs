use crate::target::SafetyNet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndpointType {
    Search,
    Create,
    Upsert,
    Delete,
    Scroll,
    Recommend,
}

pub fn classify_endpoint_type(param_name: &str, api_endpoint: &str) -> EndpointType {
    let ep_lower = api_endpoint.to_lowercase();
    let p_lower = param_name.to_lowercase();

    if ep_lower.contains("search") || matches!(p_lower.as_str(), "limit" | "offset" | "score_threshold" | "with_payload" | "with_vector") {
        EndpointType::Search
    } else if ep_lower.contains("create") || matches!(p_lower.as_str(), "size" | "distance" | "shard_number" | "replication_factor" | "write_consistency_factor") {
        EndpointType::Create
    } else if ep_lower.contains("upsert") || matches!(p_lower.as_str(), "points" | "vectors") {
        EndpointType::Upsert
    } else if ep_lower.contains("delete") {
        EndpointType::Delete
    } else if ep_lower.contains("scroll") {
        EndpointType::Scroll
    } else if ep_lower.contains("recommend") {
        EndpointType::Recommend
    } else {
        EndpointType::Search
    }
}

pub fn is_search_param(param_name: &str) -> bool {
    let p = param_name.to_lowercase();
    p == "hnsw_ef" || p == "exact" || p == "quantization" || p == "oversampling"
}

pub fn format_boundary(v: f64) -> String {
    if v == v.floor() && v.abs() < 1e10 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

pub fn generate_probe(endpoint_type: EndpointType, param: &str, value: &str, label: &str) -> String {
    if is_search_param(param) {
        search_params_probe(param, value, label)
    } else {
        match endpoint_type {
            EndpointType::Search => search_probe(param, value, label),
            EndpointType::Create => create_probe(param, value, label),
            EndpointType::Upsert => upsert_probe(param, value, label),
            EndpointType::Delete => delete_probe(param, value, label),
            EndpointType::Scroll => scroll_probe(param, value, label),
            EndpointType::Recommend => recommend_probe(param, value, label),
        }
    }
}

pub fn search_probe(param: &str, value: &str, label: &str) -> String {
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
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {{r.status_code}} accepted {label}'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

pub fn search_params_probe(param: &str, value: &str, _label: &str) -> String {
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
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {{r.json()}} accepted'); sys.exit(1)
else: print(f'properly rejected: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
    )
}

pub fn create_probe(param: &str, value: &str, label: &str) -> String {
    let create_json = match param {
        "vectors.size" | "size" => format!(r#"{{"vectors":{{"size":{},"distance":"Cosine"}}}}"#, value),
        "shard_number" => format!(r#"{{"vectors":{{"size":4,"distance":"Cosine"}},"shard_number":{}}}"#, value),
        "replication_factor" => format!(r#"{{"vectors":{{"size":4,"distance":"Cosine"}},"replication_factor":{}}}"#, value),
        "write_consistency_factor" => format!(r#"{{"vectors":{{"size":4,"distance":"Cosine"}},"write_consistency_factor":{}}}"#, value),
        _ => format!(r#"{{"vectors":{{"size":4,"distance":"Cosine"}},"{param}":{value}}}"#, param = param, value = value),
    };

    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={create_json})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        create_json = create_json,
        label = label,
    )
}

pub fn upsert_probe(param: &str, value: &str, _label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
import time; time.sleep(0.5)
body = {{"points":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}}
body["{param}"] = {value}
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] string value for {param} accepted'); sys.exit(1)
else: print(f'properly rejected string value for {param}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
    )
}

pub fn delete_probe(param: &str, value: &str, _label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
import time; time.sleep(0.5)
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{"points":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}},{{"id":2,"vector":[0.5,0.6,0.7,0.8]}}]}})
if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)
time.sleep(0.3)
body = {{"points":[1]}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/collections/{{c}}/points/delete', json=body)
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] string value for {param} accepted'); sys.exit(1)
else: print(f'properly rejected string value for {param}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
    )
}

pub fn scroll_probe(param: &str, value: &str, label: &str) -> String {
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
body = {{"limit":10}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/collections/{{c}}/points/scroll', json=body)
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

pub fn recommend_probe(param: &str, value: &str, label: &str) -> String {
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
body = {{"positive":[1],"limit":3}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/collections/{{c}}/points/recommend', json=body)
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.status_code}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

pub fn search_string_probe(param: &str, value: &str, label: &str) -> String {
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

pub fn search_nonexistent_collection() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'nonexistent_' + uuid.uuid4().hex
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":3})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] search on nonexistent collection returned 200'); sys.exit(1)
else: print(f'properly rejected search on nonexistent collection: {r.status_code}'); sys.exit(0)"#.to_string()
}

pub fn count_consistency_check() -> String {
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

pub fn nan_vector_check() -> String {
    r#"import requests, sys, json, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_nan_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
import time; time.sleep(0.5)
body = '{"vector":[NaN,0.2,0.3,0.4],"limit":3}'
r = requests.post(f'{BASE}/collections/{c}/points/search', data=body, headers={'Content-Type':'application/json'})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] NaN vector accepted'); sys.exit(1)
else: print(f'properly rejected NaN vector: {r.status_code}'); sys.exit(0)"#.to_string()
}

pub fn duplicate_collection_check() -> String {
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

pub fn invalid_distance_check() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_dist_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"InvalidMetric"}})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] invalid distance metric accepted'); sys.exit(1)
else: print(f'properly rejected invalid distance metric: {r.status_code}'); sys.exit(0)"#.to_string()
}

pub fn inf_vector_search_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_inf_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[float('inf'),0.2,0.3,0.4],"limit":3})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] Infinity vector accepted'); sys.exit(1)
else: print(f'Infinity vector properly rejected: {r.status_code}'); sys.exit(0)"#.to_string()
}

pub fn empty_vector_search_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_empty_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[],"limit":3})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] empty vector accepted'); sys.exit(1)
else: print(f'empty vector properly rejected: {r.status_code}'); sys.exit(0)"#.to_string()
}

pub fn upsert_nan_vector_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_upnan_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
body = '{"points":[{"id":1,"vector":[NaN,0.2,0.3,0.4]}]}'
r = requests.put(f'{BASE}/collections/{c}/points', data=body, headers={'Content-Type':'application/json'})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] NaN vector accepted'); sys.exit(1)
else: print(f'NaN vector properly rejected: {r.status_code}'); sys.exit(0)"#.to_string()
}

pub fn upsert_inf_vector_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_upinf_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
body = '{"points":[{"id":1,"vector":[Infinity,0.2,0.3,0.4]}]}'
r = requests.put(f'{BASE}/collections/{c}/points', data=body, headers={'Content-Type':'application/json'})
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] Infinity vector accepted'); sys.exit(1)
else: print(f'Infinity vector properly rejected: {r.status_code}'); sys.exit(0)"#.to_string()
}

pub struct SimpleSafetyNet {
    pub name: String,
    pub endpoint_type: EndpointType,
    pub param: String,
    pub value: String,
    pub label: String,
    pub redundant_with_mutation: bool,
}

impl SimpleSafetyNet {
    pub fn to_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: generate_probe(self.endpoint_type, &self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_generate_probe_dispatches_correctly() {
        let script = generate_probe(EndpointType::Create, "replication_factor", "0", "replication_factor=0");
        assert!(script.contains("replication_factor"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_simple_safety_net_to_safety_net() {
        let spec = SimpleSafetyNet {
            name: "test_probe".to_string(),
            endpoint_type: EndpointType::Search,
            param: "limit".to_string(),
            value: "0".to_string(),
            label: "limit=0".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_safety_net();
        assert_eq!(sn.name, "test_probe");
        assert!(sn.script.contains("limit"));
        assert!(!sn.redundant_with_mutation);
    }
}
