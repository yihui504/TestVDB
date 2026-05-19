mod batch_runner;
mod cli;
mod commands;
pub mod agent;
pub mod contract;
mod contract_loader;
mod feedback_loop;
pub mod crawler;
pub mod infra;
pub mod report;
pub mod sandbox;
pub mod review;
pub mod target;
mod verification_runner;

use clap::Parser;
use cli::{Cli, Commands};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

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
            commands::run_extract(target, docs_url, out_dir).await?;
        }
        Commands::Test { target, version, contracts, repo_url, docs_url, multi_defect } => {
            commands::run_test(target, version, contracts, repo_url, docs_url, *multi_defect).await?;
        }
        Commands::Batch { target, network, db_host, db_port, non_redundant_only } => {
            batch_runner::run_batch(target, network, db_host, *db_port, *non_redundant_only).await?;
        }
        Commands::Mine { target, version, contracts, repo_url, docs_url, multi_defect, shadow, skip_verify, max_rounds } => {
            commands::run_mine(target, version, contracts, repo_url, docs_url, *multi_defect, *shadow, *skip_verify, *max_rounds).await?;
        }
    }

    {
        info!("Cleaning up all testvdb Docker resources...");
        infra::full_docker_cleanup();
        infra::cleanup_volumes(".");
        info!("All resources cleanup complete.");
    }

    Ok(())
}

#[cfg(test)]
mod normalization_tests {
    use crate::report::verification::formal_report_output_path;
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
