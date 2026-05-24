use crate::agent::classifier::DefectType;
use crate::agent::sandbox_runner::run_script_in_fresh_sandbox;
use crate::sandbox::manager::SidecarSpec;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamEffect {
    ConfirmedIgnored,
    Ambiguous,
    ActuallyApplied,
}

#[async_trait]
pub trait SemanticGateProvider: Send + Sync {
    fn target_name(&self) -> &str;
    async fn check_param_effect(
        &self,
        mre_code: &str,
        defect_type: &DefectType,
        db_image: &str,
        pip_packages: &[String],
        db_port: u16,
        sidecars: &[SidecarSpec],
        db_env: &[(String, String)],
        db_command: &[String],
    ) -> ParamEffect;
}

pub struct QdrantSemanticGate;

#[async_trait]
impl SemanticGateProvider for QdrantSemanticGate {
    fn target_name(&self) -> &str {
        "qdrant"
    }

    async fn check_param_effect(
        &self,
        mre_code: &str,
        defect_type: &DefectType,
        db_image: &str,
        pip_packages: &[String],
        db_port: u16,
        sidecars: &[SidecarSpec],
        db_env: &[(String, String)],
        db_command: &[String],
    ) -> ParamEffect {
        if defect_type != &DefectType::IllegalSuccess {
            return ParamEffect::Ambiguous;
        }

        let is_create = mre_code.contains("/collections/")
            && (mre_code.contains("requests.put") || mre_code.contains("requests.post"))
            && !mre_code.contains("/points")
            && !mre_code.contains("/search")
            && !mre_code.contains("/scroll")
            && !mre_code.contains("/recommend")
            && !mre_code.contains("update");

        if !is_create {
            info!("SemanticGate: not a create_collection probe, returning Ambiguous");
            return ParamEffect::Ambiguous;
        }

        let create_json = match extract_create_json(mre_code) {
            Some(j) => j,
            None => {
                info!("SemanticGate: could not extract create_collection JSON from MRE");
                return ParamEffect::Ambiguous;
            }
        };

        let script = generate_qdrant_verification_script(&create_json);

        let run = match run_script_in_fresh_sandbox(
            db_image,
            pip_packages,
            db_port,
            &script,
            "semantic_gate",
            sidecars,
            db_env,
            db_command,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("SemanticGate: sandbox execution failed: {}", e);
                return ParamEffect::Ambiguous;
            }
        };

        parse_semantic_gate_output(&run.stdout)
    }
}

// TODO: extract_create_json does not handle curly braces inside JSON string values
// (e.g. json={"name": "a{b}c"}). A proper fix would require a JSON-aware parser.
fn extract_create_json(mre_code: &str) -> Option<String> {
    let start = mre_code.find("json=")?;
    let json_start = start + 5;
    let bytes = mre_code.as_bytes();
    let mut depth = 0;
    let mut end = json_start;
    for i in json_start..bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 || end == json_start {
        return None;
    }
    Some(mre_code[json_start..end].to_string())
}

fn generate_qdrant_verification_script(create_json: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'sg_' + uuid.uuid4().hex[:8]
request_body = {create_json}
r = requests.put(f'{{BASE}}/collections/{{c}}', json=request_body)
if r.status_code != 200:
    print(f'SEMANTIC_GATE: REJECTED (status {{r.status_code}})')
    sys.exit(0)
green = False
for i in range(15):
    time.sleep(1)
    resp = requests.get(f'{{BASE}}/collections/{{c}}')
    result = resp.json().get('result', {{}})
    if result.get('status') == 'green':
        green = True
        break
if not green:
    print('SEMANTIC_GATE: AMBIGUOUS (timeout waiting for green)')
    sys.exit(0)
config = result.get('config', {{}})
params = config.get('params', {{}})
test_params = {{k: v for k, v in request_body.items() if k != 'vectors'}}
if not test_params:
    vectors_req = request_body.get('vectors', {{}})
    vectors_actual = params.get('vectors', {{}})
    if vectors_req == vectors_actual:
        print('SEMANTIC_GATE: ACTUALLY_APPLIED')
    else:
        print('SEMANTIC_GATE: CONFIRMED_IGNORED')
    sys.exit(0)
all_applied = True
any_applied = False
for key, value in test_params.items():
    actual = params.get(key)
    if actual is None:
        actual = config.get(key)
    if actual is None and key == 'optimizers_config':
        actual = config.get('optimizer_config')
    if actual is None and key == 'optimizer_config':
        actual = config.get('optimizers_config')
    if actual == value:
        any_applied = True
    else:
        all_applied = False
if all_applied:
    print('SEMANTIC_GATE: ACTUALLY_APPLIED')
elif not any_applied:
    print('SEMANTIC_GATE: CONFIRMED_IGNORED')
else:
    print('SEMANTIC_GATE: AMBIGUOUS')
"#,
        create_json = create_json,
    )
}

fn parse_semantic_gate_output(stdout: &str) -> ParamEffect {
    if stdout.contains("SEMANTIC_GATE: CONFIRMED_IGNORED") {
        ParamEffect::ConfirmedIgnored
    } else if stdout.contains("SEMANTIC_GATE: ACTUALLY_APPLIED") {
        ParamEffect::ActuallyApplied
    } else {
        ParamEffect::Ambiguous
    }
}

pub fn get_semantic_gate(target: &str) -> Option<Box<dyn SemanticGateProvider>> {
    match target {
        "qdrant" => Some(Box::new(QdrantSemanticGate)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_create_json_simple() {
        let mre = r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"},"shard_number":-1})"#;
        let result = extract_create_json(mre);
        assert!(result.is_some());
        let json = result.unwrap();
        assert!(json.contains("shard_number"));
        assert!(json.contains("-1"));
    }

    #[test]
    fn test_extract_create_json_nested() {
        let mre = r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"},"optimizers_config":{"indexing_threshold":0}})"#;
        let result = extract_create_json(mre);
        assert!(result.is_some());
        let json = result.unwrap();
        assert!(json.contains("optimizers_config"));
        assert!(json.contains("indexing_threshold"));
    }

    #[test]
    fn test_extract_create_json_vectors_only() {
        let mre = r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":0,"distance":"Cosine"}})"#;
        let result = extract_create_json(mre);
        assert!(result.is_some());
        let json = result.unwrap();
        assert!(json.contains("size"));
        assert!(json.contains("0"));
    }

    #[test]
    fn test_extract_create_json_no_json() {
        let mre = r#"r = requests.post(f'{BASE}/collections/{c}/points/search', json=body)"#;
        let result = extract_create_json(mre);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_semantic_gate_output_confirmed_ignored() {
        assert_eq!(
            parse_semantic_gate_output("SEMANTIC_GATE: CONFIRMED_IGNORED (requested -1, got 1)"),
            ParamEffect::ConfirmedIgnored
        );
    }

    #[test]
    fn test_parse_semantic_gate_output_actually_applied() {
        assert_eq!(
            parse_semantic_gate_output("SEMANTIC_GATE: ACTUALLY_APPLIED"),
            ParamEffect::ActuallyApplied
        );
    }

    #[test]
    fn test_parse_semantic_gate_output_ambiguous() {
        assert_eq!(
            parse_semantic_gate_output("SEMANTIC_GATE: AMBIGUOUS (timeout)"),
            ParamEffect::Ambiguous
        );
    }

    #[test]
    fn test_get_semantic_gate_qdrant() {
        let gate = get_semantic_gate("qdrant");
        assert!(gate.is_some());
        assert_eq!(gate.unwrap().target_name(), "qdrant");
    }

    #[test]
    fn test_get_semantic_gate_milvus() {
        let gate = get_semantic_gate("milvus");
        assert!(gate.is_none());
    }

    #[test]
    fn test_generate_verification_script_contains_placeholder() {
        let script = generate_qdrant_verification_script(
            r#"{"vectors":{"size":4,"distance":"Cosine"},"shard_number":-1}"#,
        );
        assert!(script.contains("{TESTVDB_DB_URL}"));
        assert!(script.contains("SEMANTIC_GATE:"));
        assert!(script.contains("shard_number"));
    }
}
