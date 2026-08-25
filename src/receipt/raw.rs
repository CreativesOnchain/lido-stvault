use crate::error::Result;
use crate::models::VerificationReceipt;
use std::fs;
use std::path::Path;

pub struct EvidenceWriter;

impl EvidenceWriter {
    /// Saves all receipts and raw evidence to the output directory.
    pub fn save_all<P: AsRef<Path>>(
        output_dir: P,
        receipt: &VerificationReceipt,
        markdown: &str,
        json_str: &str,
        csv_str: &str,
    ) -> Result<()> {
        let dir = output_dir.as_ref();
        fs::create_dir_all(dir)?;

        let md_path = dir.join("receipt_summary.md");
        fs::write(&md_path, markdown)?;

        let json_path = dir.join("receipt.json");
        fs::write(&json_path, json_str)?;

        let csv_path = dir.join("consolidations.csv");
        fs::write(&csv_path, csv_str)?;

        let evidence_dir = dir.join("evidence");
        fs::create_dir_all(&evidence_dir)?;

        let meta_path = evidence_dir.join("verification_metadata.json");
        let meta_json = serde_json::to_string_pretty(&serde_json::json!({
            "timestamp": receipt.timestamp,
            "tool_version": receipt.tool_version,
            "el_rpc_url": receipt.el_rpc_url,
            "cl_beacon_url": receipt.cl_beacon_url,
            "summary": receipt.summary,
            "fee_exemption": receipt.fee_exemption,
        }))?;
        fs::write(&meta_path, meta_json)?;

        Ok(())
    }
}
