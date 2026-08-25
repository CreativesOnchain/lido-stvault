use clap::Parser;
use colored::*;
use std::process::ExitCode;
use stvault_receipt::cli::CliArgs;
use stvault_receipt::terminal;
use stvault_receipt::{
    AppError, BeaconClient, ElClient, VerificationEngine, generate_and_save_receipts,
    parse_manifest_file,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args = CliArgs::parse();

    // Handle shell completion generation if requested
    if let Some(shell) = args.generate_completions {
        CliArgs::print_completions(shell);
        return ExitCode::SUCCESS;
    }

    match run(args).await {
        Ok(all_accepted) => {
            if all_accepted {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("{} {}", "ERROR:".bold().red(), e);
            ExitCode::from(e.exit_code())
        }
    }
}

/// Executes the cross-layer verification pipeline.
/// Returns `Ok(true)` if all pairs are accepted, or `Ok(false)` if any require attention.
async fn run(args: CliArgs) -> Result<bool, AppError> {
    let manifest_path = args.manifest.as_ref().ok_or_else(|| {
        AppError::Manifest("Missing required argument: --manifest <PATH>".to_string())
    })?;

    if !args.quiet {
        terminal::print_banner();
        println!("📂 Parsing manifest: {}", manifest_path.display());
    }

    // Step 1: Parse Manifest
    let pairs = parse_manifest_file(manifest_path)?;

    if !args.quiet {
        terminal::print_connection_info(pairs.len(), &args.el_rpc, &args.cl_beacon_api);
    }

    // Step 2: Initialize RPC Clients
    let timeout = std::time::Duration::from_secs(args.timeout);
    let el_client = ElClient::with_timeout(&args.el_rpc, timeout);
    let beacon_client = BeaconClient::with_timeout(&args.cl_beacon_api, timeout);

    // Step 3: Execute Cross-Layer Verification
    let normalized_txs = args.normalized_el_txs();
    let receipt = VerificationEngine::run_verification(
        &pairs,
        &normalized_txs,
        &el_client,
        &beacon_client,
        args.st_vault_dashboard.as_deref(),
    )
    .await?;

    // Step 4: Generate and Save Artifacts (Markdown, JSON, CSV, Evidence)
    let artifacts = generate_and_save_receipts(&args.output_dir, &receipt)?;

    if !args.quiet {
        println!(
            "💾 Receipts saved to directory: {}",
            args.output_dir.display()
        );
        terminal::print_verification_results(&receipt);
    }

    // Print raw output format if requested (Markdown, JSON, or CSV)
    terminal::print_requested_format(
        args.format,
        &artifacts.markdown,
        &artifacts.json,
        &artifacts.csv,
    );

    Ok(receipt.summary.is_all_accepted())
}
