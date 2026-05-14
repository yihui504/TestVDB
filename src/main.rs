mod cli;
pub mod agent;
pub mod contract;
pub mod crawler;
pub mod report;
pub mod sandbox;
pub mod review;
pub mod target;

use agent::classifier::ClassificationDisposition;
use agent::llm::{DeepSeekClient, Message};
use agent::orchestrator::FAOrchestrator;
use anyhow::Context;
use clap::Parser;
use cli::{Cli, Commands};
use crawler::engine::{Crawler, ReqwestCrawler};
use crawler::parser::{clean_content, extract_toc};
use report::verification::{self, VerificationOutcome};
use std::fs;
use std::path::Path;
use target::{QdrantPlugin, TargetRegistry};
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let cli = Cli::parse();

    match &cli.command {
        Commands::Extract { target, docs_url, out_dir } => {
            run_extract(target, docs_url, out_dir).await?;
        }
        Commands::Test { target, version, contracts, repo_url, docs_url, multi_defect } => {
            run_test(target, version, contracts, repo_url, docs_url, *multi_defect).await?;
        }
    }

    Ok(())
}

async fn run_extract(target: &str, docs_url: &str, out_dir: &str) -> anyhow::Result<()> {
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

    use contract::schema::StructuredContract;
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

fn resolve_first_link(toc_links: &[String], docs_url: &str) -> String {
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

async fn run_test(
    target: &str,
    version: &str,
    contracts: &Option<String>,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
    multi_defect: bool,
) -> anyhow::Result<()> {
    info!("Starting testing pipeline for target: {} version: {}", target, version);

    use contract::schema::StructuredContract;
    use report::generator::{CandidateDefect, CandidateStatus};

    let llm_client = DeepSeekClient::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e))?;

    let contract_content = if let Some(contracts_dir) = contracts {
        info!("Loading contracts from: {}", contracts_dir);
        let contract_path = Path::new(contracts_dir).join(format!("{}_contract.json", target));
        if !contract_path.exists() {
            anyhow::bail!("Contract file not found: {:?}", contract_path);
        }
        fs::read_to_string(&contract_path)?
    } else {
        run_knowledge_agent(&llm_client, target, version, repo_url, docs_url).await?
    };

    let mut contract: StructuredContract = serde_json::from_str(&contract_content)
        .context("Failed to parse contract JSON for testing")?;

    augment_contract(&mut contract);

    let contract_content = serde_json::to_string_pretty(&contract)
        .context("Failed to re-serialize contract with behavioral templates")?;

    let mut registry = TargetRegistry::new();
    registry.register(Box::new(QdrantPlugin));
    let plugin = registry.get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target database: {}. Available: {:?}", target, registry.available_targets()))?;

    let db_image = plugin.target_image(version);
    let pip_packages = plugin.pip_packages();
    let db_port = plugin.db_port();

    let orchestrator = FAOrchestrator::new(&llm_client, plugin, contract_content, version, 12, multi_defect);
    let (script_code, initial_run, initial_classification, collected_defects) = orchestrator.run().await?;

    let (script_code, initial_run, initial_classification) =
        if initial_classification.disposition == ClassificationDisposition::Pass && !collected_defects.is_empty() {
            warn!("Gatekeeper: Final MRE passed, but {} defect(s) collected. Processing first.", collected_defects.len());
            let first = collected_defects.into_iter().next().unwrap();
            (first.script, first.evidence, first.classification)
        } else {
            (script_code, initial_run, initial_classification)
        };

    match initial_classification.disposition {
        ClassificationDisposition::Pass => info!("Gatekeeper: No defects found."),
        ClassificationDisposition::CoverageDetected => info!("Gatekeeper: Coverage report only. {}", initial_classification.reason),
        ClassificationDisposition::NonDefectInfraError => warn!("Gatekeeper: Infrastructure issue. Reason: {}", initial_classification.reason),
        ClassificationDisposition::RetryableScriptError => warn!("Gatekeeper: Script error. Reason: {}", initial_classification.reason),
        ClassificationDisposition::CandidateDefect => {
            let defect_type = initial_classification.defect_type.clone()
                .context("Candidate defect is missing a defect type")?;
            warn!("Gatekeeper: Candidate defect detected ({:?}). Starting verification.", defect_type);

            let mut candidate = CandidateDefect {
                target: target.to_string(),
                version: version.to_string(),
                defect_type,
                doc_citation_url: contract.doc_url.clone(),
                contract_assertions: contract.assertions.clone(),
                surviving_assertions: contract.assertions.clone(),
                mre_code: script_code,
                initial_run,
                reproduction_runs: Vec::new(),
                status: CandidateStatus::Pending,
                downgrade_reason: None,
                independent_review_summary: None,
                review_scope: None,
            };

            let mre_code = candidate.mre_code.clone();
            let outcome = verification::verify_candidate_defect(
                &mut candidate, &mre_code, &db_image, &pip_packages, db_port, plugin, target,
            ).await?;

            match outcome {
                VerificationOutcome::Verified(report) => {
                    let report_path = verification::formal_report_output_path(target, &report.submission_grade_review.verdict);
                    report.export_to_markdown(&report_path)?;
                    match report.submission_grade_review.verdict {
                        report::generator::SubmissionGradeVerdict::SubmissionGrade => {
                            info!("Verified submission-grade bug report: {}", report_path);
                        }
                        report::generator::SubmissionGradeVerdict::NeedsRewrite => {
                            warn!("Report needs rewrite: {}", report_path);
                        }
                    }
                }
                VerificationOutcome::Rejected(reason) => warn!("Candidate defect rejected: {}", reason),
            }
        }
    }
    Ok(())
}

async fn run_knowledge_agent(
    llm_client: &DeepSeekClient,
    target: &str,
    version: &str,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
) -> anyhow::Result<String> {
    use contract::schema::{StructuredContract, EndpointRegistry, EndpointEntry};

    let repo = repo_url.as_ref().context("repo_url is required when contracts directory is not provided")?;
    let docs = docs_url.as_ref().context("docs_url is required when contracts directory is not provided")?;

    info!("Starting Knowledge Agent Phase... Target Repo: {}", repo);

    let registry_path = Path::new(".trae/endpoints/qdrant.toml");
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
        match agent::engine::knowledge_exploration_loop(llm_client, &kw_sandbox, target, repo, &entry.name, &entry.api_path, &entry.docs_url, 8).await {
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

fn augment_contract(contract: &mut contract::schema::StructuredContract) {
    let behavioral_templates_path = Path::new("contracts/qdrant_behavioral_templates.json");
    if behavioral_templates_path.exists() {
        match contract::load_behavioral_templates(behavioral_templates_path) {
            Ok(templates) => {
                info!("Loaded {} behavioral contract templates", templates.len());
                contract.behavioral_contracts.extend(templates);
            }
            Err(e) => warn!("Failed to load behavioral templates: {}", e),
        }
    }

    let openapi_path = Path::new("contracts/qdrant_openapi.json");
    if openapi_path.exists() {
        if let Ok(spec_content) = std::fs::read_to_string(openapi_path) {
            if let Ok(parser) = contract::openapi::OpenApiParser::from_json(&spec_content) {
                let type_constraints = parser.extract_all_type_constraints();
                let range_constraints = parser.extract_all_range_constraints();
                if !type_constraints.is_empty() || !range_constraints.is_empty() {
                    info!("OpenAPI: {} type, {} range constraints", type_constraints.len(), range_constraints.len());
                    contract.type_constraints = type_constraints;
                    contract.range_constraints = range_constraints;
                }
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
}

#[cfg(test)]
mod normalization_tests {
    use super::verification::formal_report_output_path;
    use crate::agent::classifier::{DefectType, normalize_observed_output};
    use crate::report::generator::SubmissionGradeVerdict;
    use crate::review::qdrant::{build_qdrant_search_poor_diagnostics_mre, IndependentProbeResult, summarize_qdrant_independent_probe};

    #[test]
    fn dedupes_same_issue_with_different_wording() {
        let output = "\
[DEFECT: POOR_DIAGNOSTICS] Error message does not mention limit must be > 0\n\
[DEFECT: POOR_DIAGNOSTICS] Limit constraint is missing from the error text\n\
[DEFECT: POOR_DIAGNOSTICS] Error message does not mention offset must be >= 0";
        let normalized = normalize_observed_output(output);
        let lines: Vec<&str> = normalized.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("limit"));
        assert!(lines[1].contains("offset"));
    }

    #[test]
    fn needs_rewrite_reports_use_different_output_path() {
        assert_eq!(formal_report_output_path("qdrant", &SubmissionGradeVerdict::SubmissionGrade), "qdrant_bug_report.md");
        assert_eq!(formal_report_output_path("qdrant", &SubmissionGradeVerdict::NeedsRewrite), "qdrant_report_needs_rewrite.md");
    }

    #[test]
    fn independent_probe_prefers_illegal_success() {
        let result = IndependentProbeResult {
            create_status: 200, create_body: "{}".to_string(),
            upsert_status: 200, upsert_body: "{}".to_string(),
            vector_status: 400, vector_body: "{\"status\":{\"error\":\"wrong vector size\"}}".to_string(),
            limit_status: 200, limit_body: "{}".to_string(),
            offset_status: 400, offset_body: "{\"status\":{\"error\":\"wrong offset\"}}".to_string(),
            hnsw_ef_status: 400, hnsw_ef_body: "{\"status\":{\"error\":\"invalid hnsw_ef\"}}".to_string(),
            ..Default::default()
        };
        let summary = summarize_qdrant_independent_probe(&result).expect("expected issue");
        assert_eq!(summary.0, DefectType::IllegalSuccess);
        assert!(summary.1.iter().any(|issue| issue.contains("limit=0 request succeeded")));
    }

    #[test]
    fn unexpected_status_is_not_treated_as_poor_diagnostics() {
        let result = IndependentProbeResult {
            create_status: 200, create_body: "{}".to_string(),
            upsert_status: 200, upsert_body: "{}".to_string(),
            vector_status: 500, vector_body: "{\"status\":{\"error\":\"internal error\"}}".to_string(),
            limit_status: 404, limit_body: "{\"status\":{\"error\":\"not found\"}}".to_string(),
            offset_status: 405, offset_body: "{\"status\":{\"error\":\"method not allowed\"}}".to_string(),
            hnsw_ef_status: 500, hnsw_ef_body: "{}".to_string(),
            ..Default::default()
        };
        assert!(summarize_qdrant_independent_probe(&result).is_none());
    }

    #[test]
    fn narrowed_limit_mre_accepts_positive_synonyms() {
        let mre = build_qdrant_search_poor_diagnostics_mre(&[
            "limit diagnostics do not clearly mention the limit constraint".to_string(),
        ]);
        assert!(mre.contains("\"limit\" not in r.text.lower()"));
    }

    #[test]
    fn narrowed_mre_restricts_poor_diagnostics_to_expected_validation_failures() {
        let mre = build_qdrant_search_poor_diagnostics_mre(&[
            "limit diagnostics do not clearly mention the limit constraint".to_string(),
            "offset diagnostics do not clearly mention the offset constraint".to_string(),
        ]);
        assert_eq!(mre.matches("[DEFECT: POOR_DIAGNOSTICS]").count(), 2);
    }

    #[test]
    fn hnsw_ef_zero_triggers_illegal_success() {
        let result = IndependentProbeResult {
            create_status: 200, create_body: "{}".to_string(),
            upsert_status: 200, upsert_body: "{}".to_string(),
            vector_status: 400, vector_body: "{\"status\":{\"error\":\"wrong vector size\"}}".to_string(),
            limit_status: 400, limit_body: "{\"status\":{\"error\":\"limit must be positive\"}}".to_string(),
            offset_status: 400, offset_body: "{\"status\":{\"error\":\"offset must be non-negative\"}}".to_string(),
            hnsw_ef_status: 200, hnsw_ef_body: "{\"result\":[]}".to_string(),
            ..Default::default()
        };
        let summary = summarize_qdrant_independent_probe(&result).expect("expected issue");
        assert_eq!(summary.0, DefectType::IllegalSuccess);
        assert!(summary.1.iter().any(|issue| issue.contains("hnsw_ef=0")));
    }
}
