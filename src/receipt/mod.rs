pub mod csv;
pub mod json;
pub mod markdown;
pub mod raw;

pub use csv::generate_csv_receipt;
pub use json::generate_json_receipt;
pub use markdown::generate_markdown_receipt;
pub use raw::EvidenceWriter;

use crate::error::Result;
use crate::models::VerificationReceipt;
use std::path::Path;

/// In-memory rendered receipt artifacts.
#[derive(Debug, Clone)]
pub struct ReceiptArtifacts {
    pub markdown: String,
    pub json: String,
    pub csv: String,
}

/// Generates all receipt formats (Markdown, JSON, CSV) and persists them along with raw evidence.
pub fn generate_and_save_receipts<P: AsRef<Path>>(
    output_dir: P,
    receipt: &VerificationReceipt,
) -> Result<ReceiptArtifacts> {
    let markdown = generate_markdown_receipt(receipt);
    let json = generate_json_receipt(receipt)?;
    let csv = generate_csv_receipt(receipt)?;

    EvidenceWriter::save_all(output_dir, receipt, &markdown, &json, &csv)?;

    Ok(ReceiptArtifacts {
        markdown,
        json,
        csv,
    })
}
