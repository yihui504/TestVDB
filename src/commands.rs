use crate::agent::classifier::ClassificationDisposition;
use crate::agent::llm::DeepSeekClient;
use crate::agent::orchestrator::FAOrchestrator;
use crate::agent::vdbfuzz::mutation::CreativeMutationPrompt;
use crate::contract::analyzer::{BatchDefect, ResultAnalyzer};
use crate::contract::prompt::PromptGenerator;
use crate::contract::schema::StructuredContract;
use crate::target::{MilvusPlugin, QdrantPlugin, TargetRegistry};
use crate::{contract_loader, feedback_loop, infra, sandbox, verification_runner};
use anyhow::Context;
use tracing::{info, warn};

pub async fn run_extract(target: &str, docs_url: &str, out_dir: &str) -> anyhow::Result<()> {
    contract_loader::run_extract(target, docs_url, out_dir).await
}

pub async fn run_test(
    target: &str,
    version: &str,
    contracts: &Option<String>,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
    multi_defect: bool,
) -> anyhow::Result<()> {
    info!("Starting testing pipeline for target: {} version: {}", target, version);

    let llm_client = DeepSeekClient::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e))?;

    let contract_content = contract_loader::load_contract_content(
        contracts, target, version, repo_url, docs_url, &llm_client
    ).await?;

    let mut contract: StructuredContract = serde_json::from_str(&contract_content)
        .context("Failed to parse contract JSON for testing")?;

    contract_loader::augment_contract(&mut contract, target);

    let contract_content = serde_json::to_string_pretty(&contract)
        .context("Failed to re-serialize contract with behavioral templates")?;

    let mut registry = TargetRegistry::new();
    registry.register(Box::new(QdrantPlugin));
    registry.register(Box::new(MilvusPlugin));
    let plugin = registry.get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target database: {}. Available: {:?}", target, registry.available_targets()))?;

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
            if let Some(report) = verification_runner::verify_llm_defect(
                defect_type, script_code, initial_run, &contract, target, version, plugin
            ).await? {
                match report.submission_grade_review.verdict {
                    crate::report::generator::SubmissionGradeVerdict::SubmissionGrade => {
                        info!("Verified submission-grade bug report");
                    }
                    crate::report::generator::SubmissionGradeVerdict::NeedsRewrite => {
                        warn!("Report needs rewrite");
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn run_batch(
    target: &str,
    network: &Option<String>,
    db_host: &Option<String>,
    db_port: u16,
    non_redundant_only: bool,
) -> anyhow::Result<()> {
    crate::batch_runner::run_batch(target, network, db_host, db_port, non_redundant_only).await
}

pub async fn run_mine(
    target: &str,
    version: &str,
    contracts: &Option<String>,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
    multi_defect: bool,
    shadow: bool,
    skip_verify: bool,
    max_rounds: usize,
) -> anyhow::Result<()> {
    info!("Starting contract-driven bug mining for target: {} version: {}", target, version);

    infra::cleanup_stale_containers();

    let llm_client = DeepSeekClient::new()
        .map_err(|e| anyhow::anyhow!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e))?;

    let contract_content = contract_loader::load_contract_content(
        contracts, target, version, repo_url, docs_url, &llm_client
    ).await?;

    let mut contract: StructuredContract = serde_json::from_str(&contract_content)
        .context("Failed to parse contract JSON for mining")?;

    contract_loader::augment_contract(&mut contract, target);

    let mut registry = TargetRegistry::new();
    registry.register(Box::new(QdrantPlugin));
    registry.register(Box::new(MilvusPlugin));
    let plugin = registry.get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target: {}. Available: {:?}", target, registry.available_targets()))?;

    let style = plugin.target_style();
    let mut store = contract_loader::build_contract_store(&contract, target, version);
    info!("{}", store.constraint_stats());

    let pgen = PromptGenerator::new(store.clone(), style);
    let constraint_prompt = pgen.generate();

    info!(
        "Generated {} violation scenarios across {} strategies",
        constraint_prompt.violation_scenarios.len(),
        crate::contract::prompt::count_unique_strategies(&constraint_prompt.violation_scenarios),
    );

    let max_rounds = max_rounds.max(1).min(10);
    let mut all_round_defects: Vec<Vec<BatchDefect>> = Vec::new();
    let mut converged_at: Option<usize> = None;

    let db_image = plugin.target_image(version);
    let db_port = plugin.db_port();
    let db_sidecars = plugin.db_sidecars();
    let db_env = plugin.db_env();
    let db_command = plugin.db_command();
    let pip_pkgs_owned = plugin.pip_packages();
    let pip_pkgs: Vec<&str> = pip_pkgs_owned.iter().map(|s| s.as_str()).collect();

    info!("Spawning DB sandbox for feedback loop: {}", db_image);
    let feedback_sandbox = match sandbox::manager::Sandbox::create_network_and_containers(
        &db_image, &pip_pkgs, db_port, &db_sidecars, &db_env, &db_command,
    ).await {
        Ok(sb) => {
            info!("DB sandbox ready for feedback loop");
            Some(sb)
        }
        Err(e) => {
            warn!("Failed to spawn DB sandbox for feedback loop: {}. Running feedback loop without dedicated DB (will try to find existing container).", e);
            None
        }
    };

    for round in 1..=max_rounds {
        info!("=== Feedback Loop Round {} ===", round);
        info!("Round {}: ContractStore snapshot: {} type_constraints, {} range_constraints, {} observed_behaviors",
            round, store.type_constraints.len(), store.range_constraints.len(), store.observed_behaviors.len());

        let round_defects = feedback_loop::run_deterministic_round(target, &store, style, feedback_sandbox.as_ref()).await;
        let defect_count = round_defects.len();
        info!("Round {}: found {} total defects", round, defect_count);

        all_round_defects.push(round_defects.clone());

        if round < max_rounds {
            let new_observations = ResultAnalyzer::assimilate_batch(&mut store, &round_defects);
            info!("Round {}: assimilated {} new observations into ContractStore", round, new_observations);
            if new_observations == 0 {
                info!("Round {}: feedback loop converged (reason: new_observations == 0)", round);
                converged_at = Some(round);
                break;
            }
            if round == 3 && defect_count == 0 {
                info!("Round {}: feedback loop converged (reason: defect_count == 0)", round);
                converged_at = Some(round);
                break;
            }
            info!("Round {}: ContractStore now has {} type_constraints, {} range_constraints, {} observed_behaviors",
                round, store.type_constraints.len(), store.range_constraints.len(), store.observed_behaviors.len());
        }
    }

    if let Some(ref sb) = feedback_sandbox {
        info!("Cleaning up feedback loop DB sandbox...");
        if let Err(e) = sb.cleanup().await {
            warn!("Feedback sandbox cleanup error: {}", e);
        }
    }

    if converged_at.is_none() && max_rounds > 3 {
        info!("Feedback loop did not converge after {} rounds", max_rounds);
    }

    let defect_counts: Vec<(&str, usize)> = vec![
        ("boundary", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "boundary")).count()),
        ("mutation", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "mutation")).count()),
        ("state", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "state")).count()),
        ("meta", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "meta")).count()),
        ("seq", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "seq")).count()),
        ("res", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "res")).count()),
        ("combo", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "combo")).count()),
        ("diff", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "diff")).count()),
        ("conc", all_round_defects.iter().flat_map(|d| d.iter().filter(|d| d.test_prefix == "conc")).count()),
    ];

    let creative = CreativeMutationPrompt::from_store(&store);
    info!(
        "Generated {} creative mutation targets across {} categories",
        creative.targets.len(),
        crate::agent::vdbfuzz::mutation::count_creative_categories(&creative.targets),
    );

    let contract_content_for_orchestrator = serde_json::to_string_pretty(&contract)?;

    let mut initial_msg = constraint_prompt.initial_message;
    if !creative.targets.is_empty() {
        initial_msg.push_str("\n\n");
        initial_msg.push_str(&creative.prompt);
    }

    let orchestrator = FAOrchestrator::new(
        &llm_client,
        plugin,
        contract_content_for_orchestrator,
        version,
        12,
        multi_defect,
    ).with_custom_prompt(
        constraint_prompt.system_prompt,
        initial_msg,
    );

    let (script_code, initial_run, initial_classification, collected_defects) =
        orchestrator.run().await?;

    let (script_code, initial_run, initial_classification) =
        if initial_classification.disposition == ClassificationDisposition::Pass && !collected_defects.is_empty() {
            warn!("Gatekeeper: Final MRE passed, but {} defect(s) collected.", collected_defects.len());
            let first = collected_defects.into_iter().next().unwrap();
            (first.script, first.evidence, first.classification)
        } else {
            (script_code, initial_run, initial_classification)
        };

    let mine_defect_count = match initial_classification.disposition {
        ClassificationDisposition::CandidateDefect => {
            let defect_type = initial_classification.defect_type.clone()
                .context("Candidate defect missing defect type")?;
            warn!("Contract-driven mining found defect: {:?}", defect_type);
            let verified = verification_runner::verify_llm_defect(
                defect_type, script_code, initial_run, &contract, target, version, plugin
            ).await?;
            if verified.is_some() { 1 } else { 0 }
        }
        _ => {
            info!("Contract-driven mining: no defects found.");
            0
        }
    };

    let all_batch_defects: Vec<BatchDefect> = {
        let flat: Vec<BatchDefect> = all_round_defects.into_iter().flatten().collect();
        let mut seen = std::collections::HashSet::new();
        flat.into_iter().filter(|d| seen.insert(d.test_name.clone())).collect()
    };

    if skip_verify {
        info!("=== SKIP VERIFY: Skipping per-defect sandbox verification ===");
        info!("=== Deterministic Test Generator Summary ===");
        info!("Total unique defects: {}", all_batch_defects.len());
        for (prefix, count) in &defect_counts {
            info!("  {}: {} defects", prefix, count);
        }
        let mut defect_types: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for d in &all_batch_defects {
            let dtype = if d.defect_line.contains("ILLEGAL_SUCCESS") {
                "ILLEGAL_SUCCESS".to_string()
            } else if d.defect_line.contains("IDEMPOTENT_SUCCESS") {
                "IDEMPOTENT_SUCCESS".to_string()
            } else if d.defect_line.contains("PERMISSIVE_PARSING") {
                "PERMISSIVE_PARSING".to_string()
            } else if d.defect_line.contains("SEQUENCE_VIOLATION") {
                "SEQUENCE_VIOLATION".to_string()
            } else if d.defect_line.contains("DIFFERENTIAL_MISMATCH") {
                "DIFFERENTIAL_MISMATCH".to_string()
            } else {
                "OTHER".to_string()
            };
            *defect_types.entry(dtype).or_insert(0) += 1;
        }
        for (dtype, count) in &defect_types {
            info!("  {}: {} defects", dtype, count);
        }
        let json_path = format!("{}_mine_defects_skip_verify.json", target);
        let json_data = serde_json::to_string_pretty(&all_batch_defects)?;
        std::fs::write(&json_path, &json_data)?;
        info!("Defect data saved to {}", json_path);
    } else {
        verification_runner::verify_batch_defects(&all_batch_defects, target, version, plugin).await?;
    }

    if shadow {
        info!("=== SHADOW MODE: Running batch mode for comparison ===");
        let batch_result = crate::batch_runner::run_batch_simple(target).await;
        match batch_result {
            Ok(batch_defects) => {
                println!("\n=== SHADOW MODE COMPARISON ===");
                println!("Contract-driven (LLM) defects: {}", mine_defect_count);
                for (prefix, count) in &defect_counts {
                    println!("Contract-driven ({}) defects: {}", prefix, count);
                }
                println!("Batch (hand-written) defects: {}", batch_defects);
                if mine_defect_count > 0 || defect_counts.iter().any(|(_, c)| *c > 0) {
                    println!("Contract-driven mining found defects — promising!");
                }
            }
            Err(e) => warn!("Shadow batch run failed: {}", e),
        }
    }

    Ok(())
}
