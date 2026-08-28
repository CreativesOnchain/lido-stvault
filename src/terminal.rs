use crate::cli::OutputFormat;
use crate::models::{ConsolidationStatus, VerificationReceipt};
use colored::*;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};

/// Prints the ASCII banner for stvault-receipt.
pub fn print_banner() {
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

/// Prints connection endpoints and manifest item counts.
pub fn print_connection_info(pair_count: usize, el_rpc: &str, cl_beacon_api: &str) {
    println!("   Found {} consolidation pairs in manifest.", pair_count);
    println!("⚡ Connecting to Execution Layer RPC: {}", el_rpc);
    println!("📡 Connecting to Consensus Beacon API: {}", cl_beacon_api);
    println!("🔍 Running cross-layer verification...");
}

/// Renders the full comfy-table results and summary status in the terminal.
pub fn print_verification_results(receipt: &VerificationReceipt) {
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
                .map(|idx| format!("#{}", idx))
                .unwrap_or_default(),
            &pair.source_pubkey[..pair.source_pubkey.len().min(10)]
        );
        let tgt_text = format!(
            "{} ({})",
            pair.target_index
                .map(|idx| format!("#{}", idx))
                .unwrap_or_default(),
            &pair.target_pubkey[..pair.target_pubkey.len().min(10)]
        );
        let tx_text = pair
            .el_tx_hash
            .as_deref()
            .map(|h| format!("{}...", &h[..h.len().min(10)]))
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

    if receipt.fee_exemption.is_any_role_active() {
        println!(
            "\n{}",
            "⚠️  WARNING: vaults.NodeOperatorFee.FeeExemptRole is currently ACTIVE on one or more source validator accounts."
                .bold()
                .yellow()
        );
    }
}

/// Prints the requested output format payload to standard output.
pub fn print_requested_format(format: OutputFormat, markdown: &str, json_str: &str, csv_str: &str) {
    match format {
        OutputFormat::Markdown => println!("{}", markdown),
        OutputFormat::Json => println!("{}", json_str),
        OutputFormat::Csv => println!("{}", csv_str),
        OutputFormat::All => {}
    }
}
