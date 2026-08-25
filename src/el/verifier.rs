use super::client::ElClient;
use super::predeploy::ConsolidationPredeploy;
use super::types::{ElVerificationEvidence, ElVerifiedTx};
use crate::error::{AppError, Result};
use crate::models::ConsolidationPair;
use std::collections::HashMap;

pub async fn verify_execution_layer(
    client: &ElClient,
    tx_hashes: &[String],
    manifest_pairs: &[ConsolidationPair],
) -> Result<ElVerificationEvidence> {
    let mut verified_txs = HashMap::new();
    let mut pair_to_tx_map = HashMap::new();
    let mut raw_receipts = HashMap::new();
    let mut raw_blocks = HashMap::new();

    for tx_hash in tx_hashes {
        let receipt = client
            .get_transaction_receipt(tx_hash)
            .await?
            .ok_or_else(|| {
                AppError::ElRpc(format!("Transaction receipt not found for {}", tx_hash))
            })?;

        raw_receipts.insert(tx_hash.clone(), receipt.raw.clone());

        let tx_details = client.get_transaction_by_hash(tx_hash).await.ok().flatten();

        let block = client
            .get_block_by_number(receipt.block_number)
            .await?
            .ok_or_else(|| {
                AppError::ElRpc(format!(
                    "Block not found for number {}",
                    receipt.block_number
                ))
            })?;

        let block_timestamp = block.timestamp;
        raw_blocks.insert(receipt.block_number, block);

        // Check if "to" is Predeploy directly or if calldata / logs contain interaction
        let mut predeploy_interaction = false;
        if let Some(ref to) = receipt.to
            && ConsolidationPredeploy::is_predeploy_address(to)
        {
            predeploy_interaction = true;
        }

        // Check logs for predeploy address
        for log in &receipt.logs {
            if ConsolidationPredeploy::is_predeploy_address(&log.address) {
                predeploy_interaction = true;
            }
        }

        let mut matched_pairs = Vec::new();

        // Check calldata if available
        if let Some(ref details) = tx_details {
            let input_clean = details.input.trim_start_matches("0x");
            if let Ok(calldata_bytes) = hex::decode(input_clean) {
                // If direct calldata to predeploy
                if let Some(direct_pair) =
                    ConsolidationPredeploy::decode_predeploy_calldata(&calldata_bytes)
                {
                    predeploy_interaction = true;
                    if manifest_pairs.contains(&direct_pair) {
                        matched_pairs.push(direct_pair);
                    }
                } else {
                    // Match calldata byte patterns against manifest
                    let matches = ConsolidationPredeploy::match_pairs_in_calldata(
                        &calldata_bytes,
                        manifest_pairs,
                    );
                    if !matches.is_empty() {
                        predeploy_interaction = true;
                        matched_pairs.extend(matches);
                    }
                }
            }
        }

        // If no direct calldata match was found (e.g. nested contract calls),
        // but tx succeeded and manifest has exactly 1 pair per tx, associate cautiously
        if matched_pairs.is_empty()
            && manifest_pairs.len() == tx_hashes.len()
            && let Some(idx) = tx_hashes.iter().position(|h| h == tx_hash)
            && let Some(pair) = manifest_pairs.get(idx)
        {
            matched_pairs.push(pair.clone());
        }

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
            predeploy_interaction_detected: predeploy_interaction,
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
