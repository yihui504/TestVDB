use crate::agent::classifier::DefectType;
use crate::contract::analyzer::BatchDefect;
use crate::contract::schema::StructuredContract;
use crate::report::generator::{BugReport, CandidateDefect, CandidateStatus, RunEvidence};
use crate::report::verification::{self, VerificationOutcome};
use crate::target::TargetPlugin;
use tracing::{info, warn};

pub async fn verify_llm_defect(
    defect_type: DefectType,
    script_code: String,
    initial_run: RunEvidence,
    contract: &StructuredContract,
    target: &str,
    version: &str,
    plugin: &dyn TargetPlugin,
) -> anyhow::Result<Option<BugReport>> {
    let db_image = plugin.target_image(version);
    let pip_packages = plugin.pip_packages();
    let db_port = plugin.db_port();
    let sidecars = plugin.db_sidecars();
    let db_env = plugin.db_env();
    let db_command = plugin.db_command();

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
        &mut candidate, &mre_code, &db_image, &pip_packages, db_port, plugin, target, &sidecars, &db_env, &db_command,
    ).await?;

    match outcome {
        VerificationOutcome::Verified(report) => {
            let report_path = verification::formal_report_output_path(target, &report.submission_grade_review.verdict);
            report.export_to_markdown(&report_path)?;
            info!("Verified bug report: {}", report_path);
            Ok(Some(report))
        }
        VerificationOutcome::Rejected(reason) => {
            warn!("Candidate rejected: {}", reason);
            Ok(None)
        }
    }
}

pub async fn verify_batch_defects(
    defects: &[BatchDefect],
    target: &str,
    version: &str,
    plugin: &dyn TargetPlugin,
) -> anyhow::Result<Vec<BugReport>> {
    if defects.is_empty() {
        return Ok(Vec::new());
    }

    info!("=== Verifying {} batch defects (after dedup) through sandbox reproduction ===", defects.len());
    let db_image = plugin.target_image(version);
    let pip_packages = plugin.pip_packages();
    let db_port = plugin.db_port();
    let sidecars = plugin.db_sidecars();
    let db_env = plugin.db_env();
    let db_command = plugin.db_command();

    let mut verified_reports = Vec::new();

    for (i, bd) in defects.iter().enumerate() {
        let defect_type = match bd.test_prefix.as_str() {
            "boundary" | "res" | "mutation" => DefectType::IllegalSuccess,
            "combo" | "conc" | "seq" => DefectType::SequenceViolation,
            "diff" => DefectType::DifferentialMismatch,
            "meta" => DefectType::MetamorphicViolation,
            "state" => DefectType::StateLogicViolation,
            _ => DefectType::IllegalSuccess,
        };

        let doc_citation_url = plugin.doc_citation_url();

        let initial_run = RunEvidence {
            phase: "initial".to_string(),
            db_url: "batch".to_string(),
            stdout: bd.stdout.clone(),
            stderr: bd.stderr.clone(),
            classifier_reason: bd.defect_line.clone(),
            classifier_evidence_excerpt: bd.defect_line.clone(),
        };

        let mut candidate = CandidateDefect {
            target: target.to_string(),
            version: version.to_string(),
            defect_type: defect_type.clone(),
            doc_citation_url,
            contract_assertions: vec![bd.defect_line.clone()],
            surviving_assertions: vec![bd.defect_line.clone()],
            mre_code: bd.script.clone(),
            initial_run,
            reproduction_runs: Vec::new(),
            status: CandidateStatus::Pending,
            downgrade_reason: None,
            independent_review_summary: None,
            review_scope: None,
        };

        let mre_code = bd.script.clone();
        let outcome = verification::verify_candidate_defect(
            &mut candidate, &mre_code, &db_image, &pip_packages, db_port, plugin, target, &sidecars, &db_env, &db_command,
        ).await?;

        match outcome {
            VerificationOutcome::Verified(report) => {
                let report_path = verification::formal_report_output_path(
                    target,
                    &report.submission_grade_review.verdict,
                );
                let named_path = format!("{}_batch_{}_{}", report_path.trim_end_matches(".md"), bd.test_prefix, i);
                let final_path = format!("{}.md", named_path);
                report.export_to_markdown(&final_path)?;
                info!("Verified batch bug report [{}/{}]: {}", i + 1, defects.len(), final_path);
                verified_reports.push(report);
            }
            VerificationOutcome::Rejected(reason) => {
                info!("Batch defect [{}/{}] rejected: {}", i + 1, defects.len(), reason);
            }
        }
    }

    Ok(verified_reports)
}
