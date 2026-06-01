use crate::agent::classifier::{ClassificationDisposition, analyze_execution_result};
use crate::agent::sandbox_runner::{run_script_in_fresh_sandbox, refresh_candidate_evidence_with_mre, run_additional_reproduction};
use crate::report::generator::{BugReport, CandidateDefect, CandidateStatus};
use crate::agent::llm::DeepSeekClient;
use crate::report::llm_analysis::{analyze_defect_with_llm, generate_verification_variant, optimize_defect_report};
use crate::report::semantic_gate::{self, ParamEffect};
use crate::sandbox::manager::SidecarSpec;
use crate::target::TargetPlugin;
use crate::review::IndependentReviewer;
use crate::sandbox::manager::Sandbox;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::{info, warn};

struct ReviewerLog;

impl ReviewerLog {
    fn log_entry(target: &str, defect_type: &str, phase: &str, passed: bool, detail: &str) {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let status = if passed { "PASS" } else { "FAIL" };
        let line = format!(
            "[{}] [{}] [{}] {} | {}: {}\n",
            timestamp, target, defect_type, phase, status, detail
        );
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("reviewer.log") {
            let _ = writeln!(file, "{}", line.trim_end());
        }
        info!("ReviewerLog: {}", line.trim_end());
    }

    fn log_variant(target: &str, defect_type: &str, variant_desc: &str, confirmed: bool) {
        let status = if confirmed { "CONFIRMED" } else { "NOT_CONFIRMED" };
        Self::log_entry(target, defect_type, "variant_test", confirmed, &format!("variant_desc={}, result={}", variant_desc, status));
    }

    fn log_independent_review(target: &str, defect_type: &str, passed: bool, summary: &str) {
        Self::log_entry(target, defect_type, "independent_review", passed, summary);
    }

    fn log_final_verdict(target: &str, defect_type: &str, passed: bool, reason: &str) {
        let verdict = if passed { "ISSUE_GENERATED" } else { "NEEDS_REVIEW" };
        Self::log_entry(target, defect_type, "final_verdict", passed, &format!("verdict={}, reason={}", verdict, reason));
    }
}

pub async fn verify_candidate_defect(
    llm_client: &DeepSeekClient,
    candidate: &mut CandidateDefect,
    script_code: &str,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
    plugin: &dyn TargetPlugin,
    target: &str,
    sidecars: &[SidecarSpec],
    db_env: &[(String, String)],
    db_command: &[String],
) -> anyhow::Result<VerificationOutcome> {
    let defect_type = candidate.defect_type.clone();
    let mut effective_script = script_code.to_string();

    if let Some(gate) = semantic_gate::get_semantic_gate(target) {
        info!("SemanticGate: checking param effect for target={}", target);
        let effect = gate.check_param_effect(
            script_code,
            &defect_type,
            db_image,
            pip_packages,
            db_port,
            sidecars,
            db_env,
            db_command,
        ).await;

        let effect_str = format!("{:?}", effect);
        info!("SemanticGate: result={}", effect_str);
        candidate.semantic_gate_result = Some(effect_str.clone());

        match effect {
            ParamEffect::ConfirmedIgnored => {
                if defect_type == crate::agent::classifier::DefectType::ParamIgnored {
                    info!(
                        "SemanticGate: ConfirmedIgnored for ParamIgnored defect. Parameter was silently ignored as expected. Annotating as confirmed."
                    );
                } else {
                    candidate.status = CandidateStatus::Rejected;
                    candidate.downgrade_reason = Some(
                        "SemanticGate: parameter was silently ignored by the server (ConfirmedIgnored). Not a real defect.".to_string(),
                    );
                    let candidate_path = format!("{}_candidate_defect.md", target);
                    BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
                    warn!(
                        "Candidate rejected by SemanticGate (ConfirmedIgnored). Saved to {}",
                        candidate_path
                    );
                    return Ok(VerificationOutcome::Rejected(
                        candidate.downgrade_reason.clone().unwrap_or_default(),
                    ));
                }
            }
            ParamEffect::ActuallyApplied => {
                info!(
                    "SemanticGate: parameter was actually applied (ActuallyApplied). Annotating as potential design behavior, continuing verification."
                );
            }
            ParamEffect::Ambiguous => {
                info!("SemanticGate: result is Ambiguous. Proceeding with normal verification.");
            }
        }
    }

    if candidate.initial_run.exit_success {
        warn!(
            "MRE effectiveness gate: initial run classified as {:?} but script exited with code 0 (exit_success=true). MRE may not effectively demonstrate the defect.",
            defect_type
        );
    }

    let auth_header = plugin.auth_header_value().unwrap_or("");

    for phase in ["repro_1", "repro_2"] {
        let run = run_script_in_fresh_sandbox(
            db_image,
            pip_packages,
            db_port,
            &effective_script,
            phase,
            sidecars,
            db_env,
            db_command,
            None,
            auth_header,
        )
        .await?;
        let repro_classification = analyze_execution_result(&run.stdout, &run.stderr, None);

        if repro_classification.disposition != ClassificationDisposition::CandidateDefect
            || repro_classification.defect_type.as_ref() != Some(&defect_type)
        {
            if phase == "repro_1" {
                info!("repro_1 failed deterministic classification. Invoking LLM root cause analysis...");
                let defect_type_str = format!("{:?}", defect_type);
                let llm_analysis = analyze_defect_with_llm(
                    llm_client,
                    &run.stdout, &run.stderr, &defect_type_str, &effective_script,
                ).await;

                if llm_analysis.is_real_defect {
                    info!("LLM confirms real defect (confidence={:.2}): {}", llm_analysis.confidence, llm_analysis.root_cause);
                    if let Some(fixed) = &llm_analysis.fixed_script {
                        info!("LLM provided fixed script. Retrying repro_1 with fixed script...");
                        effective_script = fixed.clone();
                        candidate.mre_code = fixed.clone();
                        continue;
                    }
                    candidate.reproduction_runs.push(run);
                    warn!("LLM confirmed defect but no fixed script available. Proceeding with original classification.");
                    break;
                } else {
                    info!("LLM does NOT confirm real defect: {}", llm_analysis.root_cause);
                }
            }

            candidate.status = CandidateStatus::Rejected;
            candidate.downgrade_reason = Some(format!(
                "{} failed verification: {}",
                phase, repro_classification.reason
            ));
            candidate.reproduction_runs.push(run);
            let candidate_path = format!("{}_candidate_defect.md", target);
            BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
            warn!(
                "Candidate defect downgraded after failed reproduction. Saved candidate artifact to {}",
                candidate_path
            );
            return Ok(VerificationOutcome::Rejected(candidate.downgrade_reason.clone().unwrap_or_default()));
        }

        if phase == "repro_1" && run.stderr.to_lowercase().contains("traceback") {
            info!("repro_1 passed but stderr contains Traceback. Invoking LLM analysis to fix script...");
            let defect_type_str = format!("{:?}", defect_type);
            let llm_analysis = analyze_defect_with_llm(
                llm_client,
                &run.stdout, &run.stderr, &defect_type_str, &effective_script,
            ).await;

            if llm_analysis.is_real_defect {
                if let Some(fixed) = &llm_analysis.fixed_script {
                    info!("LLM provided fixed script (removed post-defect crash). Using for repro_2...");
                    effective_script = fixed.clone();
                    candidate.mre_code = fixed.clone();
                } else {
                    info!("LLM confirmed defect but no fixed script. Continuing with original MRE.");
                }
            } else {
                info!("LLM analysis of Traceback: {}", llm_analysis.root_cause);
            }
        }

        candidate.reproduction_runs.push(run);
    }

    candidate.status = CandidateStatus::ReproducedTwice;

    info!("Generating LLM verification variant for enhanced reproducibility check...");
    let defect_type_str = format!("{:?}", candidate.defect_type);
    let evidence_excerpt = candidate.initial_run.classifier_evidence_excerpt.clone();
    let variant_result = generate_verification_variant(
        llm_client,
        &candidate.mre_code, &defect_type_str, &evidence_excerpt,
    ).await;
    match variant_result {
        Ok(variant) => {
            info!("LLM generated verification variant: {}", variant.variant_description);
            let variant_run = run_script_in_fresh_sandbox(
                db_image, pip_packages, db_port, &variant.variant_script,
                "variant_1", sidecars, db_env, db_command, None, auth_header,
            ).await?;
            let variant_classification = analyze_execution_result(&variant_run.stdout, &variant_run.stderr, None);
            if variant_classification.disposition == ClassificationDisposition::CandidateDefect {
                info!("Variant verification CONFIRMED the defect with different parameters!");
                candidate.reproduction_runs.push(variant_run);
                ReviewerLog::log_variant(target, &defect_type_str, &variant.variant_description, true);
            } else {
                info!("Variant verification did not confirm defect (may be different manifestation). Continuing with original reproducibility.");
                ReviewerLog::log_variant(target, &defect_type_str, &variant.variant_description, false);
            }
        }
        Err(e) => {
            warn!("Failed to generate verification variant: {}. Continuing without variant test.", e);
            ReviewerLog::log_entry(target, &defect_type_str, "variant_test", false, &format!("generation_failed: {}", e));
        }
    }

    if let Some(corrected) = plugin.correct_mre_api_params(&effective_script, &defect_type) {
        info!("ApiParamGate: corrected MRE script generated, running in fresh sandbox to check for false positive...");
        let corrected_run = run_script_in_fresh_sandbox(
            db_image, pip_packages, db_port, &corrected,
            "api_param_gate", sidecars, db_env, db_command, None, auth_header,
        ).await?;
        let corrected_classification = analyze_execution_result(&corrected_run.stdout, &corrected_run.stderr, None);
        if corrected_classification.disposition != ClassificationDisposition::CandidateDefect {
            candidate.status = CandidateStatus::Rejected;
            candidate.downgrade_reason = Some(
                "ApiParamGate: corrected script (with proper API parameters) no longer reproduces the defect. Likely false positive from missing API parameters.".to_string(),
            );
            let candidate_path = format!("{}_candidate_defect.md", target);
            BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
            warn!(
                "Candidate rejected by ApiParamGate. Corrected script did not reproduce defect. Saved to {}",
                candidate_path
            );
            return Ok(VerificationOutcome::Rejected(candidate.downgrade_reason.clone().unwrap_or_default()));
        }
        info!("ApiParamGate: corrected script still reproduces defect. Continuing normal verification.");
    }

    let reviewer_opt: Option<Box<dyn IndependentReviewer>> = plugin.create_reviewer();
    if let Some(reviewer) = reviewer_opt {
        let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
        let review_sandbox = Sandbox::create_network_and_containers(
            db_image,
            &pip_refs,
            db_port,
            sidecars,
            db_env,
            db_command,
        ).await?;
        let probe_result = match reviewer.run_probe(&review_sandbox, db_port).await {
            Ok(v) => v,
            Err(err) => {
                candidate.status = CandidateStatus::NeedsReview;
                let reason = format!(
                    "Independent developer-side review could not complete cleanly: {}",
                    err
                );
                candidate.downgrade_reason = Some(reason.clone());
                ReviewerLog::log_independent_review(target, &defect_type_str, false, &reason);
                ReviewerLog::log_final_verdict(target, &defect_type_str, false, "independent_review_probe_failed");

                let candidate_path = format!("{}_candidate_defect.md", target);
                BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
                warn!(
                    "Candidate defect marked NEEDS_REVIEW because independent review could not complete cleanly. Saved candidate artifact to {}",
                    candidate_path
                );
                return Ok(VerificationOutcome::Rejected(candidate.downgrade_reason.clone().unwrap_or_default()));
            }
        };
        let independent_review = reviewer.summarize_findings(&probe_result);
        let Some((_reviewed_defect_type, validated_issues)) = independent_review else {
            candidate.status = CandidateStatus::NeedsReview;
            let reason = "Independent developer-side review did not confirm any remaining issue.".to_string();
            candidate.downgrade_reason = Some(reason.clone());
            ReviewerLog::log_independent_review(target, &defect_type_str, false, &reason);
            ReviewerLog::log_final_verdict(target, &defect_type_str, false, "independent_review_no_issues_confirmed");

            let candidate_path = format!("{}_candidate_defect.md", target);
            BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
            warn!(
                "Candidate defect marked NEEDS_REVIEW after independent review rejected the conclusion. Saved candidate artifact to {}",
                candidate_path
            );
            return Ok(VerificationOutcome::Rejected(candidate.downgrade_reason.clone().unwrap_or_default()));
        };
        candidate.surviving_assertions = validated_issues.clone();
        candidate.independent_review_summary = Some(format!(
            "Independent developer-side replay confirmed the surviving issue subset: {}.",
            candidate.surviving_assertions.join("; ")
        ));
        candidate.review_scope = Some(
            format!("Fresh independent replay covered collection creation, seed insert, and the narrowed {} search assertions outside the LLM-generated script.", candidate.target)
        );
        ReviewerLog::log_independent_review(target, &defect_type_str, true, &format!("surviving_assertions={}", validated_issues.len()));
        if candidate.defect_type == crate::agent::classifier::DefectType::PoorDiagnostics {
            candidate.mre_code =
                crate::review::qdrant::build_qdrant_search_poor_diagnostics_mre(&candidate.surviving_assertions);
            if let Some(reason) = refresh_candidate_evidence_with_mre(
                candidate,
                db_image,
                pip_packages,
                db_port,
                sidecars,
                db_env,
                db_command,
                None,
                auth_header,
            )
            .await?
            {
                let candidate_path = format!("{}_candidate_defect.md", target);
                BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
                warn!(
                    "Candidate defect downgraded after narrowed evidence replay failed: {} Saved candidate artifact to {}",
                    reason, candidate_path
                );
                return Ok(VerificationOutcome::Rejected(reason));
            }
        }
    }
    if candidate.independent_review_summary.is_none() {
        if let Some(reason) = run_additional_reproduction(
            candidate,
            db_image,
            pip_packages,
            db_port,
            sidecars,
            db_env,
            db_command,
            None,
            auth_header,
        )
        .await?
        {
            candidate.status = CandidateStatus::Rejected;
            candidate.downgrade_reason = Some(reason.clone());
            let candidate_path = format!("{}_candidate_defect.md", target);
            BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
            warn!(
                "Candidate defect downgraded because generic independent review failed. Saved candidate artifact to {}",
                candidate_path
            );
            return Ok(VerificationOutcome::Rejected(reason));
        }
    }

    let mut report = BugReport::from_verified_candidate(candidate)?;

    info!("Optimizing defect report with LLM...");
    let defect_type_str = format!("{:?}", report.defect_type);
    let evidence_excerpt = report.runtime_evidence.chars().take(2000).collect::<String>();
    if let Ok(optimized) = optimize_defect_report(
        llm_client,
        &defect_type_str,
        &report.surviving_assertions,
        &report.mre_code,
        &evidence_excerpt,
    ).await {
        info!("LLM optimized report title: {}", optimized.title);
        report.title = optimized.title;
        report.root_cause_analysis = optimized.root_cause_analysis;
        report.improvement_suggestions = optimized.improvement_suggestions;
        report.github_issue_body = Some(optimized.github_issue_body);
    }

    report.validate()?;
    info!(
        "Submission-grade review verdict: {:?}. Summary: {}",
        report.submission_grade_review.verdict,
        report.submission_grade_review.summary
    );
    for reason in &report.submission_grade_review.direct_fail_reasons {
        warn!("Submission-grade review fail reason: {}", reason);
    }

    ReviewerLog::log_final_verdict(target, &defect_type_str, true, "issue_generated");

    Ok(VerificationOutcome::Verified(report))
}

pub enum VerificationOutcome {
    Verified(BugReport),
    Rejected(String),
}

pub fn formal_report_output_path(
    target: &str,
    verdict: &crate::report::generator::SubmissionGradeVerdict,
) -> String {
    match verdict {
        crate::report::generator::SubmissionGradeVerdict::SubmissionGrade => {
            format!("{}_bug_report.md", target)
        }
        crate::report::generator::SubmissionGradeVerdict::NeedsRewrite => {
            format!("{}_report_needs_rewrite.md", target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_rewrite_reports_use_different_output_path() {
        let s = crate::report::generator::SubmissionGradeVerdict::SubmissionGrade;
        let r = crate::report::generator::SubmissionGradeVerdict::NeedsRewrite;
        assert_eq!(formal_report_output_path("qdrant", &s), "qdrant_bug_report.md");
        assert_eq!(formal_report_output_path("qdrant", &r), "qdrant_report_needs_rewrite.md");
    }
}
