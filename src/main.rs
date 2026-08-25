use clap::Parser;
use colored::*;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use std::process::ExitCode;
use stvault_receipt::cli::{CliArgs, OutputFormat};
use stvault_receipt::models::ConsolidationStatus;
use stvault_receipt::{
    generate_csv_receipt, generate_json_receipt, generate_markdown_receipt, parse_manifest_file,
    BeaconClient, ElClient, EvidenceWriter, VerificationEngine,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args = CliArgs::parse();

    // Check if user requested shell autocompletions
    if let Some(shell) = args.generate_completions {
        CliArgs::print_completions(shell);
        return ExitCode::SUCCESS;
    }

    let manifest_path = match args.manifest.as_ref() {
        Some(p) => p,
        None => {
            eprintln!(
                "{} Missing required argument: --manifest <PATH>",
                "ERROR:".bold().red()
            );
            return ExitCode::from(2);
        }
    };

    if !args.quiet {
        println!(
            "{}",
            "=========================================================".cyan()
        );
        println!(
            "{}",
            "       Lido stVault Consolidation Request Receipt        "
                .bold()
                .cyan()
        );
        println!(
            "{}",
            "=========================================================".cyan()
        );
    }

    // Step 1: Parse Manifest
    if !args.quiet {
        println!("📂 Parsing manifest: {}", manifest_path.display());
    }
    let pairs = match parse_manifest_file(manifest_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} Failed to parse manifest: {}", "ERROR:".bold().red(), e);
            return ExitCode::from(2);
        }
    };

    if !args.quiet {
        println!("   Found {} consolidation pairs in manifest.", pairs.len());
        println!("⚡ Connecting to Execution Layer RPC: {}", args.el_rpc);
        println!(
            "📡 Connecting to Consensus Beacon API: {}",
            args.cl_beacon_api
        );
    }

    // Step 2: Initialize Clients
    let el_client = ElClient::new(&args.el_rpc);
    let beacon_client = BeaconClient::new(&args.cl_beacon_api);

    // Step 3: Run Verification
    if !args.quiet {
        println!("🔍 Running cross-layer verification...");
    }

    let receipt = match VerificationEngine::run_verification(
        &pairs,
        &args.el_txs,
        &el_client,
        &beacon_client,
        args.st_vault_dashboard.as_deref(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{} Verification failed: {}", "ERROR:".bold().red(), e);
            return ExitCode::from(1);
        }
    };

    // Step 4: Generate Artifacts
    let markdown = generate_markdown_receipt(&receipt);
    let json_str = match generate_json_receipt(&receipt) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{} Failed to generate JSON receipt: {}",
                "ERROR:".bold().red(),
                e
            );
            return ExitCode::from(1);
        }
    };
    let csv_str = match generate_csv_receipt(&receipt) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{} Failed to generate CSV receipt: {}",
                "ERROR:".bold().red(),
                e
            );
            return ExitCode::from(1);
        }
    };

    // Save evidence & outputs to disk
    if let Err(e) =
        EvidenceWriter::save_all(&args.output_dir, &receipt, &markdown, &json_str, &csv_str)
    {
        eprintln!(
            "{} Failed to save output files: {}",
            "WARNING:".bold().yellow(),
            e
        );
    } else if !args.quiet {
        println!(
            "💾 Receipts saved to directory: {}",
            args.output_dir.display()
        );
    }

    // Step 5: Terminal Output Presentation
    if !args.quiet {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS);
        table.set_header(vec![
            Cell::new("#").fg(Color::Cyan),
            Cell::new("Source Validator").fg(Color::Cyan),
            Cell::new("Target Validator").fg(Color::Cyan),
            Cell::new("EL Tx").fg(Color::Cyan),
            Cell::new("Status").fg(Color::Cyan),
        ]);

        for (i, pair) in receipt.pairs.iter().enumerate() {
            let (status_cell, status_color) = match pair.status {
                ConsolidationStatus::Accepted => ("ACCEPTED", Color::Green),
                ConsolidationStatus::Queued => ("QUEUED", Color::Yellow),
                ConsolidationStatus::NotAccepted => ("NOT_ACCEPTED", Color::Red),
                ConsolidationStatus::Indeterminate => ("INDETERMINATE", Color::Magenta),
            };

            let src_text = format!(
                "{} ({})",
                pair.source_index
                    .map(|i| format!("#{}", i))
                    .unwrap_or_default(),
                &pair.source_pubkey[0..10]
            );
            let tgt_text = format!(
                "{} ({})",
                pair.target_index
                    .map(|i| format!("#{}", i))
                    .unwrap_or_default(),
                &pair.target_pubkey[0..10]
            );
            let tx_text = pair
                .el_tx_hash
                .as_deref()
                .map(|h| format!("{}...", &h[0..10]))
                .unwrap_or_else(|| "N/A".to_string());

            table.add_row(Row::from(vec![
                Cell::new(i + 1),
                Cell::new(src_text),
                Cell::new(tgt_text),
                Cell::new(tx_text),
                Cell::new(status_cell).fg(status_color),
            ]));
        }

        println!("\n{}", table);

        println!("\n{}", "--- Summary ---".bold());
        println!(
            "Total Pairs: {} | Accepted: {} | Queued: {} | Not Accepted: {} | Indeterminate: {}",
            receipt.summary.total_pairs,
            receipt.summary.accepted.to_string().green(),
            receipt.summary.queued.to_string().yellow(),
            receipt.summary.not_accepted.to_string().red(),
            receipt.summary.indeterminate.to_string().magenta(),
        );

        if let Some(active) = receipt.fee_exemption.role_active {
            if active {
                println!(
                    "\n{}",
                    "⚠️  WARNING: NODE_OPERATOR_FEE_EXEMPT_ROLE is currently ACTIVE on stVault Dashboard."
                        .bold()
                        .yellow()
                );
            }
        }
    }

    // Print requested format to stdout if format != All or in quiet mode
    match args.format {
        OutputFormat::Markdown => println!("{}", markdown),
        OutputFormat::Json => println!("{}", json_str),
        OutputFormat::Csv => println!("{}", csv_str),
        OutputFormat::All => {}
    }

    // Exit code: 0 if all accepted, 1 if any failure or indeterminate
    if receipt.summary.accepted == receipt.summary.total_pairs && receipt.summary.total_pairs > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
