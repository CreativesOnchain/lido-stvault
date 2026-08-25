use clap::Parser;
use colored::*;
use std::process::ExitCode;
use stvault_receipt::cli::CliArgs;
use stvault_receipt::terminal;
use stvault_receipt::{
    BeaconClient, ElClient, VerificationEngine, generate_and_save_receipts, parse_manifest_file,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args = CliArgs::parse();

    // Handle shell completion generation if requested
    if let Some(shell) = args.generate_completions {
        CliArgs::print_completions(shell);
        return ExitCode::SUCCESS;
    }

    let Some(manifest_path) = args.manifest.as_ref() else {
        eprintln!(
            "{} Missing required argument: --manifest <PATH>",
            "ERROR:".bold().red()
        );
        return ExitCode::from(2);
    };

    if !args.quiet {
        terminal::print_banner();
        println!("📂 Parsing manifest: {}", manifest_path.display());
    }

    // Step 1: Parse Manifest
    let pairs = match parse_manifest_file(manifest_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} Failed to parse manifest: {}", "ERROR:".bold().red(), e);
            return ExitCode::from(e.exit_code());
        }
    };

    if !args.quiet {
        terminal::print_connection_info(pairs.len(), &args.el_rpc, &args.cl_beacon_api);
    }

    // Step 2: Initialize RPC Clients
    let el_client = ElClient::new(&args.el_rpc);
    let beacon_client = BeaconClient::new(&args.cl_beacon_api);

    // Step 3: Execute Cross-Layer Verification
    let normalized_txs = args.normalized_el_txs();
    let receipt = match VerificationEngine::run_verification(
        &pairs,
        &normalized_txs,
        &el_client,
        &beacon_client,
        args.st_vault_dashboard.as_deref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{} Verification failed: {}", "ERROR:".bold().red(), e);
            return ExitCode::from(e.exit_code());
        }
    };

    // Step 4: Generate and Save Artifacts (Markdown, JSON, CSV, Evidence)
    let artifacts = match generate_and_save_receipts(&args.output_dir, &receipt) {
        Ok(bundle) => {
            if !args.quiet {
                println!(
                    "💾 Receipts saved to directory: {}",
                    args.output_dir.display()
                );
            }
            bundle
        }
        Err(e) => {
            eprintln!(
                "{} Failed to generate or save receipt artifacts: {}",
                "WARNING:".bold().yellow(),
                e
            );
            return ExitCode::from(1);
        }
    };

    // Step 5: Terminal Output Presentation
    if !args.quiet {
        terminal::print_verification_results(&receipt);
    }

    // Print raw output format if requested (Markdown, JSON, or CSV)
    terminal::print_requested_format(
        args.format,
        &artifacts.markdown,
        &artifacts.json,
        &artifacts.csv,
    );

    // Exit Code: 0 if all accepted, 1 if any not accepted / indeterminate
    if receipt.summary.is_all_accepted() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
