use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    All,
    Markdown,
    Json,
    Csv,
}

#[derive(Parser, Debug)]
#[command(
    name = "stvault-receipt",
    author = "Lido stVault Contributors",
    version,
    about = "Verifies post-submission status of Lido stVault validator consolidation requests across Ethereum Execution & Consensus layers.",
    long_about = "A high-assurance, read-only CLI tool that traces Lido stVault validator consolidation requests from Execution Layer predeploy transactions to exact Consensus Layer pending_consolidations state acceptance."
)]
pub struct CliArgs {
    /// Path to the Lido stVault consolidation manifest file (JSON or YAML)
    #[arg(short, long, value_name = "PATH", required_unless_present = "generate_completions")]
    pub manifest: Option<PathBuf>,

    /// Execution layer transaction hash(es) separated by comma or specified multiple times
    #[arg(
        short = 't',
        long = "el-tx",
        value_name = "TX_HASH",
        value_delimiter = ',',
        required_unless_present = "generate_completions"
    )]
    pub el_txs: Vec<String>,

    /// Ethereum Execution Layer JSON-RPC URL (e.g. http://127.0.0.1:8545)
    #[arg(long, env = "EL_RPC_URL", value_name = "URL", default_value = "http://127.0.0.1:8545")]
    pub el_rpc: String,

    /// Ethereum Consensus Layer Beacon API URL (e.g. http://127.0.0.1:5052)
    #[arg(
        long,
        env = "CL_BEACON_API_URL",
        value_name = "URL",
        default_value = "http://127.0.0.1:5052"
    )]
    pub cl_beacon_api: String,

    /// Lido stVault Dashboard or AccessControl contract address (for fee-exemption role inspection)
    #[arg(long, env = "ST_VAULT_DASHBOARD", value_name = "ADDRESS")]
    pub st_vault_dashboard: Option<String>,

    /// Output directory where receipts and evidence artifacts will be saved
    #[arg(short, long, default_value = "./stvault_receipt_output", value_name = "DIR")]
    pub output_dir: PathBuf,

    /// Output format to print to stdout (all, markdown, json, csv)
    #[arg(long, value_enum, default_value = "all")]
    pub format: OutputFormat,

    /// Generate shell autocompletions (bash, zsh, fish, powershell, elvish)
    #[arg(long, value_name = "SHELL")]
    pub generate_completions: Option<Shell>,

    /// Suppress informative logging
    #[arg(short, long)]
    pub quiet: bool,
}

impl CliArgs {
    /// Helper to print shell autocompletions directly to standard output.
    pub fn print_completions(shell: Shell) {
        let mut cmd = Self::command();
        clap_complete::generate(shell, &mut cmd, "stvault-receipt", &mut std::io::stdout());
    }
}
