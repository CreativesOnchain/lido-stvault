use crate::error::{AppError, Result};
use crate::models::{ConsolidationStatus, VerificationReceipt};
use serde::Serialize;

/// Borrowed, zero-copy CSV record mapping for high-throughput serialization.
#[derive(Serialize)]
struct CsvRow<'a> {
    pair_number: usize,
    source_pubkey: &'a str,
    source_index: Option<u64>,
    target_pubkey: &'a str,
    target_index: Option<u64>,
    withdrawal_credentials: Option<&'a str>,
    derived_source_address: Option<&'a str>,
    el_tx_hash: Option<&'a str>,
    el_block_number: Option<u64>,
    el_predeploy_found: bool,
    beacon_slot: Option<u64>,
    beacon_request_found: bool,
    parent_state_absent: Option<bool>,
    post_state_present: Option<bool>,
    block_finalized: Option<bool>,
    status: ConsolidationStatus,
    details: &'a str,
    indeterminate_reason: Option<&'a str>,
}

/// Generates a CSV formatted receipt string from verification results with zero redundant allocations.
pub fn generate_csv_receipt(receipt: &VerificationReceipt) -> Result<String> {
    // Pre-allocate buffer (~256 bytes per row estimate)
    let estimated_capacity = 256 + (receipt.pairs.len() * 256);
    let mut wtr = csv::WriterBuilder::new().from_writer(Vec::with_capacity(estimated_capacity));

    for (index, pair) in receipt.pairs.iter().enumerate() {
        let row = CsvRow {
            pair_number: index + 1,
            source_pubkey: &pair.source_pubkey,
            source_index: pair.source_index,
            target_pubkey: &pair.target_pubkey,
            target_index: pair.target_index,
            withdrawal_credentials: pair.withdrawal_credentials.as_deref(),
            derived_source_address: pair.derived_source_address.as_deref(),
            el_tx_hash: pair.el_tx_hash.as_deref(),
            el_block_number: pair.el_block_number,
            el_predeploy_found: pair.el_predeploy_found,
            beacon_slot: pair.beacon_slot,
            beacon_request_found: pair.beacon_request_found,
            parent_state_absent: pair.parent_state_absent,
            post_state_present: pair.post_state_present,
            block_finalized: pair.block_finalized,
            status: pair.status,
            details: &pair.details,
            indeterminate_reason: pair.indeterminate_reason.as_deref(),
        };

        wtr.serialize(row)?;
    }

    let bytes = wtr.into_inner().map_err(|e| e.into_error())?;
    String::from_utf8(bytes)
        .map_err(|e| AppError::Evaluation(format!("CSV UTF-8 encoding error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LidoFeeExemptionReport, PairVerificationResult, VerificationSummary};
    use chrono::Utc;

    #[test]
    fn test_generate_csv_receipt() {
        let receipt = VerificationReceipt {
            tool_version: "0.1.0".to_string(),
            timestamp: Utc::now(),
            el_rpc_url: "http://localhost:8545".to_string(),
            cl_beacon_url: "http://localhost:5052".to_string(),
            summary: VerificationSummary {
                total_pairs: 1,
                accepted: 1,
                ..Default::default()
            },
            fee_exemption: LidoFeeExemptionReport {
                st_vault_dashboard: None,
                role_name: "vaults.NodeOperatorFee.FeeExemptRole".to_string(),
                role_hash: "0x123".to_string(),
                audited_sources: vec![],
                fee_exemption_observed: false,
                notes: "Skipped".to_string(),
            },
            pairs: vec![PairVerificationResult {
                source_pubkey: "0xsource".to_string(),
                source_index: Some(101),
                target_pubkey: "0xtarget".to_string(),
                target_index: Some(202),
                withdrawal_credentials: Some("0x01".to_string()),
                derived_source_address: Some("0xaddr".to_string()),
                el_tx_hash: Some("0xtx".to_string()),
                el_block_number: Some(500),
                el_predeploy_found: true,
                beacon_slot: Some(1000),
                beacon_request_found: true,
                parent_state_absent: Some(true),
                post_state_present: Some(true),
                block_finalized: Some(true),
                status: ConsolidationStatus::Accepted,
                details: "All checks passed".to_string(),
                indeterminate_reason: None,
            }],
            raw_evidence: None,
        };

        let csv_output = generate_csv_receipt(&receipt).expect("CSV generation failed");
        assert!(csv_output.contains("pair_number,source_pubkey,source_index"));
        assert!(csv_output.contains("ACCEPTED,All checks passed,"));
    }
}
