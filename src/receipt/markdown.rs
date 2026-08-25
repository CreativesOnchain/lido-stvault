use crate::models::{ConsolidationStatus, VerificationReceipt};

pub fn generate_markdown_receipt(receipt: &VerificationReceipt) -> String {
    let mut md = String::new();

    md.push_str("# Lido stVault Consolidation Request Receipt\n\n");
    md.push_str(&format!(
        "**Generated At:** {}\n",
        receipt.timestamp.to_rfc3339()
    ));
    md.push_str(&format!(
        "**CLI Tool Version:** v{}\n",
        receipt.tool_version
    ));
    md.push_str(&format!(
        "**Execution Layer RPC:** `{}`\n",
        receipt.el_rpc_url
    ));
    md.push_str(&format!(
        "**Consensus Beacon API:** `{}`\n\n",
        receipt.cl_beacon_url
    ));

    // Overall Status Banner
    let is_all_accepted =
        receipt.summary.accepted == receipt.summary.total_pairs && receipt.summary.total_pairs > 0;
    if is_all_accepted {
        md.push_str("> [!NOTE]\n");
        md.push_str("> **STATUS: ALL CONSOLIDATION REQUESTS ACCEPTED**\n");
        md.push_str("> All consolidation pairs have been verified in the Consensus Layer pending queue.\n\n");
    } else if receipt.summary.not_accepted > 0 || receipt.summary.indeterminate > 0 {
        md.push_str("> [!WARNING]\n");
        md.push_str("> **STATUS: ATTENTION REQUIRED**\n");
        md.push_str(
            "> One or more consolidation requests were NOT accepted or returned INDETERMINATE.\n",
        );
        md.push_str(
            "> Do NOT decommission source validator hardware until all pairs are verified.\n\n",
        );
    }

    // Summary Metrics Table
    md.push_str("## 1. Summary Metrics\n\n");
    md.push_str("| Metric | Count | Percentage |\n");
    md.push_str("| :--- | :--- | :--- |\n");
    let total = receipt.summary.total_pairs;
    let pct = |count: usize| {
        if total > 0 {
            (count as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    };

    md.push_str(&format!(
        "| **Total Validator Pairs** | `{}` | `100.0%` |\n",
        total
    ));
    md.push_str(&format!(
        "| **Accepted (In CL Pending Queue)** | `{}` | `{:.1}%` |\n",
        receipt.summary.accepted,
        pct(receipt.summary.accepted)
    ));
    md.push_str(&format!(
        "| **Queued (Awaiting CL State)** | `{}` | `{:.1}%` |\n",
        receipt.summary.queued,
        pct(receipt.summary.queued)
    ));
    md.push_str(&format!(
        "| **Not Accepted / Dropped** | `{}` | `{:.1}%` |\n",
        receipt.summary.not_accepted,
        pct(receipt.summary.not_accepted)
    ));
    md.push_str(&format!(
        "| **Indeterminate / Unverified** | `{}` | `{:.1}%` |\n\n",
        receipt.summary.indeterminate,
        pct(receipt.summary.indeterminate)
    ));

    // Lido Fee Exemption Inspection
    md.push_str("## 2. Lido Fee-Exemption Role Audit\n\n");
    md.push_str(&format!(
        "- **stVault Dashboard / ACL:** `{}`\n",
        receipt
            .fee_exemption
            .st_vault_dashboard
            .as_deref()
            .unwrap_or("N/A")
    ));
    md.push_str(&format!(
        "- **Operator Address:** `{}`\n",
        receipt
            .fee_exemption
            .operator_address
            .as_deref()
            .unwrap_or("N/A")
    ));
    md.push_str(&format!(
        "- **`NODE_OPERATOR_FEE_EXEMPT_ROLE` Hash:** `{}`\n",
        receipt.fee_exemption.role_hash
    ));
    let role_status_str = match receipt.fee_exemption.role_active {
        Some(true) => "⚠️ ACTIVE (Elevated privilege active)",
        Some(false) => "✅ INACTIVE (Revoked or clean)",
        None => "ℹ️ UNCHECKED / NOT PROVIDED",
    };
    md.push_str(&format!("- **Current Role State:** {}\n", role_status_str));
    md.push_str(&format!(
        "- **Audit Notes:** {}\n\n",
        receipt.fee_exemption.notes
    ));

    // Pair-by-pair table
    md.push_str("## 3. Pair-by-Pair Consolidation Verification\n\n");
    md.push_str("| # | Source Validator | Target Validator | EL Tx Hash | Status | Details |\n");
    md.push_str("| :- | :--- | :--- | :--- | :--- | :--- |\n");

    for (i, pair) in receipt.pairs.iter().enumerate() {
        let src_display = if let Some(idx) = pair.source_index {
            format!("Index `{}` (`{}...`)", idx, &pair.source_pubkey[0..10])
        } else {
            format!("`{}...`", &pair.source_pubkey[0..12])
        };

        let tgt_display = if let Some(idx) = pair.target_index {
            format!("Index `{}` (`{}...`)", idx, &pair.target_pubkey[0..10])
        } else {
            format!("`{}...`", &pair.target_pubkey[0..12])
        };

        let tx_display = pair
            .el_tx_hash
            .as_deref()
            .map(|h| {
                if h.len() > 14 {
                    format!("`{}...{}`", &h[0..8], &h[h.len() - 6..])
                } else {
                    format!("`{}`", h)
                }
            })
            .unwrap_or_else(|| "N/A".to_string());

        let status_badge = match pair.status {
            ConsolidationStatus::Accepted => "✅ **ACCEPTED**",
            ConsolidationStatus::Queued => "⏳ **QUEUED**",
            ConsolidationStatus::NotAccepted => "❌ **NOT_ACCEPTED**",
            ConsolidationStatus::Indeterminate => "❓ **INDETERMINATE**",
        };

        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            i + 1,
            src_display,
            tgt_display,
            tx_display,
            status_badge,
            pair.details
        ));
    }

    md.push_str("\n---\n*Generated by `stvault-receipt` (Lido stVault Consolidation Verifier)*\n");
    md
}
