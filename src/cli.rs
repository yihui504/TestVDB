use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Extract structured contracts from official documentation
    Extract {
        /// Target vector database (e.g., milvus, qdrant)
        #[arg(long)]
        target: String,

        /// Official documentation URL to crawl
        #[arg(long)]
        docs_url: String,

        /// Output directory to save the JSON/YAML contracts
        #[arg(long)]
        out_dir: String,
    },
    /// Load contracts and run tests in Docker Sandbox
    Test {
        /// Target vector database
        #[arg(long)]
        target: String,

        /// Target vector database version
        #[arg(long)]
        version: String,

        /// Optional: Directory containing the extracted JSON contracts. If omitted, Knowledge Agent will generate it automatically.
        #[arg(long)]
        contracts: Option<String>,

        /// Optional: Target Git Repository URL for Knowledge Agent (required if contracts is not provided)
        #[arg(long)]
        repo_url: Option<String>,

        /// Optional: Target Documentation URL for Knowledge Agent (required if contracts is not provided)
        #[arg(long)]
        docs_url: Option<String>,

        /// Continue exploration after finding the first defect, collecting all defects
        #[arg(long, default_value_t = false)]
        multi_defect: bool,
    },
}
