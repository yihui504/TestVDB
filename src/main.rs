mod experience_handoff;
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
mod mine_state;

use clap::Parser;
use cli::{Cli, Commands};
use tracing::{info, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

fn init_logging() {
    let file_appender = tracing_appender::rolling::never(".", "testvdb_run.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    std::mem::forget(_guard);

    let filter = EnvFilter::from_default_env()
        .add_directive(Level::INFO.into());

    let stdout_layer = tracing_subscriber::fmt::layer();
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    let cli = Cli::parse();

    let mut skip_cleanup = false;

    match &cli.command {
        Commands::Extract { target, docs_url, out_dir, llm_url, llm_model, llm_temperature } => {
            commands::run_extract(target, docs_url, out_dir, llm_url, llm_model, *llm_temperature).await?;
        }
        Commands::Test { target, version, contracts, repo_url, docs_url, multi_defect, llm_url, llm_model, llm_temperature } => {
            commands::run_test(target, version, contracts, repo_url, docs_url, *multi_defect, llm_url, llm_model, *llm_temperature).await?;
        }
        Commands::Batch { target, network, db_host, db_port, non_redundant_only, cache_images } => {
            skip_cleanup = *cache_images;
            batch_runner::run_batch(target, network, db_host, *db_port, *non_redundant_only).await?;
        }
        Commands::Mine { target, version, contracts, repo_url, docs_url, multi_defect, shadow, skip_verify, max_rounds, skip_generators, llm_turns, skip_safety_nets, strategy_threshold, cache_images, baseline_output, llm_url, llm_model, llm_temperature } => {
            skip_cleanup = *cache_images;
            commands::run_mine(target, version, contracts, repo_url, docs_url, *multi_defect, *shadow, *skip_verify, *max_rounds, *skip_generators, *llm_turns, *skip_safety_nets, *strategy_threshold, baseline_output, llm_url, llm_model, *llm_temperature).await?;
        }
        Commands::MineAll { version, max_rounds, llm_turns, skip_generators, multi_defect, shadow, skip_verify, skip_safety_nets, strategy_threshold, cache_images, llm_url, llm_model, llm_temperature } => {
            skip_cleanup = *cache_images;
            commands::run_mine_all(version, *max_rounds, *llm_turns, *skip_generators, *multi_defect, *shadow, *skip_verify, *skip_safety_nets, *strategy_threshold, *cache_images, llm_url, llm_model, *llm_temperature).await?;
        }
        Commands::Verify { target, version, issue_file, attempts } => {
            commands::run_verify(target, version, issue_file, *attempts).await?;
        }
    }

    if !skip_cleanup {
        info!("Cleaning up all testvdb Docker resources...");
        infra::full_docker_cleanup();
        infra::cleanup_volumes(".");
        info!("All resources cleanup complete.");
    } else {
        info!("--cache-images: skipping Docker cleanup");
    }

    Ok(())
}
