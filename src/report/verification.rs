use crate::agent::classifier::{ClassificationDisposition, analyze_execution_result};
use crate::agent::sandbox_runner::{run_script_in_fresh_sandbox, refresh_candidate_evidence_with_mre, run_additional_reproduction};
use crate::report::generator::{BugReport, CandidateDefect, CandidateStatus};
use crate::target::TargetPlugin;
use crate::review::IndependentReviewer;
use crate::sandbox::manager::Sandbox;
use tracing::{info, warn};

pub async fn verify_candidate_defect(
    candidate: &mut CandidateDefect,
    script_code: &str,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
    plugin: &dyn TargetPlugin,
    target: &str,
) -> anyhow::Result<VerificationOutcome> {
    let defect_type = candidate.defect_type.clone();

    for phase in ["repro_1", "repro_2"] {
        let run = run_script_in_fresh_sandbox(
            db_image,
            pip_packages,
            db_port,
            script_code,
            phase,
        )
        .await?;
        let repro_classification = analyze_execution_result(&run.stdout, &run.stderr);

        if repro_classification.disposition != ClassificationDisposition::CandidateDefect
            || repro_classification.defect_type.as_ref() != Some(&defect_type)
        {
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

        candidate.reproduction_runs.push(run);
    }

    candidate.status = CandidateStatus::ReproducedTwice;

    let reviewer_opt: Option<Box<dyn IndependentReviewer>> = plugin.create_reviewer();
    if let Some(reviewer) = reviewer_opt {
        let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
        let review_sandbox = Sandbox::create_network_and_containers(
            db_image,
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
                BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
                warn!(
                    "Candidate defect downgraded because independent review could not complete cleanly. Saved candidate artifact to {}",
                    candidate_path
                );
                return Ok(VerificationOutcome::Rejected(candidate.downgrade_reason.clone().unwrap_or_default()));
            }
        };
        let independent_review = reviewer.summarize_findings(&probe_result);
        let Some((reviewed_defect_type, validated_issues)) = independent_review else {
            candidate.status = CandidateStatus::Rejected;
            candidate.downgrade_reason = Some(
                "Independent developer-side review did not confirm any remaining issue.".to_string(),
            );
            let candidate_path = format!("{}_candidate_defect.md", target);
            BugReport::export_candidate_to_markdown(candidate, &candidate_path)?;
            warn!(
                "Candidate defect downgraded after independent review rejected the conclusion. Saved candidate artifact to {}",
                candidate_path
            );
            return Ok(VerificationOutcome::Rejected(candidate.downgrade_reason.clone().unwrap_or_default()));
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
        if candidate.defect_type == crate::agent::classifier::DefectType::PoorDiagnostics {
            candidate.mre_code =
                crate::review::qdrant::build_qdrant_search_poor_diagnostics_mre(&candidate.surviving_assertions);
            if let Some(reason) = refresh_candidate_evidence_with_mre(
                candidate,
                db_image,
                pip_packages,
                db_port,
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

    let report = BugReport::from_verified_candidate(candidate)?;
    report.validate()?;
    info!(
        "Submission-grade review verdict: {:?}. Summary: {}",
        report.submission_grade_review.verdict,
        report.submission_grade_review.summary
    );
    for reason in &report.submission_grade_review.direct_fail_reasons {
        warn!("Submission-grade review fail reason: {}", reason);
    }

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
