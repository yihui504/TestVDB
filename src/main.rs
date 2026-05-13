mod cli;
pub mod agent;
pub mod contract;
pub mod crawler;
pub mod report;
pub mod sandbox;
pub mod review;
pub mod target;

use agent::classifier::{analyze_execution_result, ClassificationDisposition};
use agent::llm::{DeepSeekClient, Message};
use agent::orchestrator::FAOrchestrator;
use anyhow::Context;
use clap::Parser;
use cli::{Cli, Commands};
use crawler::engine::{Crawler, ReqwestCrawler};
use crawler::parser::{clean_content, extract_toc};
use std::fs;
use std::path::Path;
use target::{QdrantPlugin, TargetRegistry};
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;



fn formal_report_output_path(
    target: &str,
    verdict: &report::generator::SubmissionGradeVerdict,
) -> String {
    match verdict {
        report::generator::SubmissionGradeVerdict::SubmissionGrade => {
            format!("{}_bug_report.md", target)
        }
        report::generator::SubmissionGradeVerdict::NeedsRewrite => {
            format!("{}_report_needs_rewrite.md", target)
        }
    }
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

async fn run_script_in_fresh_sandbox(
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
    script_code: &str,
    phase: &str,
) -> anyhow::Result<report::generator::RunEvidence> {

    use sandbox::manager::Sandbox;

    let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
    let sandbox = Sandbox::create_network_and_containers(db_image, &pip_refs, db_port).await?;
    let db_url = format!("http://{}:{}", sandbox.db_host.as_ref().unwrap(), db_port);
    let rebound_script = script_code.replace("{{TESTVDB_DB_URL}}", &db_url);
    let output = sandbox.exec_command_with_env(&["python", "-c", &rebound_script], &[("TESTVDB_DB_URL", &db_url)]).await?;
    let normalized_stdout = agent::classifier::normalize_observed_output(&output.stdout);
    let normalized_stderr = agent::classifier::normalize_observed_output(&output.stderr);
    let classification = analyze_execution_result(&normalized_stdout, &normalized_stderr);

    Ok(report::generator::RunEvidence {
        phase: phase.to_string(),
        db_url,
        stdout: normalized_stdout,
        stderr: normalized_stderr,
        classifier_reason: classification.reason,
        classifier_evidence_excerpt: classification.evidence_excerpt,
    })
}

async fn refresh_candidate_evidence_with_mre(
    candidate: &mut report::generator::CandidateDefect,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
) -> anyhow::Result<Option<String>> {


    let phases = ["initial", "repro_1", "repro_2"];
    let mut refreshed_runs = Vec::new();
    let mut expected_excerpt: Option<String> = None;

    for phase in phases {
        let run = match run_script_in_fresh_sandbox(
            db_image,
            pip_packages,
            db_port,
            &candidate.mre_code,
            phase,
        )
        .await
        {
            Ok(run) => run,
            Err(err) => {
                candidate.status = report::generator::CandidateStatus::Rejected;
                candidate.downgrade_reason = Some(format!(
                    "{} could not be replayed after narrowing candidate evidence: {}",
                    phase, err
                ));
                if !refreshed_runs.is_empty() {
                    candidate.initial_run = refreshed_runs.remove(0);
                    candidate.reproduction_runs = refreshed_runs;
                }
                return Ok(candidate.downgrade_reason.clone());
            }
        };
        let classification = analyze_execution_result(&run.stdout, &run.stderr);

        if classification.disposition != ClassificationDisposition::CandidateDefect
            || classification.defect_type.as_ref() != Some(&candidate.defect_type)
        {
            candidate.status = report::generator::CandidateStatus::Rejected;
            candidate.downgrade_reason = Some(format!(
                "{} failed after narrowing candidate evidence: {}",
                phase, classification.reason
            ));
            refreshed_runs.push(run);
            candidate.initial_run = refreshed_runs.remove(0);
            candidate.reproduction_runs = refreshed_runs;
            return Ok(candidate.downgrade_reason.clone());
        }

        if let Some(expected) = &expected_excerpt {
            if classification.evidence_excerpt != *expected {
                candidate.status = report::generator::CandidateStatus::Rejected;
                candidate.downgrade_reason = Some(format!(
                    "{} produced a different evidence excerpt after narrowing candidate evidence.",
                    phase
                ));
                refreshed_runs.push(run);
                candidate.initial_run = refreshed_runs.remove(0);
                candidate.reproduction_runs = refreshed_runs;
                return Ok(candidate.downgrade_reason.clone());
            }
        } else {
            expected_excerpt = Some(classification.evidence_excerpt.clone());
        }

        refreshed_runs.push(run);
    }

    candidate.initial_run = refreshed_runs.remove(0);
    candidate.reproduction_runs = refreshed_runs;

    Ok(None)
}

async fn run_additional_reproduction(
    candidate: &mut report::generator::CandidateDefect,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
) -> anyhow::Result<Option<String>> {

    let run = match run_script_in_fresh_sandbox(
        db_image,
        pip_packages,
        db_port,
        &candidate.mre_code,
        "independent_review",
    )
    .await
    {
        Ok(run) => run,
        Err(err) => {
            return Ok(Some(format!(
                "Independent replay of the stabilized MRE could not complete cleanly: {}",
                err
            )));
        }
    };
    let classification = agent::classifier::analyze_execution_result(&run.stdout, &run.stderr);
    if classification.disposition != ClassificationDisposition::CandidateDefect
        || classification.defect_type.as_ref() != Some(&candidate.defect_type)
    {
        return Ok(Some(format!(
            "Independent replay of the stabilized MRE did not confirm the verified finding: {}",
            classification.reason
        )));
    }

    candidate.independent_review_summary = Some(format!(
        "Fresh post-verification replay of the stabilized MRE reproduced {:?} with the same evidence excerpt.",
        candidate.defect_type
    ));
    candidate.review_scope = Some(
        "Independent review reran the stabilized final MRE in a fresh sandbox after double reproduction, outside the initial promotion loop."
            .to_string(),
    );

    Ok(None)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing (logging)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let cli = Cli::parse();

    match &cli.command {
        Commands::Extract { target, docs_url, out_dir } => {
            info!("Starting contract extraction for target: {}", target);
            info!("Docs URL: {}", docs_url);
            info!("Output Directory: {}", out_dir);
            
            // 1. Ensure out_dir exists
            fs::create_dir_all(out_dir)?;

            // 2. Fetch entry page
            let crawler = ReqwestCrawler::new(); // Using Reqwest directly here for the CLI fast-path, or Agent could use Chromium
            info!("Fetching entry page...");
            let html = crawler.fetch_page(docs_url).await?;

            // 3. Extract TOC
            let toc_links = extract_toc(&html);
            info!("Extracted {} TOC links.", toc_links.len());

            // 4. Mock extracting the first link as a proof of concept
            let link_to_process = if !toc_links.is_empty() {
                // In a real scenario, we need to resolve relative URLs
                let first_link = &toc_links[0];
                let resolved_url = if first_link.starts_with("http") {
                    first_link.to_string()
                } else if first_link.starts_with('/') {
                    // Extract origin from docs_url (e.g. "https://milvus.io" from "https://milvus.io/docs")
                    // Note: In production we should use the `url` crate. Doing a basic split here for the mock phase.
                    let parts: Vec<&str> = docs_url.splitn(4, '/').collect();
                    let origin = if parts.len() >= 3 {
                        format!("{}//{}", parts[0], parts[2])
                    } else {
                        docs_url.trim_end_matches('/').to_string()
                    };
                    format!("{}{}", origin, first_link)
                } else {
                    format!("{}/{}", docs_url.trim_end_matches('/'), first_link)
                };
                resolved_url
            } else {
                warn!("No TOC links found. Falling back to the entry page.");
                docs_url.to_string()
            };

            info!("Processing page: {}", link_to_process);
            let page_html = crawler.fetch_page(&link_to_process).await?;
            let markdown = clean_content(&page_html);
            info!("Successfully cleaned page content into Markdown.");

            // 5. Extract StructuredContract via DeepSeek LLM
            let llm_client = match DeepSeekClient::new() {
                Ok(client) => client,
                Err(e) => {
                    anyhow::bail!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e);
                }
            };

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
        }
        Commands::Test { target, version, contracts, repo_url, docs_url } => {
            info!("Starting testing pipeline for target: {} version: {}", target, version);
            
            use report::generator::{BugReport, CandidateDefect, CandidateStatus};
            use contract::schema::StructuredContract;
            
            // 1. Initialize LLM Client
            let llm_client = match DeepSeekClient::new() {
                Ok(client) => client,
                Err(e) => {
                    anyhow::bail!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e);
                }
            };

            // 2. Obtain Contract (Load from file OR generate via Knowledge Agent)
            let contract_content = if let Some(contracts_dir) = contracts {
                info!("Loading contracts from: {}", contracts_dir);
                let contract_path = Path::new(contracts_dir).join(format!("{}_contract.json", target));
                if !contract_path.exists() {
                    anyhow::bail!("Contract file not found: {:?}", contract_path);
                }
                fs::read_to_string(&contract_path)?
            } else {
                let repo = repo_url.as_ref().context("repo_url is required when contracts directory is not provided")?;
                let docs = docs_url.as_ref().context("docs_url is required when contracts directory is not provided")?;
                
                info!("No contracts directory provided. Starting Knowledge Agent Phase...");
                info!("Target Repo: {}", repo);
                info!("Target Docs: {}", docs);

                // Load endpoint registry
                let registry_path = Path::new(".trae/endpoints/qdrant.toml");
                let registry = if registry_path.exists() {
                    info!("Loading endpoint registry from {:?}", registry_path);
                    contract::load_endpoint_registry(registry_path)?
                } else {
                    // Fallback: single-endpoint mode with the provided docs URL
                    info!("No endpoint registry found. Using single-endpoint mode.");
                    contract::schema::EndpointRegistry {
                        target: target.clone(),
                        version: version.clone(),
                        endpoints: vec![contract::schema::EndpointEntry {
                            name: target.clone(),
                            api_path: "search".to_string(),
                            docs_url: docs.clone(),
                            category: "points".to_string(),
                        }],
                    }
                };

                info!("Endpoint registry loaded: {} endpoints", registry.endpoints.len());

                // Clone repo once for all endpoints
                let kw_sandbox = sandbox::manager::Sandbox::create_knowledge_worker("ubuntu:latest", &["git", "curl", "grep", "ca-certificates"]).await?;
                let clone_result = kw_sandbox.exec_command(&["git", "clone", repo, "/workspace"]).await?;
                if !clone_result.success {
                    warn!("Git clone may have failed: {}", clone_result.stderr);
                } else {
                    info!("Repository cloned successfully.");
                }

                // Iterate over each endpoint
                let mut all_contracts: Vec<StructuredContract> = Vec::new();
                for entry in &registry.endpoints {
                    info!("Knowledge Agent: extracting contract for endpoint '{}' ({})", entry.name, entry.api_path);
                    let max_turns = 8;  // per-endpoint turns (B1: forced submit at turn 3-4)
                    let contract_result = agent::engine::knowledge_exploration_loop(
                        &llm_client,
                        &kw_sandbox,
                        target,
                        repo,
                        &entry.name,
                        &entry.api_path,
                        &entry.docs_url,
                        max_turns,
                    ).await;

                    match contract_result {
                        Ok(contract) => {
                            info!("Contract extracted for '{}': {} assertions, {} type, {} range, {} state",
                                entry.name,
                                contract.assertions.len(),
                                contract.type_constraints.len(),
                                contract.range_constraints.len(),
                                contract.state_constraints.len(),
                            );
                            all_contracts.push(contract);
                        }
                        Err(e) => {
                            warn!("Failed to extract contract for '{}': {}. Continuing with next endpoint.", entry.name, e);
                        }
                    }
                }

                kw_sandbox.cleanup().await?;

                if all_contracts.is_empty() {
                    anyhow::bail!("Knowledge Agent failed to extract any contracts from {} endpoints.", registry.endpoints.len());
                }

                info!("Knowledge Agent extracted {} endpoint contracts.", all_contracts.len());

                // === Post-process: tag→layer classification ===
                // KA produces flat assertions like "[TYPE] limit must be integer"
                // We parse the prefix to fill the structured layer fields.
                let mut merged_type: Vec<contract::schema::TypeConstraint> = Vec::new();
                let mut merged_range: Vec<contract::schema::RangeConstraint> = Vec::new();
                let mut merged_state: Vec<contract::schema::StateConstraint> = Vec::new();
                let mut merged_api_endpoints = Vec::new();

                for c in &all_contracts {
                    merged_api_endpoints.push(c.api_endpoint.clone());
                    for a in &c.assertions {
                        let a_lower = a.to_lowercase();
                        if a_lower.starts_with("[type]") {
                            let content = a[6..].trim();
                            // Extract param_name: first word after "[TYPE]"
                            let param = content.split_whitespace().next().unwrap_or("unknown");
                            merged_type.push(contract::schema::TypeConstraint {
                                param_name: param.to_string(),
                                expected_type: content.to_string(),
                                violation_examples: vec![],
                            });
                        } else if a_lower.starts_with("[range]") {
                            let content = a[7..].trim();
                            let param = content.split_whitespace().next().unwrap_or("unknown");
                            merged_range.push(contract::schema::RangeConstraint {
                                param_name: param.to_string(),
                                description: content.to_string(),
                                min: None,
                                max: None,
                                violation_examples: vec![],
                            });
                        } else if a_lower.starts_with("[state") {
                            let is_deterministic = a_lower.contains("deterministic") || !a_lower.contains("non-deterministic") && a_lower.starts_with("[state]");
                            // Extract content after "] " or just the whole string
                            let content = if let Some(idx) = a.find("] ") {
                                a[idx+2..].trim().to_string()
                            } else {
                                a.clone()
                            };
                            merged_state.push(contract::schema::StateConstraint {
                                description: content,
                                determinism: if is_deterministic {
                                    contract::schema::Determinism::Deterministic
                                } else {
                                    contract::schema::Determinism::NonDeterministic
                                },
                                setup_script_template: None,
                            });
                        }
                        // Unprefixed assertions stay in the flat list only
                    }
                }

                let merged_api_endpoint = merged_api_endpoints.join("+");

                let generated_contract = StructuredContract {
                    api_endpoint: merged_api_endpoint,
                    doc_url: docs.clone(),
                    assertions: all_contracts.iter().flat_map(|c| c.assertions.clone()).collect(),
                    type_constraints: merged_type,
                    range_constraints: merged_range,
                    state_constraints: merged_state,
                    state_invariants: all_contracts.iter().flat_map(|c| c.state_invariants.clone()).collect(),
                };
                
                // Save it temporarily for debugging or Fuzzing Agent consumption
                fs::create_dir_all(".trae/auto_contracts").unwrap_or_default();
                let contract_path = format!(".trae/auto_contracts/{}_contract.json", target);
                let json_str = serde_json::to_string_pretty(&generated_contract)?;
                fs::write(&contract_path, &json_str)?;
                info!("Saved auto-generated contract to {}", contract_path);
                
                json_str
            };

            let contract: StructuredContract = serde_json::from_str(&contract_content)
                .context("Failed to parse contract JSON for testing")?;

            // 3. Initialize Target Registry and get plugin
            let mut registry = TargetRegistry::new();
            registry.register(Box::new(QdrantPlugin));
            let plugin = registry.get(target)
                .ok_or_else(|| anyhow::anyhow!("Unsupported target database: {}. Available: {:?}", target, registry.available_targets()))?;

            let db_image = plugin.target_image(version);
            let pip_packages = plugin.pip_packages();
            let db_port = plugin.db_port();

            // 4. Run Agentic Exploration via FAOrchestrator
            let max_turns = 12;
            let orchestrator = FAOrchestrator::new(
                &llm_client,
                plugin,
                contract_content,
                version,
                max_turns,
            );
            let (script_code, initial_run, initial_classification) = orchestrator.run().await?;

            match initial_classification.disposition {
                ClassificationDisposition::Pass => {
                    info!("Gatekeeper: Test passed perfectly. No defects found.");
                }
                ClassificationDisposition::CoverageDetected => {
                    info!("Gatekeeper: FA submitted coverage report. No defects found by FA. {}", initial_classification.reason);
                }
                ClassificationDisposition::NonDefectInfraError => {
                    warn!(
                        "Gatekeeper: Non-defect infrastructure issue detected. No formal bug report will be generated. Reason: {}",
                        initial_classification.reason
                    );
                }
                ClassificationDisposition::RetryableScriptError => {
                    anyhow::bail!("Unexpected retryable script error escaped retry loop.");
                }
                ClassificationDisposition::CandidateDefect => {
                    let defect_type = initial_classification
                        .defect_type
                        .clone()
                        .context("Candidate defect is missing a defect type")?;
                    warn!(
                        "Gatekeeper: Candidate defect detected. Starting double reproduction for {:?}.",
                        defect_type
                    );

                    let mut candidate = CandidateDefect {
                        target: target.clone(),
                        version: version.clone(),
                        defect_type: defect_type.clone(),
                        doc_citation_url: contract.doc_url.clone(),
                        contract_assertions: contract.assertions.clone(),
                        surviving_assertions: contract.assertions.clone(),
                        mre_code: script_code.clone(),
                        initial_run,
                        reproduction_runs: Vec::new(),
                        status: CandidateStatus::Pending,
                        downgrade_reason: None,
                        independent_review_summary: None,
                        review_scope: None,
                    };

                    for phase in ["repro_1", "repro_2"] {
                        let run = run_script_in_fresh_sandbox(
                            &db_image,
                            &pip_packages,
                            db_port,
                            &script_code,
                            phase,
                        )
                        .await?;
                        let repro_classification = agent::classifier::analyze_execution_result(&run.stdout, &run.stderr);

                        if repro_classification.disposition != ClassificationDisposition::CandidateDefect
                            || repro_classification.defect_type.as_ref() != Some(&defect_type)
                            // Relax the exact excerpt matching during Agentic Fuzzing, as generated scripts might output slightly different error texts on different runs, we just need the same DefectType and Marker.
                        {
                            candidate.status = CandidateStatus::Rejected;
                            candidate.downgrade_reason = Some(format!(
                                "{} failed verification: {}",
                                phase, repro_classification.reason
                            ));
                            candidate.reproduction_runs.push(run);
                            let candidate_path = format!("{}_candidate_defect.md", target);
                            BugReport::export_candidate_to_markdown(&candidate, &candidate_path)?;
                            warn!(
                                "Candidate defect downgraded after failed reproduction. Saved candidate artifact to {}",
                                candidate_path
                            );
                            return Ok(());
                        }

                        candidate.reproduction_runs.push(run);
                    }

                    candidate.status = CandidateStatus::ReproducedTwice;
                    let reviewer_opt: Option<Box<dyn crate::review::IndependentReviewer>> = plugin.create_reviewer();
                    if let Some(reviewer) = reviewer_opt {
                        let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
                        let review_sandbox = sandbox::manager::Sandbox::create_network_and_containers(
                            &db_image,
                            &pip_refs,
                            db_port,
                        ).await?;
                        let probe_result = match reviewer.run_probe(&review_sandbox, db_port).await {
                            Ok(v) => v,
                            Err(err) => {
                                candidate.status = CandidateStatus::Rejected;
                                candidate.downgrade_reason = Some(format!(
                                    "Independent developer-side review could not complete cleanly: {}",
                                    err
                                ));
                                let candidate_path = format!("{}_candidate_defect.md", target);
                                BugReport::export_candidate_to_markdown(&candidate, &candidate_path)?;
                                warn!(
                                    "Candidate defect downgraded because independent review could not complete cleanly. Saved candidate artifact to {}",
                                    candidate_path
                                );
                                return Ok(());
                            }
                        };
                        let independent_review = reviewer.summarize_findings(&probe_result);
                        let Some((reviewed_defect_type, validated_issues)) = independent_review else {
                            candidate.status = CandidateStatus::Rejected;
                            candidate.downgrade_reason = Some(
                                "Independent developer-side review did not confirm any remaining issue.".to_string(),
                            );
                            let candidate_path = format!("{}_candidate_defect.md", target);
                            BugReport::export_candidate_to_markdown(&candidate, &candidate_path)?;
                            warn!(
                                "Candidate defect downgraded after independent review rejected the conclusion. Saved candidate artifact to {}",
                                candidate_path
                            );
                            return Ok(());
                        };
                        candidate.defect_type = reviewed_defect_type;
                        candidate.surviving_assertions = validated_issues;
                        candidate.independent_review_summary = Some(format!(
                            "Independent developer-side replay confirmed the surviving issue subset: {}.",
                            candidate.surviving_assertions.join("; ")
                        ));
                        candidate.review_scope = Some(
                            "Fresh independent replay covered collection creation, seed insert, and the narrowed Qdrant search assertions outside the LLM-generated script."
                                .to_string(),
                        );
                        if candidate.defect_type == agent::classifier::DefectType::PoorDiagnostics {
                            candidate.mre_code =
                                crate::review::qdrant::build_qdrant_search_poor_diagnostics_mre(&candidate.surviving_assertions);
                            if let Some(reason) = refresh_candidate_evidence_with_mre(
                                &mut candidate,
                                &db_image,
                                &pip_packages,
                                db_port,
                            )
                            .await?
                            {
                                let candidate_path = format!("{}_candidate_defect.md", target);
                                BugReport::export_candidate_to_markdown(&candidate, &candidate_path)?;
                                warn!(
                                    "Candidate defect downgraded after narrowed evidence replay failed: {} Saved candidate artifact to {}",
                                    reason, candidate_path
                                );
                                return Ok(());
                            }
                        }
                    }
                    if candidate.independent_review_summary.is_none() {
                        if let Some(reason) = run_additional_reproduction(
                            &mut candidate,
                            &db_image,
                            &pip_packages,
                            db_port,
                        )
                        .await?
                        {
                            candidate.status = CandidateStatus::Rejected;
                            candidate.downgrade_reason = Some(reason.clone());
                            let candidate_path = format!("{}_candidate_defect.md", target);
                            BugReport::export_candidate_to_markdown(&candidate, &candidate_path)?;
                            warn!(
                                "Candidate defect downgraded because generic independent review failed. Saved candidate artifact to {}",
                                candidate_path
                            );
                            return Ok(());
                        }
                    }
                    let report = BugReport::from_verified_candidate(&candidate)?;
                    report.validate()?;
                    info!(
                        "Submission-grade review verdict: {:?}. Summary: {}",
                        report.submission_grade_review.verdict,
                        report.submission_grade_review.summary
                    );
                    for reason in &report.submission_grade_review.direct_fail_reasons {
                        warn!("Submission-grade review fail reason: {}", reason);
                    }
                    let report_path =
                        formal_report_output_path(target, &report.submission_grade_review.verdict);
                    report.export_to_markdown(&report_path)?;
                    match report.submission_grade_review.verdict {
                        report::generator::SubmissionGradeVerdict::SubmissionGrade => {
                            info!("Verified submission-grade bug report successfully generated: {}", report_path);
                        }
                        report::generator::SubmissionGradeVerdict::NeedsRewrite => {
                            warn!(
                                "Verified defect evidence exported, but the report still needs rewrite before it counts as submission-grade: {}",
                                report_path
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod normalization_tests {
    use super::{formal_report_output_path};
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
        assert_eq!(
            formal_report_output_path("qdrant", &SubmissionGradeVerdict::SubmissionGrade),
            "qdrant_bug_report.md"
        );
        assert_eq!(
            formal_report_output_path("qdrant", &SubmissionGradeVerdict::NeedsRewrite),
            "qdrant_report_needs_rewrite.md"
        );
    }

    #[test]
    fn independent_probe_prefers_illegal_success() {
        let result = IndependentProbeResult {
            create_status: 200,
            create_body: "{}".to_string(),
            upsert_status: 200,
            upsert_body: "{}".to_string(),
            vector_status: 400,
            vector_body: "{\"status\":{\"error\":\"wrong vector size\"}}".to_string(),
            limit_status: 200,
            limit_body: "{}".to_string(),
            offset_status: 400,
            offset_body: "{\"status\":{\"error\":\"wrong offset\"}}".to_string(),
            hnsw_ef_status: 400,
            hnsw_ef_body: "{\"status\":{\"error\":\"invalid hnsw_ef\"}}".to_string(),
            ..Default::default()
        };

        let summary = summarize_qdrant_independent_probe(&result).expect("expected issue");
        assert_eq!(summary.0, DefectType::IllegalSuccess);
        assert!(
            summary
                .1
                .iter()
                .any(|issue| issue.contains("limit=0 request succeeded"))
        );
    }

    #[test]
    fn unexpected_status_is_not_treated_as_poor_diagnostics() {
        let result = IndependentProbeResult {
            create_status: 200,
            create_body: "{}".to_string(),
            upsert_status: 200,
            upsert_body: "{}".to_string(),
            vector_status: 500,
            vector_body: "{\"status\":{\"error\":\"internal error\"}}".to_string(),
            limit_status: 404,
            limit_body: "{\"status\":{\"error\":\"not found\"}}".to_string(),
            offset_status: 405,
            offset_body: "{\"status\":{\"error\":\"method not allowed\"}}".to_string(),
            hnsw_ef_status: 500,
            hnsw_ef_body: "{}".to_string(),
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
            create_status: 200,
            create_body: "{}".to_string(),
            upsert_status: 200,
            upsert_body: "{}".to_string(),
            vector_status: 400,
            vector_body: "{\"status\":{\"error\":\"wrong vector size\"}}".to_string(),
            limit_status: 400,
            limit_body: "{\"status\":{\"error\":\"limit must be positive\"}}".to_string(),
            offset_status: 400,
            offset_body: "{\"status\":{\"error\":\"offset must be non-negative\"}}".to_string(),
            hnsw_ef_status: 200,
            hnsw_ef_body: "{\"result\":[]}".to_string(),
            ..Default::default()
        };

        let summary = summarize_qdrant_independent_probe(&result).expect("expected issue");
        assert_eq!(summary.0, DefectType::IllegalSuccess);
        assert!(
            summary
                .1
                .iter()
                .any(|issue| issue.contains("hnsw_ef=0"))
        );
    }
}
