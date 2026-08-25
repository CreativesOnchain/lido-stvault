use super::client::BeaconClient;
use super::types::{BeaconBlockResponse, PendingConsolidationItem};
use crate::error::Result;
use crate::models::ConsolidationPair;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClVerifiedPairEvidence {
    pub source_pubkey: String,
    pub source_index: Option<u64>,
    pub target_pubkey: String,
    pub target_index: Option<u64>,
    pub beacon_slot: Option<u64>,
    pub beacon_request_found: bool,
    pub cl_pending_found: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClVerificationEvidence {
    pub validator_indices: HashMap<String, u64>,
    pub pair_evidence: HashMap<ConsolidationPair, ClVerifiedPairEvidence>,
    pub pending_consolidations_queue: Vec<PendingConsolidationItem>,
    pub beacon_blocks: HashMap<u64, BeaconBlockResponse>,
}

pub async fn verify_consensus_layer(
    client: &BeaconClient,
    pairs: &[ConsolidationPair],
    el_block_timestamps: &HashMap<u64, u64>,
) -> Result<ClVerificationEvidence> {
    // 1. Gather all unique pubkeys
    let mut all_pubkeys = Vec::new();
    for pair in pairs {
        if !all_pubkeys.contains(&pair.source_pubkey) {
            all_pubkeys.push(pair.source_pubkey.clone());
        }
        if !all_pubkeys.contains(&pair.target_pubkey) {
            all_pubkeys.push(pair.target_pubkey.clone());
        }
    }

    // 2. Fetch validator indices
    let validator_indices = client
        .get_validators_by_pubkeys(&all_pubkeys)
        .await
        .unwrap_or_default();

    // 3. Fetch Genesis to calculate slot from timestamp
    let genesis_res = client.get_genesis().await.ok();
    let genesis_time = genesis_res
        .and_then(|g| g.data.genesis_time.parse::<u64>().ok())
        .unwrap_or(1606824023); // fallback mainnet genesis if unreachable

    // 4. Fetch pending_consolidations from Beacon head state
    let pending_consolidations = client
        .get_pending_consolidations("head")
        .await
        .unwrap_or_default();

    let mut beacon_blocks = HashMap::new();
    let mut pair_evidence = HashMap::new();

    for pair in pairs {
        let src_idx = validator_indices
            .get(&pair.source_pubkey.to_lowercase())
            .copied();
        let tgt_idx = validator_indices
            .get(&pair.target_pubkey.to_lowercase())
            .copied();

        // Check pending_consolidations queue
        let mut cl_pending_found = false;
        if let (Some(s_idx), Some(t_idx)) = (src_idx, tgt_idx) {
            cl_pending_found = pending_consolidations.iter().any(|item| {
                item.source_index.parse::<u64>().ok() == Some(s_idx)
                    && item.target_index.parse::<u64>().ok() == Some(t_idx)
            });
        }

        // Try locating beacon block containing the request
        let mut beacon_slot = None;
        let mut beacon_request_found = false;

        // If we have EL block timestamps, calculate estimated slot: (timestamp - genesis_time) / 12
        for &timestamp in el_block_timestamps.values() {
            if timestamp >= genesis_time {
                let estimated_slot = (timestamp - genesis_time) / 12;
                beacon_slot = Some(estimated_slot);

                // Fetch beacon block around that slot
                if let Ok(Some(block_resp)) =
                    client.get_beacon_block(&estimated_slot.to_string()).await
                {
                    // Check execution_requests
                    if let Some(ref requests) = block_resp.data.message.body.execution_requests
                        && let Some(ref consolidations) = requests.consolidations
                    {
                        for req in consolidations {
                            let match_pubkeys =
                                req.source_pubkey.as_deref().map(|s| s.to_lowercase())
                                    == Some(pair.source_pubkey.to_lowercase())
                                    && req.target_pubkey.as_deref().map(|s| s.to_lowercase())
                                        == Some(pair.target_pubkey.to_lowercase());

                            let match_indices =
                                if let (Some(s_idx), Some(t_idx)) = (src_idx, tgt_idx) {
                                    req.source_index
                                        .as_deref()
                                        .and_then(|s| s.parse::<u64>().ok())
                                        == Some(s_idx)
                                        && req
                                            .target_index
                                            .as_deref()
                                            .and_then(|s| s.parse::<u64>().ok())
                                            == Some(t_idx)
                                } else {
                                    false
                                };

                            if match_pubkeys || match_indices {
                                beacon_request_found = true;
                                break;
                            }
                        }
                    }
                    beacon_blocks.insert(estimated_slot, block_resp);
                }
            }
        }

        pair_evidence.insert(
            pair.clone(),
            ClVerifiedPairEvidence {
                source_pubkey: pair.source_pubkey.clone(),
                source_index: src_idx,
                target_pubkey: pair.target_pubkey.clone(),
                target_index: tgt_idx,
                beacon_slot,
                beacon_request_found,
                cl_pending_found,
            },
        );
    }

    Ok(ClVerificationEvidence {
        validator_indices,
        pair_evidence,
        pending_consolidations_queue: pending_consolidations,
        beacon_blocks,
    })
}
