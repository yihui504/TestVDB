use crate::target::SafetyNet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndpointType {
    Search,
    Create,
    Upsert,
    Delete,
    Scroll,
    Recommend,
    Config,
}

pub fn classify_endpoint_type(param_name: &str, api_endpoint: &str) -> EndpointType {
    let ep_lower = api_endpoint.to_lowercase();
    let p_lower = param_name.to_lowercase();

    let config_params = [
        "quantization_config", "optimizers_config", "optimizer_config",
        "wal_config", "hnsw_config", "storage_config", "cluster_config",
        "collection_config",
    ];
    if config_params.iter().any(|c| p_lower.contains(c)) {
        return EndpointType::Config;
    }

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
        EndpointType::Config
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

pub fn dot_to_nested_json(dotted_path: &str, value: &str) -> String {
    let parts: Vec<&str> = dotted_path.split('.').collect();
    if parts.len() == 1 {
        return format!(r#""{}":{}"#, parts[0], value);
    }
    let leaf = format!(r#"{{"{}":{}}}"#, parts.last().expect("parts has >=2 elements when len != 1"), value);
    let mut json = leaf;
    for part in parts[..parts.len() - 1].iter().rev() {
        json = format!(r#"{{"{}":{}}}"#, part, json);
    }
    json
}

pub fn strip_endpoint_prefix<'a>(param: &'a str) -> (&'a str, &'a str) {
    let prefixes = [
        "create_collection.", "search_points.", "upsert_points.",
        "delete_points.", "scroll_points.", "recommend_points.",
    ];
    for prefix in &prefixes {
        if let Some(rest) = param.strip_prefix(prefix) {
            return (prefix.trim_end_matches('.'), rest);
        }
    }
    ("", param)
}

pub fn assemble_script(preamble: &str, setup_lines: &[&str], test_step: &str) -> String {
    let mut out = String::with_capacity(
        preamble.len() + 1 + setup_lines.iter().map(|s| s.len() + 1).sum::<usize>() + test_step.len()
    );
    out.push_str(preamble);
    out.push('\n');
    for line in setup_lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(test_step);
    out
}

pub fn generate_probe(endpoint_type: EndpointType, param: &str, value: &str, label: &str) -> String {
    let template = QdrantProbeTemplate;
    if is_search_param(param) {
        template.search_params_probe(param, value, label)
    } else {
        match endpoint_type {
            EndpointType::Search => template.search_probe(param, value, label),
            EndpointType::Create => template.create_probe(param, value, label),
            EndpointType::Upsert => template.upsert_probe(param, value, label),
            EndpointType::Delete => template.delete_probe(param, value, label),
            EndpointType::Scroll => template.scroll_probe(param, value, label),
            EndpointType::Recommend => template.recommend_probe(param, value, label),
            EndpointType::Config => {
                let parsed = crate::contract::store::parse_param_name(param);
                match parsed.endpoint.as_str() {
                    "create_collection" => template.create_probe(param, value, label),
                    _ => template.update_config_probe(param, value, label),
                }
            }
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
        _ => {
            let parsed = crate::contract::store::parse_param_name(param);
            let json_path = &parsed.json_path;
            if json_path == "vectors.size" {
                format!(r#"{{"vectors":{{"size":{},"distance":"Cosine"}}}}"#, value)
            } else {
                let nested = dot_to_nested_json(json_path, value);
                format!(r#"{{"vectors":{{"size":4,"distance":"Cosine"}},{}}}"#, nested)
            }
        }
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
    r#"import requests, json, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_inf_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.3)
try:
    body = json.dumps({"vector":[float('inf'),0.2,0.3,0.4],"limit":3})
    r = requests.post(f'{BASE}/collections/{c}/points/search', data=body, headers={"Content-Type":"application/json"})
    if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS] Infinity vector accepted'); sys.exit(1)
    else: print(f'Infinity vector properly rejected: {r.status_code}'); sys.exit(0)
except (ValueError, TypeError):
    print('Infinity vector not JSON-serializable, properly rejected'); sys.exit(0)"#.to_string()
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
    use regex::Regex;

    #[derive(Debug, PartialEq, Eq)]
    struct ProbeSemantics {
        request_endpoints: Vec<String>,
        defect_markers: Vec<String>,
        params_tested: Vec<String>,
    }

    fn normalize_url(url: &str) -> String {
        url.replace("{BASE}", "")
            .replace("{c}", "{coll}")
            .trim_start_matches('/')
            .to_string()
    }

    fn extract_probe_semantics(script: &str) -> ProbeSemantics {
        let request_re = Regex::new(r"requests\.(get|post|put|delete|patch)\(f?'([^']*)'").unwrap();
        let defect_re = Regex::new(r"\[DEFECT:\s*([A-Z_]+)\]").unwrap();
        let body_param_re = Regex::new(r#"body\["(\w+)"\]"#).unwrap();

        let request_endpoints: Vec<String> = request_re
            .captures_iter(script)
            .filter_map(|c| {
                let method = c[1].to_uppercase();
                let url = normalize_url(&c[2]);
                if url.is_empty() { None } else { Some(format!("{} {}", method, url)) }
            })
            .collect();

        let defect_markers: Vec<String> = defect_re
            .captures_iter(script)
            .map(|c| c[1].to_string())
            .collect();

        let mut params_tested: Vec<String> = Vec::new();
        for c in body_param_re.captures_iter(script) {
            params_tested.push(c[1].to_string());
        }
        params_tested.sort();
        params_tested.dedup();

        ProbeSemantics {
            request_endpoints,
            defect_markers,
            params_tested,
        }
    }

    fn probes_semantically_equivalent(a: &str, b: &str) -> bool {
        extract_probe_semantics(a) == extract_probe_semantics(b)
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

    #[test]
    fn test_qdrant_template_search_probe_golden() {
        let template = QdrantProbeTemplate;
        let from_template = template.search_probe("limit", "0", "limit=0");
        let from_function = search_probe("limit", "0", "limit=0");
        assert!(
            probes_semantically_equivalent(&from_template, &from_function),
            "QdrantProbeTemplate::search_probe must be semantically equivalent to search_probe()\n  template: {:?}\n  function: {:?}",
            extract_probe_semantics(&from_template),
            extract_probe_semantics(&from_function)
        );
    }

    #[test]
    fn test_milvus_template_search_probe_golden() {
        let template = MilvusProbeTemplate;
        let from_template = template.search_probe("limit", "0", "limit=0");
        let from_function = crate::agent::probe_milvus::milvus_search_probe("limit", "0", "limit=0");
        assert!(
            probes_semantically_equivalent(&from_template, &from_function),
            "MilvusProbeTemplate::search_probe must be semantically equivalent to milvus_search_probe()\n  template: {:?}\n  function: {:?}",
            extract_probe_semantics(&from_template),
            extract_probe_semantics(&from_function)
        );
    }

    #[test]
    fn test_qdrant_update_config_probe_contains_patch() {
        let template = QdrantProbeTemplate;
        let script = template.update_config_probe("optimizer_config", "{}", "test");
        assert!(script.contains("PATCH") || script.contains("patch"), "update_config_probe must use PATCH method");
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_classify_config_params_returns_config() {
        assert_eq!(classify_endpoint_type("quantization_config", "search"), EndpointType::Config);
        assert_eq!(classify_endpoint_type("optimizer_config", "create"), EndpointType::Config);
        assert_eq!(classify_endpoint_type("wal_config", "unknown"), EndpointType::Config);
        assert_eq!(classify_endpoint_type("hnsw_config", "search"), EndpointType::Config);
        assert_eq!(classify_endpoint_type("storage_config", "create"), EndpointType::Config);
        assert_eq!(classify_endpoint_type("cluster_config", ""), EndpointType::Config);
        assert_eq!(classify_endpoint_type("collection_config", ""), EndpointType::Config);
    }

    #[test]
    fn test_assemble_script_basic() {
        let script = assemble_script("preamble", &["line1", "line2"], "test_step");
        assert_eq!(script, "preamble\nline1\nline2\ntest_step");
    }

    #[test]
    fn test_dot_to_nested_json_simple() {
        assert_eq!(dot_to_nested_json("limit", "0"), r#""limit":0"#);
    }

    #[test]
    fn test_dot_to_nested_json_two_levels() {
        let result = dot_to_nested_json("optimizers_config.indexing_threshold", "0");
        assert!(result.contains(r#""optimizers_config":"#));
        assert!(result.contains(r#""indexing_threshold":0"#));
    }

    #[test]
    fn test_dot_to_nested_json_three_levels() {
        let result = dot_to_nested_json("quantization_config.scalar.quantile", "1.0");
        assert!(result.contains(r#""quantization_config":"#));
        assert!(result.contains(r#""scalar":"#));
        assert!(result.contains(r#""quantile":1.0"#));
    }

    #[test]
    fn test_strip_endpoint_prefix_create_collection() {
        let (prefix, rest) = strip_endpoint_prefix("create_collection.optimizers_config.indexing_threshold");
        assert_eq!(prefix, "create_collection");
        assert_eq!(rest, "optimizers_config.indexing_threshold");
    }

    #[test]
    fn test_create_probe_nested_json_correct() {
        let script = create_probe("create_collection.optimizers_config.indexing_threshold", "0", "indexing_threshold=0");
        assert!(script.contains(r#""optimizers_config":{"indexing_threshold":0}"#));
        assert!(!script.contains("create_collection.optimizers_config"));
    }

    #[test]
    fn test_create_probe_top_level_param_unchanged() {
        let script = create_probe("shard_number", "0", "shard_number=0");
        assert!(script.contains(r#""shard_number":0"#));
    }

    #[test]
    fn test_shard_number_with_prefix_generates_correct_mre() {
        let script = create_probe("create_collection.shard_number", "0", "shard_number=0");
        println!("=== SHARD_NUMBER MRE ===\n{}\n=== END ===", script);
        assert!(script.contains(r#""shard_number":0"#));
        assert!(!script.contains("create_collection.shard_number"));
    }

    #[test]
    fn test_vectors_size_with_prefix_generates_correct_mre() {
        let script = create_probe("create_collection.vectors.size", "0", "vectors.size=0");
        println!("=== VECTORS_SIZE MRE ===\n{}\n=== END ===", script);
        assert!(script.contains(r#""vectors":{"size":0"#));
        assert!(script.contains(r#""distance":"Cosine"#));
        assert!(!script.contains("create_collection.vectors.size"));
        assert!(!script.contains(r#""vectors":{"size":4,"distance":"Cosine"},"vectors"#));

    }

    #[test]
    fn test_replication_factor_with_prefix_generates_correct_mre() {
        let script = create_probe("create_collection.replication_factor", "0", "replication_factor=0");
        println!("=== REPLICATION_FACTOR MRE ===\n{}\n=== END ===", script);
        assert!(script.contains(r#""replication_factor":0"#));
        assert!(!script.contains("create_collection.replication_factor"));
    }

    #[test]
    fn test_nested_params_generate_correct_json_full_script() {
        let script = create_probe(
            "create_collection.optimizers_config.indexing_threshold",
            "0",
            "indexing_threshold=0"
        );
        println!("=== GENERATED SCRIPT ===\n{}\n=== END SCRIPT ===", script);
        assert!(script.contains(r#""optimizers_config":{"indexing_threshold":0}"#));
        assert!(!script.contains("create_collection.optimizers_config.indexing_threshold"));
    }

    #[test]
    fn test_parse_param_name_matches_strip_endpoint_prefix() {
        use crate::contract::store::parse_param_name;
        let test_cases = [
            "create_collection.optimizers_config.indexing_threshold",
            "search_points.limit",
            "upsert_points.points",
            "delete_points.ids",
            "scroll_points.offset",
            "recommend_points.limit",
            "limit",
        ];
        for tc in &test_cases {
            let parsed = parse_param_name(tc);
            let (prefix, rest) = strip_endpoint_prefix(tc);
            assert_eq!(parsed.endpoint, prefix, "endpoint mismatch for {}", tc);
            assert_eq!(parsed.json_path, rest, "json_path mismatch for {}", tc);
        }
    }

    #[test]
    fn test_parse_param_name_non_prefix_dotted() {
        use crate::contract::store::parse_param_name;
        let parsed = parse_param_name("hnsw_config.ef_construct");
        assert_eq!(parsed.endpoint, "hnsw_config");
        assert_eq!(parsed.json_path, "ef_construct");

        let parsed2 = parse_param_name("optimizers_config.indexing_threshold");
        assert_eq!(parsed2.endpoint, "optimizers_config");
        assert_eq!(parsed2.json_path, "indexing_threshold");
    }

    #[test]
    fn test_semantic_equivalence_tolerates_judgment_change() {
        let original = search_probe("limit", "0", "limit=0");
        let modified = original.replace("if r.status_code == 200", "if r.status_code in (200, 201)");
        assert!(
            probes_semantically_equivalent(&original, &modified),
            "Changing judgment logic should not break semantic equivalence"
        );
    }

    #[test]
    fn test_semantic_equivalence_rejects_different_probes() {
        let search_script = search_probe("limit", "0", "limit=0");
        let create_script = create_probe("shard_number", "0", "shard_number=0");
        assert!(
            !probes_semantically_equivalent(&search_script, &create_script),
            "search probe and create probe must NOT be semantically equivalent"
        );
    }

    #[test]
    fn test_extract_probe_semantics_search_probe() {
        let script = search_probe("limit", "0", "limit=0");
        let sem = extract_probe_semantics(&script);
        assert!(sem.defect_markers.contains(&"ILLEGAL_SUCCESS".to_string()));
        assert!(sem.request_endpoints.iter().any(|ep| ep.contains("points/search")));
        assert!(sem.params_tested.contains(&"limit".to_string()));
    }

    #[test]
    fn test_extract_probe_semantics_known_bug_shard_number() {
        let script = create_probe("shard_number", "-1", "shard_number=-1");
        let sem = extract_probe_semantics(&script);
        assert!(sem.defect_markers.contains(&"ILLEGAL_SUCCESS".to_string()));
        assert!(sem.request_endpoints.iter().any(|ep| ep.contains("collections")));
        assert!(!sem.request_endpoints.iter().any(|ep| ep.contains("points/search")));
        assert!(sem.params_tested.is_empty());
    }
}

pub trait ProbeTemplate {
    fn search_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn create_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn upsert_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn delete_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn scroll_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn recommend_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn search_params_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn update_config_probe(&self, param: &str, value: &str, label: &str) -> String;
    fn preamble(&self) -> &str;
}

pub struct QdrantProbeTemplate;

impl ProbeTemplate for QdrantProbeTemplate {
    fn search_probe(&self, param: &str, value: &str, label: &str) -> String {
        search_probe(param, value, label)
    }
    fn create_probe(&self, param: &str, value: &str, label: &str) -> String {
        create_probe(param, value, label)
    }
    fn upsert_probe(&self, param: &str, value: &str, label: &str) -> String {
        upsert_probe(param, value, label)
    }
    fn delete_probe(&self, param: &str, value: &str, label: &str) -> String {
        delete_probe(param, value, label)
    }
    fn scroll_probe(&self, param: &str, value: &str, label: &str) -> String {
        scroll_probe(param, value, label)
    }
    fn recommend_probe(&self, param: &str, value: &str, label: &str) -> String {
        recommend_probe(param, value, label)
    }
    fn search_params_probe(&self, param: &str, value: &str, label: &str) -> String {
        search_params_probe(param, value, label)
    }
    fn update_config_probe(&self, param: &str, value: &str, _label: &str) -> String {
        let parsed = crate::contract::store::parse_param_name(param);
        let json_path = &parsed.json_path;
        let nested = dot_to_nested_json(json_path, value);
        format!(
            r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":4,"distance":"Cosine"}}}})
if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)
import time; time.sleep(0.5)
r = requests.patch(f'{{BASE}}/collections/{{c}}', json={{{nested}}})
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {{r.status_code}} accepted config {param}={value}'); sys.exit(1)
else: print(f'properly rejected config {param}={value}: {{r.status_code}}'); sys.exit(0)"#,
            param = param,
            value = value,
            nested = nested,
        )
    }
    fn preamble(&self) -> &str {
        "import requests, sys, uuid, time"
    }
}

pub struct NoopProbeTemplate;

impl ProbeTemplate for NoopProbeTemplate {
    fn search_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn create_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn upsert_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn delete_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn scroll_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn recommend_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn search_params_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn update_config_probe(&self, _param: &str, _value: &str, _label: &str) -> String { String::new() }
    fn preamble(&self) -> &str { "" }
}

pub struct MilvusProbeTemplate;

impl MilvusProbeTemplate {
    const AUTH: &str = "'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'";
}

impl ProbeTemplate for MilvusProbeTemplate {
    fn search_probe(&self, param: &str, value: &str, label: &str) -> String {
        crate::agent::probe_milvus::milvus_search_probe(param, value, label)
    }
    fn create_probe(&self, param: &str, value: &str, label: &str) -> String {
        crate::agent::probe_milvus::milvus_create_probe(param, value, label)
    }
    fn upsert_probe(&self, param: &str, value: &str, label: &str) -> String {
        crate::agent::probe_milvus::milvus_insert_probe(param, value, label)
    }
    fn delete_probe(&self, param: &str, value: &str, label: &str) -> String {
        crate::agent::probe_milvus::milvus_query_probe(param, value, label)
    }
    fn scroll_probe(&self, _param: &str, _value: &str, _label: &str) -> String {
        String::new()
    }
    fn recommend_probe(&self, param: &str, value: &str, label: &str) -> String {
        crate::agent::probe_milvus::milvus_search_probe(param, value, label)
    }
    fn search_params_probe(&self, param: &str, value: &str, label: &str) -> String {
        crate::agent::probe_milvus::milvus_search_params_probe(param, value, label)
    }
    fn update_config_probe(&self, _param: &str, _value: &str, _label: &str) -> String {
        String::new()
    }
    fn preamble(&self) -> &str {
        "import requests, sys, uuid, time"
    }
}
