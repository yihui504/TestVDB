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

    let mut skip_cleanup = false;

    match &cli.command {
        Commands::Extract { target, docs_url, out_dir } => {
            commands::run_extract(target, docs_url, out_dir).await?;
        }
        Commands::Test { target, version, contracts, repo_url, docs_url, multi_defect } => {
            commands::run_test(target, version, contracts, repo_url, docs_url, *multi_defect).await?;
        }
        Commands::Batch { target, network, db_host, db_port, non_redundant_only, cache_images } => {
            skip_cleanup = *cache_images;
            batch_runner::run_batch(target, network, db_host, *db_port, *non_redundant_only).await?;
        }
        Commands::Mine { target, version, contracts, repo_url, docs_url, multi_defect, shadow, skip_verify, max_rounds, skip_generators, llm_turns, skip_safety_nets, strategy_threshold, cache_images } => {
            skip_cleanup = *cache_images;
            commands::run_mine(target, version, contracts, repo_url, docs_url, *multi_defect, *shadow, *skip_verify, *max_rounds, *skip_generators, *llm_turns, *skip_safety_nets, *strategy_threshold).await?;
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
