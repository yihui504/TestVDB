use chrono::Utc;
use crate::agent::classifier::{ClassificationDisposition, ClassificationResult};
use crate::agent::llm::DeepSeekClient;
use crate::agent::orchestrator::{CollectedDefect, FAOrchestrator};
use crate::agent::vdbfuzz::mutation::CreativeMutationPrompt;
use crate::contract::analyzer::{BatchDefect, ResultAnalyzer};
use crate::contract::gate::ContractGate;
use crate::contract::prompt::PromptGenerator;
use crate::contract::schema::StructuredContract;
use crate::mine_state::{MinePhase, MineState};
use crate::report::false_positive_filter::FalsePositiveFilter;
use crate::report::generator;
use crate::target::TargetRegistry;
use crate::{contract_loader, feedback_loop, infra, sandbox, verification_runner};
use anyhow::Context;
use std::collections::HashMap;
use tracing::{info, warn};

pub async fn run_extract(target: &str, docs_url: &str, out_dir: &str, llm_url: &str, llm_model: &str, llm_temperature: Option<f64>) -> anyhow::Result<()> {
    let llm_client = DeepSeekClient::new(llm_url.to_string(), llm_model.to_string(), llm_temperature)
        .map_err(|e| anyhow::anyhow!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e))?;
    contract_loader::run_extract(target, docs_url, out_dir, &llm_client).await
}

pub async fn run_test(
    target: &str,
    version: &str,
    contracts: &Option<String>,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
    multi_defect: bool,
    llm_url: &str,
    llm_model: &str,
    llm_temperature: Option<f64>,
) -> anyhow::Result<()> {
    info!("Starting testing pipeline for target: {} version: {}", target, version);

    let llm_client = DeepSeekClient::new(llm_url.to_string(), llm_model.to_string(), llm_temperature)
        .map_err(|e| anyhow::anyhow!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e))?;

    let contract_content = contract_loader::load_contract_content(
        contracts, target, version, repo_url, docs_url, &llm_client
    ).await?;

    let mut contract: StructuredContract = serde_json::from_str(&contract_content)
        .context("Failed to parse contract JSON for testing")?;

    contract_loader::augment_contract(&mut contract, target);

    let contract_content = serde_json::to_string_pretty(&contract)
        .context("Failed to re-serialize contract with behavioral templates")?;

    let registry = TargetRegistry::new_with_all();
    let plugin = registry.get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target database: {}. Available: {:?}", target, registry.available_targets()))?;

    let orchestrator = FAOrchestrator::new(&llm_client, plugin, contract_content, version, 12, multi_defect);
    let (script_code, initial_run, initial_classification, collected_defects) = orchestrator.run().await?;

    let (script_code, initial_run, initial_classification) =
        resolve_primary_defect(script_code, initial_run, initial_classification, collected_defects.clone());

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
                &llm_client,
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

fn resolve_primary_defect(
    script_code: String,
    initial_run: generator::RunEvidence,
    initial_classification: ClassificationResult,
    collected_defects: Vec<CollectedDefect>,
) -> (String, generator::RunEvidence, ClassificationResult) {
    use crate::agent::classifier::DefectType;
    let is_illegal_success = |dt: Option<&DefectType>| matches!(dt, Some(DefectType::IllegalSuccess));

    let all_defects: Vec<(String, generator::RunEvidence, ClassificationResult)> = {
        let mut v = vec![(script_code, initial_run, initial_classification)];
        for d in collected_defects {
            v.push((d.script, d.evidence, d.classification));
        }
        v
    };

    let non_illegal = all_defects.iter().find(|(_, _, c)| {
        c.disposition == ClassificationDisposition::CandidateDefect
            && !is_illegal_success(c.defect_type.as_ref())
    });

    if let Some((script, run, class)) = non_illegal {
        let dt = class.defect_type.as_ref().map(|d| format!("{:?}", d)).unwrap_or_default();
        warn!("resolve_primary_defect: preferring non-ILLEGAL_SUCCESS defect ({})", dt);
        (script.clone(), run.clone(), class.clone())
    } else if let Some((_, _, c)) = all_defects.iter().find(|(_, _, c)| {
        c.disposition == ClassificationDisposition::CandidateDefect
    }) {
        let idx = all_defects.iter().position(|(_, _, cc)| cc.disposition == ClassificationDisposition::CandidateDefect).expect("position matches find above");
        let (script, run, class) = all_defects.into_iter().nth(idx).expect("idx from position is valid");
        (script, run, class)
    } else {
        let (script, run, class) = all_defects.into_iter().next().expect("all_defects is non-empty at this point");
        (script, run, class)
    }
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
    skip_generators: bool,
    llm_turns: usize,
    skip_safety_nets: bool,
    strategy_threshold: usize,
    baseline_output: &Option<String>,
    llm_url: &str,
    llm_model: &str,
    llm_temperature: Option<f64>,
) -> anyhow::Result<()> {
    info!("Starting contract-driven bug mining for target: {} version: {}", target, version);

    let loaded_state = MineState::try_load(target, version);
    let resume_from_generators = loaded_state.as_ref().map(|s| s.phase == MinePhase::Generators).unwrap_or(false);
    let resume_from_orchestrator = loaded_state.as_ref().map(|s| s.phase == MinePhase::Orchestrator).unwrap_or(false);

    infra::cleanup_stale_containers();

    let llm_client = DeepSeekClient::new(llm_url.to_string(), llm_model.to_string(), llm_temperature)
        .map_err(|e| anyhow::anyhow!("Failed to initialize LLM client: {}. Please set DEEPSEEK_API_KEY.", e))?;

    let contract_content = contract_loader::load_contract_content(
        contracts, target, version, repo_url, docs_url, &llm_client
    ).await?;

    let mut contract: StructuredContract = serde_json::from_str(&contract_content)
        .context("Failed to parse contract JSON for mining")?;

    contract_loader::augment_contract(&mut contract, target);

    let registry = TargetRegistry::new_with_all();
    let plugin = registry.get(target)
        .ok_or_else(|| anyhow::anyhow!("Unsupported target: {}. Available: {:?}", target, registry.available_targets()))?;

    let gate_result = ContractGate::check(&contract, plugin, target);
    let gate_log_path = std::path::Path::new("contract_gate.log");
    ContractGate::log_result(&gate_result, gate_log_path);
    if !gate_result.passed {
        return Err(anyhow::anyhow!(
            "Contract gate REJECTED for {}: {:.1}% core CRUD coverage ({}/{}) (threshold: 90%). Missing endpoints: {:?}. See contract_gate.log for details.",
            target,
            gate_result.coverage_pct,
            gate_result.covered_endpoints.len(),
            gate_result.total_core_endpoints,
            gate_result.missing_endpoints,
        ));
    }

    let contract_content_for_state = serde_json::to_string_pretty(&contract)?;
    let mut state = match loaded_state {
        Some(s) => s,
        None => MineState::new(
            target, version, &contract_content_for_state,
            max_rounds, llm_turns, strategy_threshold,
            skip_generators, skip_verify, multi_defect, shadow, skip_safety_nets,
        ),
    };
    if state.phase == MinePhase::ContractLoaded {
        state.phase = MinePhase::Generators;
        state.save()?;
    }

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

    let mut all_round_defects: Vec<Vec<BatchDefect>> = if resume_from_generators {
        state.all_round_defects.clone()
    } else {
        Vec::new()
    };
    let mut low_priority_defects: Vec<BatchDefect> = if resume_from_generators {
        state.low_priority_defects.clone()
    } else {
        Vec::new()
    };
    let mut converged_at: Option<usize> = if resume_from_generators {
        state.converged_at
    } else {
        None
    };

    let start_round = if resume_from_generators { state.current_round + 1 } else { 1 };

    if resume_from_generators {
        for round_defects in &all_round_defects {
            ResultAnalyzer::assimilate_batch(&mut store, round_defects);
        }
        info!("Resumed from round {}: re-assimilated {} rounds of defects into store", state.current_round, all_round_defects.len());
    }

    if skip_generators || resume_from_orchestrator {
        if resume_from_orchestrator {
            info!("=== RESUME: Skipping generators, jumping to LLM orchestrator ===");
        } else {
            info!("=== SKIP GENERATORS: Jumping straight to LLM orchestrator ===");
        }
    } else {
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

        for round in start_round..=max_rounds {
            info!("=== Feedback Loop Round {} ===", round);
            info!("Round {}: ContractStore snapshot: {} type_constraints, {} range_constraints, {} observed_behaviors",
                round, store.type_constraints.len(), store.range_constraints.len(), store.observed_behaviors.len());

            let round_defects = feedback_loop::run_deterministic_round(target, &store, style, strategy_threshold, feedback_sandbox.as_ref()).await;
            let defect_count = round_defects.len();
            info!("Round {}: found {} total defects", round, defect_count);

            all_round_defects.push(round_defects.clone());

            if round < max_rounds {
                let new_observations = ResultAnalyzer::assimilate_batch(&mut store, &round_defects);
                info!("Round {}: assimilated {} new observations into ContractStore", round, new_observations);
                let min_rounds = 2;
                if new_observations == 0 && round >= min_rounds {
                    info!("Round {}: feedback loop converged (reason: new_observations == 0)", round);
                    converged_at = Some(round);
                    state.current_round = round;
                    state.all_round_defects = all_round_defects.clone();
                    state.converged_at = converged_at;
                    state.save()?;
                    break;
                }
                if round == 3 && defect_count == 0 {
                    info!("Round {}: feedback loop converged (reason: defect_count == 0)", round);
                    converged_at = Some(round);
                    state.current_round = round;
                    state.all_round_defects = all_round_defects.clone();
                    state.converged_at = converged_at;
                    state.save()?;
                    break;
                }
                info!("Round {}: ContractStore now has {} type_constraints, {} range_constraints, {} observed_behaviors",
                    round, store.type_constraints.len(), store.range_constraints.len(), store.observed_behaviors.len());
            }

            state.current_round = round;
            state.all_round_defects = all_round_defects.clone();
            state.converged_at = converged_at;
            state.save()?;
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
    }

    if state.phase == MinePhase::Generators {
        state.phase = MinePhase::Orchestrator;
        state.all_round_defects = all_round_defects.clone();
        state.low_priority_defects = low_priority_defects.clone();
        state.converged_at = converged_at;
        state.save()?;
    }

    let creative = CreativeMutationPrompt::from_store(&store);
    info!(
        "Generated {} creative mutation targets across {} categories",
        creative.targets.len(),
        crate::agent::vdbfuzz::mutation::count_creative_categories(&creative.targets),
    );

    let contract_content_for_orchestrator = serde_json::to_string_pretty(&contract)?;

    // ── Build clustered batch defect summary for the LLM orchestrator ──
    let all_batch_defects_flat: Vec<BatchDefect> = all_round_defects
        .iter()
        .flatten()
        .filter(|d| {
            if d.defect_line.contains("PARAM_IGNORED") || d.defect_line.contains("PERMISSIVE_PARSING") {
                low_priority_defects.push((*d).clone());
                false
            } else { true }
        })
        .cloned()
        .collect();
    let clusters = ResultAnalyzer::cluster_defects(&all_batch_defects_flat, &store);
    let mut batch_summary = String::new();
    for c in &clusters {
        if c.count >= 2 || !c.likely_benign {
            let tag = if c.likely_benign { "[BENIGN]" } else { "[REAL]" };
            batch_summary.push_str(&format!("{} {:?} x{}: {} (param: {})\n",
                tag, c.defect_kind, c.count,
                c.exemplar.defect_line,
                c.exemplar.param_name.as_deref().unwrap_or("?")));
            let script_preview: String = c.exemplar.script.chars().take(500).collect();
            if !script_preview.is_empty() {
                batch_summary.push_str(&format!("  Exemplar script (first 500 chars):\n{}\n", script_preview));
            }
        }
    }
    if !batch_summary.is_empty() {
        info!("Feeding {} defect clusters to LLM orchestrator as DO-NOT-RETEST constraints", clusters.len());
    }

    let mut initial_msg = constraint_prompt.initial_message;
    if !creative.targets.is_empty() {
        initial_msg.push_str("\n\n");
        initial_msg.push_str(&creative.prompt);
    }

    let mut experience = crate::experience_handoff::ExperienceHandoff::load(target, version)
        .unwrap_or_else(|| crate::experience_handoff::ExperienceHandoff::new(target, version));
    let exp_context = experience.build_llm_context();
    if !exp_context.is_empty() {
        initial_msg.push_str(&exp_context);
        info!("Injected experience handoff context ({} rounds) into LLM prompt", experience.total_rounds_completed);
    }

    let mut orchestrator = FAOrchestrator::new(
        &llm_client,
        plugin,
        contract_content_for_orchestrator,
        version,
        llm_turns,
        multi_defect,
    ).with_batch_defects(batch_summary)
    .with_custom_prompt(
        constraint_prompt.system_prompt,
        initial_msg,
    ).with_skip_safety_nets(skip_safety_nets);

    let diagnostic_dir = format!("results/{}/{}/{}_diagnostics", target, version, Utc::now().format("%Y%m%d_%H%M%S"));
    orchestrator = orchestrator.with_diagnostic_dir(diagnostic_dir);

    if let Some(path) = baseline_output {
        orchestrator = orchestrator.with_baseline_output(path.clone());
    }

    let (script_code, initial_run, initial_classification, collected_defects) =
        orchestrator.run().await?;

    let collected_defects: Vec<CollectedDefect> = collected_defects
        .into_iter()
        .filter(|cd| {
            use crate::report::false_positive_filter::{extract_endpoint_from_script, extract_trigger_pattern};
            let endpoint = extract_endpoint_from_script(&cd.script);
            let defect_type = format!("{:?}", cd.classification.defect_type);
            let trigger_pattern = extract_trigger_pattern(&cd.evidence.stdout, &cd.evidence.stderr);
            let already = experience.already_explored(&endpoint, &defect_type, &trigger_pattern);
            if already {
                info!(
                    "Cross-round dedup: skipping already explored defect | endpoint={} type={} trigger={}",
                    endpoint, defect_type, trigger_pattern
                );
            }
            !already
        })
        .collect();

    let filter_summary = FalsePositiveFilter::filter_collected_defects(
        &collected_defects, &contract, target,
    );
    info!(
        "FalsePositiveFilter (orchestrator): {} input, {} passed, {} rejected",
        filter_summary.total_input, filter_summary.passed, filter_summary.rejected
    );

    if state.phase == MinePhase::Orchestrator {
        state.phase = MinePhase::Verification;
        state.orchestrator_defects = collected_defects.clone();
        state.save()?;
    }

    let (script_code, initial_run, initial_classification) =
        resolve_primary_defect(script_code, initial_run, initial_classification, collected_defects.clone());

    let mine_defect_count = match initial_classification.disposition {
        ClassificationDisposition::CandidateDefect => {
            let defect_type = initial_classification.defect_type.clone()
                .context("Candidate defect missing defect type")?;
            warn!("Contract-driven mining found defect: {:?}", defect_type);
            if skip_verify {
                info!("SKIP VERIFY: LLM defect found but verification skipped (--skip-verify)");
                1
            } else {
                let verified = verification_runner::verify_llm_defect(
                    &llm_client,
                    defect_type, script_code, initial_run, &contract, target, version, plugin
                ).await?;
                if verified.is_some() { 1 } else { 0 }
            }
        }
        _ => {
            info!("Contract-driven mining: no defects found.");
            0
        }
    };

    state.mine_defect_count = mine_defect_count;
    state.phase = MinePhase::Complete;
    state.save()?;

    let all_batch_defects: Vec<BatchDefect> = {
        let flat: Vec<BatchDefect> = all_round_defects.into_iter().flatten().collect();
        let mut seen = std::collections::HashSet::new();
        flat.into_iter()
            .filter(|d| {
                if d.defect_line.contains("PARAM_IGNORED") || d.defect_line.contains("PERMISSIVE_PARSING") {
                    low_priority_defects.push(d.clone());
                    false
                } else { true }
            })
            .filter(|d| seen.insert(d.test_name.clone()))
            .collect()
    };

    let defect_counts: Vec<(&str, usize)> = vec![
        ("boundary", all_batch_defects.iter().filter(|d| d.test_prefix == "boundary").count()),
        ("mutation", all_batch_defects.iter().filter(|d| d.test_prefix == "mutation").count()),
        ("state", all_batch_defects.iter().filter(|d| d.test_prefix == "state").count()),
        ("meta", all_batch_defects.iter().filter(|d| d.test_prefix == "meta").count()),
        ("seq", all_batch_defects.iter().filter(|d| d.test_prefix == "seq").count()),
        ("res", all_batch_defects.iter().filter(|d| d.test_prefix == "res").count()),
        ("combo", all_batch_defects.iter().filter(|d| d.test_prefix == "combo").count()),
        ("diff", all_batch_defects.iter().filter(|d| d.test_prefix == "diff").count()),
        ("conc", all_batch_defects.iter().filter(|d| d.test_prefix == "conc").count()),
    ];

    if skip_verify {
        info!("=== SKIP VERIFY: Skipping per-defect sandbox verification ===");
        info!("=== Deterministic Test Generator Summary ===");
        info!("Total unique defects: {}", all_batch_defects.len());
        info!("Low-priority defects (PARAM_IGNORED/PERMISSIVE_PARSING): {}", low_priority_defects.len());
        for (prefix, count) in &defect_counts {
            info!("  {}: {} defects", prefix, count);
        }
        let mut defect_types: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for d in &all_batch_defects {
            let dtype = if d.defect_line.contains("ILLEGAL_SUCCESS") {
                "ILLEGAL_SUCCESS".to_string()
            } else if d.defect_line.contains("IDEMPOTENT_SUCCESS") {
                "IDEMPOTENT_SUCCESS".to_string()
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
        // ── Root cause clustering ──
        let clusters = ResultAnalyzer::cluster_defects(&all_batch_defects, &store);
        let real_clusters: Vec<_> = clusters.iter().filter(|c| !c.likely_benign).collect();
        let benign_clusters: Vec<_> = clusters.iter().filter(|c| c.likely_benign).collect();
        info!("=== ROOT CAUSE CLUSTERS ===");
        info!("Total clusters: {} ({} real, {} benign patterns)", clusters.len(), real_clusters.len(), benign_clusters.len());
        for c in &real_clusters {
            info!("  [REAL] {:?} x{}: {}", c.defect_kind, c.count, c.root_cause);
        }
        for c in &benign_clusters {
            info!("  [BENIGN] {:?} x{}: {} — {}", c.defect_kind, c.count, c.root_cause, c.benign_rationale);
        }
        let json_path = format!("{}_mine_defects_skip_verify.json", target);
        let json_data = serde_json::to_string_pretty(&all_batch_defects)?;
        std::fs::write(&json_path, &json_data)?;
        info!("Defect data saved to {}", json_path);
    } else {
        let batch_filter_summary = FalsePositiveFilter::filter_batch_defects(
            &all_batch_defects, &contract, target,
        );
        info!(
            "FalsePositiveFilter (batch): {} input, {} passed, {} rejected",
            batch_filter_summary.total_input, batch_filter_summary.passed, batch_filter_summary.rejected
        );
        verification_runner::verify_batch_defects(&llm_client, &all_batch_defects, target, version, plugin).await?;
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

    // ── H2: Structured results storage ──
    {
        let ts = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let results_dir = format!("results/{}/{}/{}", target, version, ts);
        std::fs::create_dir_all(&results_dir)?;
        let defects_json = serde_json::to_string_pretty(&all_batch_defects)?;
        std::fs::write(format!("{}/defects.json", results_dir), &defects_json)?;
        let summary = format!(
            "# {} {} Mine Results\n\n- High-priority defects: {}\n- Low-priority defects (PARAM_IGNORED/PERMISSIVE_PARSING): {}\n- LLM defects: {}\n- Strategies: {:?}\n",
            target, version, all_batch_defects.len(), low_priority_defects.len(), mine_defect_count,
            defect_counts.iter().map(|(s,c)| format!("{}={}", s, c)).collect::<Vec<_>>()
        );
        std::fs::write(format!("{}/summary.md", results_dir), &summary)?;
        info!("Results saved to {}", results_dir);
    }

    let round_experience = build_round_experience(
        &all_batch_defects,
        &collected_defects,
        &contract,
        mine_defect_count,
        experience.total_rounds_completed + 1,
    );
    experience.add_round(round_experience);
    if let Err(e) = experience.save() {
        warn!("Failed to save experience handoff: {}", e);
    }

    MineState::cleanup();

    Ok(())
}

fn build_round_experience(
    batch_defects: &[BatchDefect],
    collected_defects: &[CollectedDefect],
    contract: &StructuredContract,
    mine_defect_count: usize,
    round_number: usize,
) -> crate::experience_handoff::RoundExperience {
    use crate::experience_handoff::DefectPattern;
    use crate::report::false_positive_filter::{extract_endpoint_from_script, extract_trigger_pattern};

    let mut explored_patterns: Vec<DefectPattern> = Vec::new();

    for bd in batch_defects {
        let ep = extract_endpoint_from_script(&bd.script);
        let tp = extract_trigger_pattern(&bd.stdout, &bd.stderr);
        explored_patterns.push(DefectPattern {
            endpoint: ep,
            defect_type: bd.test_prefix.clone(),
            trigger_pattern: tp,
            verified: bd.exit_success,
        });
    }

    for cd in collected_defects {
        let ep = extract_endpoint_from_script(&cd.script);
        let tp = format!("{:?}", cd.classification.defect_type);
        explored_patterns.push(DefectPattern {
            endpoint: ep,
            defect_type: tp,
            trigger_pattern: cd.classification.reason.clone(),
            verified: false,
        });
    }

    let covered_endpoints: Vec<String> = vec![contract.api_endpoint.clone()];

    let mut covered_params: Vec<String> = Vec::new();
    for tc in &contract.type_constraints {
        covered_params.push(tc.param_name.clone());
    }
    for rc in &contract.range_constraints {
        covered_params.push(rc.param_name.clone());
    }
    covered_params.sort();
    covered_params.dedup();

    let llm_summary = format!(
        "Round {} completed: {} batch defects, {} LLM-generated defects confirmed, {} total defects found.",
        round_number,
        batch_defects.len(),
        mine_defect_count,
        batch_defects.len() + mine_defect_count,
    );

    crate::experience_handoff::RoundExperience {
        round_number,
        timestamp: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        explored_defect_patterns: explored_patterns,
        covered_endpoints,
        covered_params,
        llm_conversation_summary: llm_summary,
        defects_found_this_round: batch_defects.len() + mine_defect_count,
    }
}

pub async fn run_mine_all(
    version: &Option<String>,
    max_rounds: usize,
    llm_turns: usize,
    skip_generators: bool,
    multi_defect: bool,
    shadow: bool,
    skip_verify: bool,
    skip_safety_nets: bool,
    strategy_threshold: usize,
    cache_images: bool,
    llm_url: &str,
    llm_model: &str,
    llm_temperature: Option<f64>,
) -> anyhow::Result<()> {
    const DB_TARGETS: &[(&str, &str)] = &[
        ("milvus", "v2.4.0"),
        ("qdrant", "v1.9.0"),
        ("weaviate", "1.25.0"),
        ("pgvector", "pg17"),
    ];

    let mut results: HashMap<&str, String> = HashMap::new();

    for (target, default_version) in DB_TARGETS {
        let ver = version.as_deref().unwrap_or(default_version);

        info!(
            "========== MineAll: starting {} (version {}) ==========",
            target, ver
        );

        if !cache_images {
            infra::full_docker_cleanup();
            infra::cleanup_volumes(".");
        }

        MineState::cleanup();

        match run_mine(
            target,
            ver,
            &None,
            &None,
            &None,
            multi_defect,
            shadow,
            skip_verify,
            max_rounds,
            skip_generators,
            llm_turns,
            skip_safety_nets,
            strategy_threshold,
            &None,
            llm_url,
            llm_model,
            llm_temperature,
        ).await {
            Ok(()) => {
                let status = format!("SUCCESS");
                info!(
                    "========== MineAll: {} (version {}) COMPLETED ==========",
                    target, ver
                );
                results.insert(target, status);
            }
            Err(e) => {
                let status = format!("FAILED: {}", e);
                warn!(
                    "========== MineAll: {} (version {}) FAILED: {} ==========",
                    target, ver, e
                );
                results.insert(target, status);
            }
        }
    }

    let mut summary = format!(
        "# TestVDB MineAll Summary Report\n\nGenerated: {}\n\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S")
    );
    summary.push_str(&format!(
        "Parameters: max_rounds={}, llm_turns={}, skip_generators={}, multi_defect={}, shadow={}, skip_verify={}\n\n",
        max_rounds, llm_turns, skip_generators, multi_defect, shadow, skip_verify,
    ));
    summary.push_str("## Per-Database Results\n\n");
    summary.push_str("| Database | Version | Status |\n");
    summary.push_str("|----------|---------|--------|\n");

    for (target, default_version) in DB_TARGETS {
        let ver = version.as_deref().unwrap_or(default_version);
        let status = results.get(target).map(|s| s.as_str()).unwrap_or("NOT RUN");
        summary.push_str(&format!("| {} | {} | {} |\n", target, ver, status));
    }

    summary.push_str("\n## Output Files\n\n");
    summary.push_str("See per-DB output files:\n");
    summary.push_str("- `{db}_bug_report.md` — Submission-grade bug report (if found)\n");
    summary.push_str("- `{db}_candidate_defect.md` — Candidate defect report\n");
    summary.push_str("- `results/{db}/{version}/` — Structured results directory\n");

    let report_path = "mineall_summary.md";
    std::fs::write(report_path, &summary)?;
    info!("MineAll summary report written to {}", report_path);
    println!("{}", summary);

    Ok(())
}

pub async fn run_verify(
    target: &str,
    version: &str,
    issue_file: &str,
    attempts: usize,
) -> anyhow::Result<()> {
    use crate::agent::classifier::analyze_execution_result;
    use crate::agent::sandbox_runner::run_script_in_fresh_sandbox;

    info!(
        "=== Verify: {} version {} (file: {}, attempts: {}) ===",
        target, version, issue_file, attempts
    );

    let registry = TargetRegistry::new_with_all();
    let plugin = registry
        .get(target)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unsupported target: {}. Available: {:?}",
                target,
                registry.available_targets()
            )
        })?;

    let db_image = plugin.target_image(version);
    let pip_packages = plugin.pip_packages();
    let db_port = plugin.db_port();
    let sidecars = plugin.db_sidecars();
    let db_env = plugin.db_env();
    let db_command = plugin.db_command();
    let auth_header = plugin.auth_header_value().unwrap_or("");

    let file_content =
        std::fs::read_to_string(issue_file).context("Failed to read issue file")?;

    let mre_code = if issue_file.ends_with(".py") {
        file_content
    } else {
        extract_mre_from_markdown(&file_content)?
    };

    let mut success_count = 0;
    for attempt in 1..=attempts {
        info!("Verification attempt {}/{}", attempt, attempts);
        match run_script_in_fresh_sandbox(
            &db_image,
            &pip_packages,
            db_port,
            &mre_code,
            &format!("verify_attempt_{}", attempt),
            &sidecars,
            &db_env,
            &db_command,
            None,
            auth_header,
        )
        .await
        {
            Ok(run) => {
                let classification =
                    analyze_execution_result(&run.stdout, &run.stderr, None);
                if classification.disposition == ClassificationDisposition::CandidateDefect {
                    success_count += 1;
                    info!("Attempt {}: DEFECT CONFIRMED", attempt);
                } else {
                    info!(
                        "Attempt {}: No defect detected ({})",
                        attempt, classification.reason
                    );
                }
            }
            Err(e) => {
                warn!("Attempt {}: Sandbox error: {}", attempt, e);
            }
        }
    }

    let result = if success_count == 0 {
        "FAIL"
    } else if success_count >= 2 {
        "PASS"
    } else {
        "INCONCLUSIVE"
    };

    println!("=== Verification Result ===");
    println!("Target: {}", target);
    println!("Version: {}", version);
    println!("Issue File: {}", issue_file);
    println!("Attempts: {}", attempts);
    println!("Successes: {}/{}", success_count, attempts);
    println!("Result: {}", result);

    Ok(())
}

fn extract_mre_from_markdown(content: &str) -> anyhow::Result<String> {
    let mre_header = "## Minimal Reproducible Example (MRE)";
    let pos = content
        .find(mre_header)
        .ok_or_else(|| anyhow::anyhow!("MRE section not found in issue file"))?;

    let after_header = &content[pos + mre_header.len()..];

    let code_start = after_header
        .find("```")
        .ok_or_else(|| anyhow::anyhow!("MRE code block start not found"))?;

    let after_start = &after_header[code_start + 3..];
    let after_lang = after_start.trim_start();

    let code_content_start = if after_lang.starts_with("python") || after_lang.starts_with("py") {
        after_lang.find('\n').map(|n| n + 1).unwrap_or(0)
    } else {
        0
    };

    let after_lang_tag = &after_lang[code_content_start..];

    let code_end = after_lang_tag
        .find("```")
        .ok_or_else(|| anyhow::anyhow!("MRE code block end not found"))?;

    let mre = after_lang_tag[..code_end].trim().to_string();
    if mre.is_empty() {
        anyhow::bail!("MRE code block is empty");
    }

    Ok(mre)
}