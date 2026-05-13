pub mod schema;

use anyhow::Context;
use schema::{StructuredContract, EndpointRegistry};
use std::fs;
use std::path::Path;

pub fn load_endpoint_registry(path: &Path) -> anyhow::Result<EndpointRegistry> {
    let content = fs::read_to_string(path)
        .context("Failed to read endpoint registry file")?;
    let registry: EndpointRegistry = toml::from_str(&content)
        .context("Failed to parse endpoint registry TOML")?;
    Ok(registry)
}

/// Saves a structured contract to a JSON file.
pub fn save_contract_json<P: AsRef<Path>>(
    contract: &StructuredContract,
    path: P,
) -> anyhow::Result<()> {
    let json_string = serde_json::to_string_pretty(contract)
        .context("Failed to serialize contract to JSON")?;
    fs::write(path, json_string).context("Failed to write contract to file")?;
    Ok(())
}

/// Loads a structured contract from a JSON file.
pub fn load_contract_json<P: AsRef<Path>>(path: P) -> anyhow::Result<StructuredContract> {
    let file_content = fs::read_to_string(path).context("Failed to read contract file")?;
    let contract: StructuredContract = serde_json::from_str(&file_content)
        .context("Failed to deserialize contract from JSON")?;
    Ok(contract)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::RangeConstraint;
    use tempfile::tempdir;

    #[test]
    fn test_serialize_deserialize_contract() {
        let json_str = r#"{
            "api_endpoint": "create_collection",
            "doc_url": "https://milvus.io/docs/create_collection.md",
            "assertions": [
                "dimension must be > 0"
            ]
        }"#;

        let contract: StructuredContract = serde_json::from_str(json_str).unwrap();
        assert_eq!(contract.api_endpoint, "create_collection");
        assert_eq!(
            contract.doc_url,
            "https://milvus.io/docs/create_collection.md"
        );
        assert_eq!(contract.assertions.len(), 1);
        assert_eq!(contract.assertions[0], "dimension must be > 0");
    }

    #[test]
    fn test_save_and_load_contract() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("contract.json");

        let contract = StructuredContract {
            api_endpoint: "search".to_string(),
            doc_url: "https://qdrant.tech/documentation/search/".to_string(),
            assertions: vec!["top must be > 0".to_string()],
            type_constraints: vec![],
            range_constraints: vec![RangeConstraint {
                param_name: "top".to_string(),
                description: "top must be > 0".to_string(),
                min: Some("1".to_string()),
                max: None,
                violation_examples: vec!["0".to_string()],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
        };

        save_contract_json(&contract, &file_path).unwrap();
        let loaded = load_contract_json(&file_path).unwrap();

        assert_eq!(contract, loaded);
    }
}
