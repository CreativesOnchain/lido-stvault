use crate::error::Result;
use crate::models::VerificationReceipt;

pub fn generate_csv_receipt(receipt: &VerificationReceipt) -> Result<String> {
    let mut wtr = csv::WriterBuilder::new().from_writer(vec![]);

    // Header row
    wtr.write_record([
        "pair_number",
        "source_pubkey",
        "source_index",
        "target_pubkey",
        "target_index",
        "el_tx_hash",
        "el_block_number",
        "el_predeploy_found",
        "beacon_slot",
        "beacon_request_found",
        "cl_pending_found",
        "status",
        "details",
        "indeterminate_reason",
    ])?;

    for (i, pair) in receipt.pairs.iter().enumerate() {
        wtr.write_record([
            (i + 1).to_string(),
            pair.source_pubkey.clone(),
            pair.source_index.map(|v| v.to_string()).unwrap_or_default(),
            pair.target_pubkey.clone(),
            pair.target_index.map(|v| v.to_string()).unwrap_or_default(),
            pair.el_tx_hash.clone().unwrap_or_default(),
            pair.el_block_number.map(|v| v.to_string()).unwrap_or_default(),
            pair.el_predeploy_found.to_string(),
            pair.beacon_slot.map(|v| v.to_string()).unwrap_or_default(),
            pair.beacon_request_found.to_string(),
            pair.cl_pending_found.to_string(),
            pair.status.to_string(),
            pair.details.clone(),
            pair.indeterminate_reason.clone().unwrap_or_default(),
        ])?;
    }

    let data = String::from_utf8(wtr.into_inner().map_err(|e| e.into_error())?)
        .map_err(|e| crate::error::AppError::Evaluation(format!("CSV UTF8 error: {}", e)))?;
    Ok(data)
}
