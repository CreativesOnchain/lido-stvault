use super::client::ElClient;
use super::predeploy::ConsolidationPredeploy;
use super::types::{BlockDetails, ElVerificationEvidence, ElVerifiedTx, TxDetails, TxReceipt};
use crate::error::{AppError, Result};
use crate::models::ConsolidationPair;
use std::collections::HashMap;

/// Verifies execution layer transactions, extracts predeploy calls, and matches manifest pairs.
pub async fn verify_execution_layer(
    client: &ElClient,
    tx_hashes: &[String],
    manifest_pairs: &[ConsolidationPair],
) -> Result<ElVerificationEvidence> {
    let mut verified_txs = HashMap::with_capacity(tx_hashes.len());
    let mut pair_to_tx_map = HashMap::with_capacity(manifest_pairs.len());
    let mut raw_receipts = HashMap::with_capacity(tx_hashes.len());
    let mut raw_blocks = HashMap::with_capacity(tx_hashes.len());

    for tx_hash in tx_hashes {
        // 1. Fetch transaction receipt (fail closed if receipt not found)
        let receipt = client
            .get_transaction_receipt(tx_hash)
            .await?
            .ok_or_else(|| {
                AppError::ElRpc(format!("Transaction receipt not found for {}", tx_hash))
            })?;

        raw_receipts.insert(tx_hash.clone(), receipt.raw.clone());

        // 2. Fetch transaction input calldata
        let tx_details = client.get_transaction_by_hash(tx_hash).await?;

        // 3. Fetch block details (with local caching to avoid duplicate queries for same block)
        let block = resolve_tx_block(client, &mut raw_blocks, receipt.block_number).await?;
        let block_timestamp = block.timestamp;

        // 4. Detect predeploy interactions and match pairs from exact 96-byte calldata chunks
        let (predeploy_detected, matched_pairs) =
            analyze_tx_interaction(&receipt, tx_details.as_ref(), manifest_pairs);

        for pair in &matched_pairs {
            pair_to_tx_map.insert(pair.clone(), tx_hash.clone());
        }

        let verified = ElVerifiedTx {
            tx_hash: tx_hash.clone(),
            status_success: receipt.status,
            block_number: receipt.block_number,
            block_hash: receipt.block_hash.clone(),
            block_timestamp,
            from: receipt.from.clone(),
            to: receipt.to.clone(),
            predeploy_interaction_detected: predeploy_detected,
            matched_manifest_pairs: matched_pairs,
            receipt,
            details: tx_details,
        };

        verified_txs.insert(tx_hash.clone(), verified);
    }

    Ok(ElVerificationEvidence {
        verified_txs,
        pair_to_tx_map,
        raw_receipts,
        raw_blocks,
    })
}

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

/// Fetches block details by number or retrieves from the local cache if already fetched.
async fn resolve_tx_block(
    client: &ElClient,
    cache: &mut HashMap<u64, BlockDetails>,
    block_number: u64,
) -> Result<BlockDetails> {
    if let Some(cached) = cache.get(&block_number) {
        return Ok(cached.clone());
    }

    let block = client
        .get_block_by_number(block_number)
        .await?
        .ok_or_else(|| AppError::ElRpc(format!("Block not found for number {}", block_number)))?;

    cache.insert(block_number, block.clone());
    Ok(block)
}

/// Analyzes receipt logs, target address, and calldata for consolidation predeploy evidence.
fn analyze_tx_interaction(
    receipt: &TxReceipt,
    tx_details: Option<&TxDetails>,
    manifest_pairs: &[ConsolidationPair],
) -> (bool, Vec<ConsolidationPair>) {
    let mut predeploy_found = detect_predeploy_in_receipt(receipt);
    let mut matched_pairs = Vec::new();

    if let Some(details) = tx_details {
        let (calldata_predeploy, pairs) =
            extract_pairs_from_calldata(&details.input, manifest_pairs);
        if calldata_predeploy {
            predeploy_found = true;
        }
        matched_pairs.extend(pairs);
    }

    (predeploy_found, matched_pairs)
}

/// Checks whether the receipt target address or any emitted log addresses match the predeploy.
fn detect_predeploy_in_receipt(receipt: &TxReceipt) -> bool {
    let to_is_predeploy = receipt
        .to
        .as_deref()
        .map(ConsolidationPredeploy::is_predeploy_address)
        .unwrap_or(false);

    let log_matches_predeploy = receipt
        .logs
        .iter()
        .any(|log| ConsolidationPredeploy::is_predeploy_address(&log.address));

    to_is_predeploy || log_matches_predeploy
}

/// Extracts consolidation pairs from transaction calldata using exact 96-byte chunk matching.
fn extract_pairs_from_calldata(
    input_hex: &str,
    manifest_pairs: &[ConsolidationPair],
) -> (bool, Vec<ConsolidationPair>) {
    let clean = input_hex.trim().trim_start_matches("0x");
    let Ok(calldata_bytes) = hex::decode(clean) else {
        return (false, Vec::new());
    };

    // Direct 96-byte calldata to predeploy
    if let Some(direct_pair) = ConsolidationPredeploy::decode_predeploy_calldata(&calldata_bytes) {
        let matched = if manifest_pairs.contains(&direct_pair) {
            vec![direct_pair]
        } else {
            Vec::new()
        };
        return (true, matched);
    }

    // Byte pattern search within batch/multicall calldata for exact 96-byte sequences
    let matches = ConsolidationPredeploy::match_pairs_in_calldata(&calldata_bytes, manifest_pairs);
    let found = !matches.is_empty();
    (found, matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_predeploy_in_receipt() {
        let receipt = TxReceipt {
            transaction_hash: "0x123".to_string(),
            block_number: 1,
            block_hash: "0xabc".to_string(),
            status: true,
            gas_used: 21000,
            from: "0xuser".to_string(),
            to: Some("0x0000bbddc7ce488642fb579f8b00f3a590007251".to_string()),
            logs: vec![],
            raw: serde_json::Value::Null,
        };
        assert!(detect_predeploy_in_receipt(&receipt));
    }
}
