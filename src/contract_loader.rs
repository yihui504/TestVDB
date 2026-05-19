use std::fs;
use std::path::Path;
use tracing::{error, info, warn};
use anyhow::Context;

use crate::agent::llm::{DeepSeekClient, Message};
use crate::contract;
use crate::contract::schema::{EndpointEntry, EndpointRegistry, StructuredContract};
use crate::contract::store::{Confidence, ConstraintSource, ContractStore};
use crate::crawler::engine::{Crawler, ReqwestCrawler};
use crate::crawler::parser::{clean_content, extract_toc};
use crate::sandbox;

pub fn load_contracts(contracts_path: &str, target: &str, version: &str) -> anyhow::Result<Vec<StructuredContract>> {
    let path = Path::new(contracts_path);
    if !path.exists() {
        anyhow::bail!("Contracts path not found: {}", contracts_path);
    }
    let content = std::fs::read_to_string(path)?;
    let contracts: Vec<StructuredContract> = serde_json::from_str(&content)?;
    info!("Loaded {} contracts from {} (target={}, version={})", contracts.len(), contracts_path, target, version);
    Ok(contracts)
}

pub fn augment_contract(contract: &mut StructuredContract, target: &str) {
    let behavioral_path = format!("contracts/{}_behavioral_templates.json", target);
    let behavioral_templates_path = Path::new(&behavioral_path);
    if behavioral_templates_path.exists() {
        match contract::load_behavioral_templates(behavioral_templates_path) {
            Ok(templates) => {
                info!("Loaded {} behavioral contract templates for {}", templates.len(), target);
                contract.behavioral_contracts.extend(templates);
            }
            Err(e) => warn!("Failed to load behavioral templates for {}: {}", target, e),
        }
    }

    let mut merged_store = ContractStore::from_structured_contracts(
        target,
        "auto",
        std::slice::from_ref(&*contract),
        ConstraintSource::ExplicitDoc,
        Confidence::Medium,
    );

    let openapi_str = format!("contracts/{}_openapi.json", target);
    let openapi_path = Path::new(&openapi_str);
    if openapi_path.exists() {
        if let Ok(spec_content) = std::fs::read_to_string(openapi_path) {
            if let Ok(parser) = contract::openapi::OpenApiParser::from_json(&spec_content) {
                let openapi_store = parser.extract_to_contract_store(target, "auto");
                info!(
                    "OpenAPI store: {} type, {} range, {} required, {} enum",
                    openapi_store.type_constraints.len(),
                    openapi_store.range_constraints.len(),
                    openapi_store.required_params.len(),
                    openapi_store.enum_values.len(),
                );
                merged_store.merge(openapi_store);
            }
        }
    }

    if contract.range_constraints.is_empty() && contract.type_constraints.is_empty() {
        let (parsed_ranges, parsed_types) = contract::parse_constraints_from_assertions(&contract.assertions);
        if !parsed_ranges.is_empty() || !parsed_types.is_empty() {
            info!(
                "Parsed from assertions: {} range constraints, {} type constraints",
                parsed_ranges.len(),
                parsed_types.len()
            );
            contract.range_constraints = parsed_ranges;
            contract.type_constraints = parsed_types;
        }
    }

    if !merged_store.type_constraints.is_empty() && contract.type_constraints.is_empty() {
        contract.type_constraints = merged_store.type_constraints.iter().map(|atc| atc.constraint.clone()).collect();
    }
    if !merged_store.range_constraints.is_empty() && contract.range_constraints.is_empty() {
        contract.range_constraints = merged_store.range_constraints.iter().map(|arc| arc.constraint.clone()).collect();
    }

    for (endpoint, params) in &merged_store.required_params {
        for param in params {
            let tag = format!("[IMPLICIT:REQUIRED] {} is required", param);
            if !contract.assertions.iter().any(|a| a.contains(param) && a.starts_with("[IMPLICIT:REQUIRED]")) {
                contract.assertions.push(tag);
            }
        }
        let _ = endpoint;
    }
    for (param_name, values) in &merged_store.enum_values {
        let tag = format!("[IMPLICIT:ENUM] {} must be one of {:?}", param_name, values);
        if !contract.assertions.iter().any(|a| a.contains(param_name) && a.starts_with("[IMPLICIT:ENUM]")) {
            contract.assertions.push(tag);
        }
    }

    info!(
        "Augmented contract: {} assertions, {} type_constraints, {} range_constraints",
        contract.assertions.len(),
        contract.type_constraints.len(),
        contract.range_constraints.len(),
    );
}

const CONTRACT_SCHEMA_PROMPT: &str = r#"
You are a highly capable database testing agent.
Your task is to read the provided Markdown documentation for a vector database and extract API constraints into a STRICT JSON object.

## Critical Distinction
- HARD CONSTRAINT: Explicitly stated as required, forbidden, or mandatory. Example: "limit must be greater than 0" or "offset must be a non-negative integer".
- SOFT RECOMMENDATION: Phrased as a suggestion using words like "recommended", "should", "we suggest", "typically". Example: "it is recommended to use values between 10 and 100".
- ONLY extract HARD constraints. Do NOT extract recommendations or usage suggestions.
- Do NOT infer constraints from code examples alone — only extract constraints that the documentation explicitly states in prose.

## Schema
Do NOT wrap the JSON in Markdown code blocks.
The JSON must exactly match the following schema:
{
    "api_endpoint": "string (the name of the API, e.g., create_collection)",
    "doc_url": "string (the URL provided to you)",
    "parameters": {
        "param_name": "type (e.g., string, int)"
    },
    "data_types": {
        "field_name": "allowed_values (e.g., positive integer, non-empty string)"
    },
    "assertions": [
        "string (HARD constraints only, e.g., 'limit must be > 0')"
    ]
}

## Confidence
For each assertion, ask yourself: "Is this constraint explicitly stated in the prose, or am I inferring it from a code example?" If you are inferring, do NOT include it.
"#;

pub async fn run_extract(target: &str, docs_url: &str, out_dir: &str) -> anyhow::Result<()> {
    info!("Starting contract extraction for target: {}", target);
    fs::create_dir_all(out_dir)?;

    let crawler = ReqwestCrawler::new();
    let html = crawler.fetch_page(docs_url).await?;
    let toc_links = extract_toc(&html);
    info!("Extracted {} TOC links.", toc_links.len());

    let link_to_process = resolve_first_link(&toc_links, docs_url);
    info!("Processing page: {}", link_to_process);
    let page_html = crawler.fetch_page(&link_to_process).await?;
    let markdown = clean_content(&page_html);

    let llm_client = DeepSeekClient::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e))?;

    let messages = vec![
        Message::system(CONTRACT_SCHEMA_PROMPT.to_string()),
        Message::user(format!("Doc URL: {}\n\nMarkdown Content:\n{}", link_to_process, markdown)),
    ];

    info!("Calling DeepSeek API to extract contract...");
    let json_response = llm_client.send_chat_json_mode(messages).await?;

    match serde_json::from_str::<StructuredContract>(&json_response) {
        Ok(contract) => {
            let file_path = Path::new(out_dir).join(format!("{}_contract.json", target));
            contract::save_contract_json(&contract, &file_path)?;
            info!("Successfully extracted and saved contract to {:?}", file_path);
        }
        Err(e) => {
            error!("Raw LLM Output:\n{}", json_response);
            anyhow::bail!("Failed to parse LLM JSON output into StructuredContract: {}", e);
        }
    }
    Ok(())
}

pub fn resolve_first_link(toc_links: &[String], docs_url: &str) -> String {
    if toc_links.is_empty() {
        warn!("No TOC links found. Falling back to the entry page.");
        return docs_url.to_string();
    }
    let first_link = &toc_links[0];
    if first_link.starts_with("http") {
        first_link.to_string()
    } else if first_link.starts_with('/') {
        let parts: Vec<&str> = docs_url.splitn(4, '/').collect();
        let origin = if parts.len() >= 3 {
            format!("{}//{}", parts[0], parts[2])
        } else {
            docs_url.trim_end_matches('/').to_string()
        };
        format!("{}{}", origin, first_link)
    } else {
        format!("{}/{}", docs_url.trim_end_matches('/'), first_link)
    }
}

pub async fn run_knowledge_agent(
    llm_client: &DeepSeekClient,
    target: &str,
    version: &str,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
) -> anyhow::Result<String> {
    use contract::schema::{EndpointRegistry, EndpointEntry};

    let repo = repo_url.as_ref().context("repo_url is required when contracts directory is not provided")?;
    let docs = docs_url.as_ref().context("docs_url is required when contracts directory is not provided")?;

    info!("Starting Knowledge Agent Phase... Target Repo: {}", repo);

    let registry_file = format!(".trae/endpoints/{}.toml", target);
    let registry_path = Path::new(&registry_file);
    let registry = if registry_path.exists() {
        contract::load_endpoint_registry(registry_path)?
    } else {
        EndpointRegistry {
            target: target.to_string(),
            version: version.to_string(),
            endpoints: vec![EndpointEntry {
                name: target.to_string(),
                api_path: "search".to_string(),
                docs_url: docs.clone(),
                category: "points".to_string(),
            }],
        }
    };

    let kw_sandbox = sandbox::manager::Sandbox::create_knowledge_worker("ubuntu:latest", &["git", "curl", "grep", "ca-certificates"]).await?;
    let clone_result = kw_sandbox.exec_command(&["git", "clone", repo, "/workspace"]).await?;
    if !clone_result.success {
        warn!("Git clone may have failed: {}", clone_result.stderr);
    }

    let mut all_contracts: Vec<StructuredContract> = Vec::new();
    for entry in &registry.endpoints {
        info!("KA: extracting contract for '{}' ({})", entry.name, entry.api_path);
        match crate::agent::engine::knowledge_exploration_loop(llm_client, &kw_sandbox, target, repo, &entry.name, &entry.api_path, &entry.docs_url, 8).await {
            Ok(contract) => {
                info!("Contract for '{}': {} assertions", entry.name, contract.assertions.len());
                all_contracts.push(contract);
            }
            Err(e) => warn!("Failed to extract contract for '{}': {}", entry.name, e),
        }
    }

    kw_sandbox.cleanup().await?;
    if all_contracts.is_empty() {
        anyhow::bail!("Knowledge Agent failed to extract any contracts from {} endpoints.", registry.endpoints.len());
    }

    let generated_contract = contract::merge_contracts_from_ka(&all_contracts, docs);
    fs::create_dir_all(".trae/auto_contracts").unwrap_or_default();
    let contract_path = format!(".trae/auto_contracts/{}_contract.json", target);
    let json_str = serde_json::to_string_pretty(&generated_contract)?;
    fs::write(&contract_path, &json_str)?;
    info!("Saved auto-generated contract to {}", contract_path);

    Ok(json_str)
}

pub async fn load_contract_content(
    contracts_dir: &Option<String>,
    target: &str,
    version: &str,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
    llm_client: &DeepSeekClient,
) -> anyhow::Result<String> {
    if let Some(dir) = contracts_dir {
        info!("Loading contracts from: {}", dir);
        let contract_path = Path::new(dir).join(format!("{}_contract.json", target));
        if !contract_path.exists() {
            anyhow::bail!("Contract file not found: {:?}", contract_path);
        }
        Ok(fs::read_to_string(&contract_path)?)
    } else {
        run_knowledge_agent(llm_client, target, version, repo_url, docs_url).await
    }
}

pub fn build_contract_store(
    contract: &StructuredContract,
    target: &str,
    version: &str,
) -> contract::store::ContractStore {
    let mut store = contract::store::ContractStore::from_structured_contracts(
        target,
        version,
        std::slice::from_ref(contract),
        contract::store::ConstraintSource::ExplicitDoc,
        contract::store::Confidence::Medium,
    );

    let openapi_str = format!("contracts/{}_openapi.json", target);
    let openapi_path = Path::new(&openapi_str);
    if openapi_path.exists() {
        if let Ok(spec_content) = std::fs::read_to_string(openapi_path) {
            if let Ok(parser) = contract::openapi::OpenApiParser::from_json(&spec_content) {
                let openapi_store = parser.extract_to_contract_store(target, version);
                info!(
                    "OpenAPI augmentation: +{} type, +{} range, +{} required, +{} enum",
                    openapi_store.type_constraints.len(),
                    openapi_store.range_constraints.len(),
                    openapi_store.required_params.len(),
                    openapi_store.enum_values.len(),
                );
                store.merge(openapi_store);
            }
        }
    }

    store
}
