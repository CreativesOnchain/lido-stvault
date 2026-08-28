use crate::error::{AppError, Result};
use crate::models::{LidoFeeExemptionReport, VerificationReceipt, VerificationSummary};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;

/// Strongly-typed, borrowed metadata serializer for the raw evidence dump.
#[derive(Serialize)]
struct EvidenceMetadata<'a> {
    timestamp: &'a DateTime<Utc>,
    tool_version: &'a str,
    el_rpc_url: &'a str,
    cl_beacon_url: &'a str,
    summary: &'a VerificationSummary,
    fee_exemption: &'a LidoFeeExemptionReport,
}

pub struct EvidenceWriter;

impl EvidenceWriter {
    /// Saves all human-readable summaries, machine-readable JSON/CSV receipts,
    /// and raw evidence artifacts to the specified output directory.
    pub fn save_all<P: AsRef<Path>>(
        output_dir: P,
        receipt: &VerificationReceipt,
        markdown: &str,
        json_str: &str,
        csv_str: &str,
    ) -> Result<()> {
        let dir = output_dir.as_ref();
        fs::create_dir_all(dir).map_err(|e| {
            AppError::Io(io::Error::other(format!(
                "Failed to create output directory '{}': {}",
                dir.display(),
                e
            )))
        })?;

        // 1. Write Markdown Summary
        write_file(dir.join("receipt_summary.md"), markdown)?;

        // 2. Write JSON Receipt
        write_file(dir.join("receipt.json"), json_str)?;

        // 3. Write CSV Spreadsheet
        write_file(dir.join("consolidations.csv"), csv_str)?;

        // 4. Write Evidence Metadata
        let evidence_dir = dir.join("evidence");
        fs::create_dir_all(&evidence_dir).map_err(|e| {
            AppError::Io(io::Error::other(format!(
                "Failed to create evidence directory '{}': {}",
                evidence_dir.display(),
                e
            )))
        })?;

        let meta = EvidenceMetadata {
            timestamp: &receipt.timestamp,
            tool_version: &receipt.tool_version,
            el_rpc_url: &receipt.el_rpc_url,
            cl_beacon_url: &receipt.cl_beacon_url,
            summary: &receipt.summary,
            fee_exemption: &receipt.fee_exemption,
        };

        let meta_json = serde_json::to_string_pretty(&meta)?;
        write_file(evidence_dir.join("verification_metadata.json"), &meta_json)?;

        Ok(())
    }
}

/// Helper function to write content to disk with rich path error messages.
fn write_file<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, content: C) -> Result<()> {
    let p = path.as_ref();
    fs::write(p, content).map_err(|e| {
        AppError::Io(io::Error::other(format!(
            "Failed to write file to '{}': {}",
            p.display(),
            e
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConsolidationStatus, PairVerificationResult};
    use tempfile::tempdir;

    #[test]
    fn test_evidence_writer_save_all() {
        let temp_dir = tempdir().expect("failed to create temp dir");
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
                source_pubkey: "0x01".to_string(),
                source_index: Some(1),
                target_pubkey: "0x02".to_string(),
                target_index: Some(2),
                withdrawal_credentials: Some("0x01".to_string()),
                derived_source_address: Some("0xaddr".to_string()),
                el_tx_hash: Some("0x123".to_string()),
                el_block_number: Some(100),
                el_predeploy_found: true,
                beacon_slot: Some(200),
                beacon_request_found: true,
                parent_state_absent: Some(true),
                post_state_present: Some(true),
                block_finalized: Some(true),
                status: ConsolidationStatus::Accepted,
                details: "OK".to_string(),
                indeterminate_reason: None,
            }],
        };

        let res = EvidenceWriter::save_all(
            temp_dir.path(),
            &receipt,
            "# Summary",
            "{\"status\": \"ok\"}",
            "header\n1,2",
        );
        assert!(res.is_ok());

        assert!(temp_dir.path().join("receipt_summary.md").exists());
        assert!(temp_dir.path().join("receipt.json").exists());
        assert!(temp_dir.path().join("consolidations.csv").exists());
        assert!(
            temp_dir
                .path()
                .join("evidence/verification_metadata.json")
                .exists()
        );
    }
}
