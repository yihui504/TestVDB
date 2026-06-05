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

        #[arg(long, env = "DEEPSEEK_API_URL", default_value = "https://api.deepseek.com/chat/completions")]
        llm_url: String,

        #[arg(long, env = "DEEPSEEK_MODEL", default_value = "deepseek-chat")]
        llm_model: String,

        #[arg(long, env = "DEEPSEEK_TEMPERATURE")]
        llm_temperature: Option<f64>,
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

        #[arg(long, env = "DEEPSEEK_API_URL", default_value = "https://api.deepseek.com/chat/completions")]
        llm_url: String,

        #[arg(long, env = "DEEPSEEK_MODEL", default_value = "deepseek-chat")]
        llm_model: String,

        #[arg(long, env = "DEEPSEEK_TEMPERATURE")]
        llm_temperature: Option<f64>,
    },
    /// Batch-run all safety net probes against a running DB instance
    Batch {
        /// Target vector database
        #[arg(long)]
        target: String,

        /// Docker network name where the DB is running
        #[arg(long)]
        network: Option<String>,

        /// DB hostname inside the Docker network (default: derived from target)
        #[arg(long)]
        db_host: Option<String>,

        /// DB port inside the Docker network (0 = auto-detect from target plugin)
        #[arg(long, default_value_t = 0)]
        db_port: u16,

        /// Only run non-redundant probes
        #[arg(long, default_value_t = false)]
        non_redundant_only: bool,

        /// Keep Docker images and Python packages between runs (skip cleanup)
        #[arg(long, default_value_t = false)]
        cache_images: bool,
    },
    /// Contract-driven bug mining: extract contracts → generate prompts → LLM tests → find bugs
    Mine {
        /// Target vector database (e.g., milvus, qdrant)
        #[arg(long)]
        target: String,

        /// Target version
        #[arg(long)]
        version: String,

        /// Optional: Directory containing extracted JSON contracts
        #[arg(long)]
        contracts: Option<String>,

        /// Optional: Git repo URL for Knowledge Agent
        #[arg(long)]
        repo_url: Option<String>,

        /// Optional: Documentation URL for Knowledge Agent
        #[arg(long)]
        docs_url: Option<String>,

        /// Continue exploration after finding the first defect
        #[arg(long, default_value_t = false)]
        multi_defect: bool,

        /// Also run batch mode (hand-written probes) for shadow comparison
        #[arg(long, default_value_t = false)]
        shadow: bool,

        /// Skip verification pipeline (only run deterministic + LLM, no per-defect sandbox verification)
        #[arg(long, default_value_t = false)]
        skip_verify: bool,

        /// Max feedback loop rounds (default: 5)
        #[arg(long, default_value_t = 5)]
        max_rounds: usize,

        /// Skip deterministic generators, go straight to LLM orchestrator (fast iteration mode)
        #[arg(long, default_value_t = false)]
        skip_generators: bool,

        /// Max LLM orchestrator turns (default: 12, use 6 for fast iteration)
        #[arg(long, default_value_t = 12)]
        llm_turns: usize,

        /// Skip safety net probes in LLM orchestrator (saves ~25min, use for fast iteration)
        #[arg(long, default_value_t = false)]
        skip_safety_nets: bool,

        /// Skip low-yield strategies (state/meta/seq/res/combo/conc) when constraints < this threshold (default: 100, 0 = run all)
        #[arg(long, default_value_t = 100)]
        strategy_threshold: usize,

        /// Keep Docker images and Python packages between runs (skip cleanup)
        #[arg(long, default_value_t = false)]
        cache_images: bool,

        /// Save baseline telemetry data to this JSON file path (for US-1.2b data collection)
        #[arg(long)]
        baseline_output: Option<String>,

        #[arg(long, env = "DEEPSEEK_API_URL", default_value = "https://api.deepseek.com/chat/completions")]
        llm_url: String,

        #[arg(long, env = "DEEPSEEK_MODEL", default_value = "deepseek-chat")]
        llm_model: String,

        #[arg(long, env = "DEEPSEEK_TEMPERATURE")]
        llm_temperature: Option<f64>,
    },
    /// Run the full Mine pipeline across all four DBs, unattended
    MineAll {
        /// Target version (applies to all DBs, overrides per-DB defaults)
        #[arg(long)]
        version: Option<String>,

        /// Max feedback loop rounds per DB (default: 5)
        #[arg(long, default_value_t = 5)]
        max_rounds: usize,

        /// Max LLM orchestrator turns per DB (default: 12)
        #[arg(long, default_value_t = 12)]
        llm_turns: usize,

        /// Skip deterministic generators, go straight to LLM orchestrator
        #[arg(long, default_value_t = false)]
        skip_generators: bool,

        /// Continue exploration after finding the first defect
        #[arg(long, default_value_t = false)]
        multi_defect: bool,

        /// Also run batch mode for shadow comparison
        #[arg(long, default_value_t = false)]
        shadow: bool,

        /// Skip verification pipeline
        #[arg(long, default_value_t = false)]
        skip_verify: bool,

        /// Skip safety net probes in LLM orchestrator
        #[arg(long, default_value_t = false)]
        skip_safety_nets: bool,

        /// Skip low-yield strategies threshold (default: 100, 0 = run all)
        #[arg(long, default_value_t = 100)]
        strategy_threshold: usize,

        /// Keep Docker images between DBs (skip cleanup)
        #[arg(long, default_value_t = false)]
        cache_images: bool,

        #[arg(long, env = "DEEPSEEK_API_URL", default_value = "https://api.deepseek.com/chat/completions")]
        llm_url: String,

        #[arg(long, env = "DEEPSEEK_MODEL", default_value = "deepseek-chat")]
        llm_model: String,

        #[arg(long, env = "DEEPSEEK_TEMPERATURE")]
        llm_temperature: Option<f64>,
    },
    /// Verify a defect from a previously-generated Issue or MRE script, independent of Mine
    Verify {
        /// Target vector database (e.g., milvus, qdrant)
        #[arg(long)]
        target: String,

        /// Target version
        #[arg(long)]
        version: String,

        /// Path to the Issue markdown file or MRE Python script to verify
        #[arg(long)]
        issue_file: String,

        /// Number of reproduction attempts (default: 3)
        #[arg(long, default_value_t = 3)]
        attempts: usize,
    },
}