use super::client::BeaconClient;
use super::types::{
    BeaconBlockResponse, ClVerificationEvidence, ClVerifiedPairEvidence, ConsolidationRequestItem,
    PendingConsolidationItem,
};
use crate::error::Result;
use crate::models::ConsolidationPair;
use std::collections::{HashMap, HashSet};

/// Default Ethereum slot duration in seconds (post-Merge / PoS).
pub const SECONDS_PER_SLOT: u64 = 12;

/// Verifies validator consolidation state across the Consensus Layer (Beacon API).
pub async fn verify_consensus_layer(
    client: &BeaconClient,
    pairs: &[ConsolidationPair],
    el_block_timestamps: &HashMap<u64, u64>,
) -> Result<ClVerificationEvidence> {
    if pairs.is_empty() {
        return Ok(ClVerificationEvidence {
            validator_indices: HashMap::new(),
            pair_evidence: HashMap::new(),
            pending_consolidations_queue: Vec::new(),
            beacon_blocks: HashMap::new(),
        });
    }

    // Step 1: Collect unique public keys in O(N) using pre-allocated HashSet
    let all_pubkeys = collect_unique_pubkeys(pairs);

    // Step 2: Fetch validator indices for all pubkeys in batch
    let validator_indices = client
        .get_validators_by_pubkeys(&all_pubkeys)
        .await
        .unwrap_or_default();

    // Step 3: Fetch Genesis to calculate slot from timestamp (network-agnostic)
    let genesis_time = client
        .get_genesis()
        .await
        .ok()
        .and_then(|g| g.data.genesis_time.parse::<u64>().ok());

    // Step 4: Fetch pending_consolidations from Beacon head state
    let pending_consolidations = client
        .get_pending_consolidations("head")
        .await
        .unwrap_or_default();

    // Step 5: Pre-fetch and cache beacon blocks for all distinct timestamps if genesis is known
    let beacon_blocks =
        if let Some(genesis) = genesis_time {
        fetch_beacon_blocks_for_timestamps(client, el_block_timestamps.values().copied(), genesis)
            .await
    } else {
        HashMap::new()
    };

    // Step 6: Evaluate evidence for each pair
    let mut pair_evidence =
        HashMap::with_capacity(pairs.len());

    for pair in pairs {
        let src_idx = validator_indices
            .get(&pair.source_pubkey.to_lowercase())
            .copied();
        let tgt_idx = validator_indices
            .get(&pair.target_pubkey.to_lowercase())
            .copied();

        let cl_pending_found = is_pair_pending(&pending_consolidations, src_idx, tgt_idx);

        // Check if any fetched beacon block contains this consolidation request
        let (beacon_slot, beacon_request_found) =
            scan_beacon_blocks(&beacon_blocks, pair, src_idx, tgt_idx);

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

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

/// Extracts unique 0x-prefixed public keys from consolidation pairs in $O(N)$ time.
fn collect_unique_pubkeys(pairs: &[ConsolidationPair]) -> Vec<String> {
    let mut set = HashSet::with_capacity(pairs.len() * 2);
    for pair in pairs {
        set.insert(pair.source_pubkey.clone());
        set.insert(pair.target_pubkey.clone());
    }
    set.into_iter().collect()
}

/// Converts an Execution Layer block timestamp to an estimated Consensus Layer slot.
pub fn timestamp_to_slot(timestamp: u64, genesis_time: u64) -> Option<u64> {
    if timestamp >= genesis_time {
        Some((timestamp - genesis_time) / SECONDS_PER_SLOT)
    } else {
        None
    }
}

/// Pre-fetches Beacon blocks for distinct timestamps to avoid duplicate queries.
async fn fetch_beacon_blocks_for_timestamps(
    client: &BeaconClient,
    timestamps: impl Iterator<Item = u64>,
    genesis_time: u64,
) -> HashMap<u64, BeaconBlockResponse> {
    let mut blocks = HashMap::new();
    let distinct_slots: HashSet<u64> = timestamps
        .filter_map(|ts| timestamp_to_slot(ts, genesis_time))
        .collect();

    for slot in distinct_slots {
        if let Ok(Some(block_resp)) = client.get_beacon_block(&slot.to_string()).await {
            blocks.insert(slot, block_resp);
        }
    }
    blocks
}

/// Checks if a validator pair is currently queued in `pending_consolidations`.
fn is_pair_pending(
    pending: &[PendingConsolidationItem],
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
) -> bool {
    match (src_idx, tgt_idx) {
        (Some(s), Some(t)) => pending.iter().any(|item| {
            item.source_index.parse::<u64>().ok() == Some(s)
                && item.target_index.parse::<u64>().ok() == Some(t)
        }),
        _ => false,
    }
}

/// Scans pre-fetched beacon blocks to determine if a consolidation request was included.
fn scan_beacon_blocks(
    blocks: &HashMap<u64, BeaconBlockResponse>,
    pair: &ConsolidationPair,
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
) -> (Option<u64>, bool) {
    for (&slot, block_resp) in blocks {
        if let Some(ref requests) = block_resp.data.message.body.execution_requests
            && let Some(ref consolidations) = requests.consolidations
        {
            for req in consolidations {
                if matches_consolidation_request(req, pair, src_idx, tgt_idx) {
                    return (Some(slot), true);
                }
            }
        }
    }

    // Return the first estimated slot if available, even if request wasn't found in body
    let first_slot = blocks.keys().copied().next();
    (first_slot, false)
}

/// Checks if an execution request matches a given consolidation pair (by pubkey or index).
fn matches_consolidation_request(
    req: &ConsolidationRequestItem,
    pair: &ConsolidationPair,
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
) -> bool {
    let match_pubkeys = req.source_pubkey.as_deref().map(|s| s.to_lowercase())
        == Some(pair.source_pubkey.to_lowercase())
        && req.target_pubkey.as_deref().map(|s| s.to_lowercase())
            == Some(pair.target_pubkey.to_lowercase());

    let match_indices = match (src_idx, tgt_idx) {
        (Some(s_idx), Some(t_idx)) => {
            req.source_index
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                == Some(s_idx)
                && req
                    .target_index
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok())
                    == Some(t_idx)
        }
        _ => false,
    };

    match_pubkeys || match_indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_to_slot() {
        let genesis = 1606824023;
        let slot = timestamp_to_slot(genesis + 24, genesis);
        assert_eq!(slot, Some(2));
    }

    #[test]
    fn test_is_pair_pending() {
        let pending = vec![PendingConsolidationItem {
            source_index: "100".to_string(),
            target_index: "200".to_string(),
        }];
        assert!(is_pair_pending(&pending, Some(100), Some(200)));
        assert!(!is_pair_pending(&pending, Some(100), Some(300)));
        assert!(!is_pair_pending(&pending, None, Some(200)));
    }
}
