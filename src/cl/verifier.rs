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
/// Slots per epoch in Ethereum proof-of-stake.
pub const SLOTS_PER_EPOCH: u64 = 32;
/// Maximum number of subsequent Consensus Layer slots to scan for an execution request.
pub const MAX_SCAN_SLOTS: u64 = 64;

/// Verifies validator consolidation state across the Consensus Layer (Beacon API) using exact block-level state delta proofs.
pub async fn verify_consensus_layer(
    client: &BeaconClient,
    pairs: &[ConsolidationPair],
    el_block_timestamps: &HashMap<u64, u64>,
) -> Result<ClVerificationEvidence> {
    if pairs.is_empty() {
        return Ok(ClVerificationEvidence {
            validator_indices: HashMap::new(),
            validator_withdrawal_credentials: HashMap::new(),
            pair_evidence: HashMap::new(),
            parent_states_pending: HashMap::new(),
            post_states_pending: HashMap::new(),
            beacon_blocks: HashMap::new(),
            finalized_epoch: None,
        });
    }

    // Step 1: Collect unique public keys in O(N) using pre-allocated HashSet
    let all_pubkeys = collect_unique_pubkeys(pairs);

    // Step 2: Fetch validator details (indices + withdrawal credentials) in batch
    let (validator_indices, validator_credentials) = client
        .get_validators_by_pubkeys(&all_pubkeys)
        .await
        .unwrap_or_else(|_| (HashMap::new(), HashMap::new()));

    // Step 3: Fetch Genesis to calculate slot from timestamp (network-agnostic)
    let genesis_time = client
        .get_genesis()
        .await
        .ok()
        .and_then(|g| g.data.genesis_time.parse::<u64>().ok());

    // Step 4: Fetch Finality Checkpoints to verify block finality
    let finalized_epoch = client
        .get_finality_checkpoints("head")
        .await
        .ok()
        .and_then(|fc| fc.data.finalized.epoch.parse::<u64>().ok());

    // Step 5: Scan and cache beacon blocks for EL timestamps across subsequent slots
    let beacon_blocks = if let Some(genesis) = genesis_time {
        fetch_beacon_blocks_for_timestamps(
            client,
            el_block_timestamps.values().copied(),
            genesis,
            MAX_SCAN_SLOTS,
        )
        .await
    } else {
        HashMap::new()
    };

    // Step 6: Query parent and post state pending_consolidations for each processing block using correct state roots
    let mut parent_states_pending: HashMap<String, Vec<PendingConsolidationItem>> = HashMap::new();
    let mut post_states_pending: HashMap<String, Vec<PendingConsolidationItem>> = HashMap::new();

    for block in beacon_blocks.values() {
        let parent_block_root = &block.data.message.parent_root;
        let post_state_root = &block.data.message.state_root;
        let post_slot = &block.data.message.slot;

        // Resolve parent state root by fetching parent block first
        let parent_state_root = resolve_parent_state_root(client, parent_block_root).await;

        // Query parent state pending consolidations
        if !parent_states_pending.contains_key(&parent_state_root) {
            if let Ok(pending) = client.get_pending_consolidations(&parent_state_root).await {
                parent_states_pending.insert(parent_state_root.clone(), pending);
            } else if parent_state_root != *parent_block_root
                && let Ok(pending) = client.get_pending_consolidations(parent_block_root).await
            {
                parent_states_pending.insert(parent_block_root.clone(), pending);
            }
        }

        // Query post state pending consolidations (by post state root or slot)
        if !post_states_pending.contains_key(post_state_root) {
            if let Ok(pending) = client.get_pending_consolidations(post_state_root).await {
                post_states_pending.insert(post_state_root.clone(), pending);
            } else if let Ok(pending) = client.get_pending_consolidations(post_slot).await {
                post_states_pending.insert(post_slot.clone(), pending);
            }
        }
    }

    // Step 7: Evaluate exact delta evidence for each pair
    let mut pair_evidence = HashMap::with_capacity(pairs.len());

    for pair in pairs {
        let src_norm = pair.source_pubkey.to_lowercase();
        let tgt_norm = pair.target_pubkey.to_lowercase();

        let src_idx = validator_indices.get(&src_norm).copied();
        let tgt_idx = validator_indices.get(&tgt_norm).copied();
        let src_creds = validator_credentials.get(&src_norm).cloned();
        let derived_addr = src_creds
            .as_deref()
            .and_then(derive_address_from_credentials);

        // Scan blocks for exact execution request
        let (matched_block, beacon_slot, beacon_request_found) = find_matching_beacon_block(
            &beacon_blocks,
            pair,
            src_idx,
            tgt_idx,
            derived_addr.as_deref(),
        );

        let mut parent_state_absent = None;
        let mut post_state_present = None;
        let mut block_finalized = None;
        let mut cl_error = None;

        if let Some(block) = matched_block {
            let parent_block_root = &block.data.message.parent_root;
            let post_state_root = &block.data.message.state_root;
            let post_slot = &block.data.message.slot;

            // Find parent pending consolidations list
            let parent_pending = parent_states_pending.get(parent_block_root).or_else(|| {
                parent_states_pending
                    .iter()
                    .find(|(k, _)| k.starts_with("0x"))
                    .map(|(_, v)| v)
            });

            // Find post pending consolidations list
            let post_pending = post_states_pending
                .get(post_state_root)
                .or_else(|| post_states_pending.get(post_slot));

            match (parent_pending, post_pending) {
                (Some(parent_list), Some(post_list)) => {
                    let in_parent = is_pair_in_queue(parent_list, src_idx, tgt_idx);
                    let in_post = is_pair_in_queue(post_list, src_idx, tgt_idx);

                    parent_state_absent = Some(!in_parent);
                    post_state_present = Some(in_post);
                }
                _ => {
                    cl_error = Some("HISTORICAL_STATE_PRUNED_OR_UNAVAILABLE".to_string());
                }
            }

            if let (Some(slot), Some(finalized_ep)) = (beacon_slot, finalized_epoch) {
                let block_epoch = slot / SLOTS_PER_EPOCH;
                // Strict finality check: block is finalized when its epoch is strictly less than finalized checkpoint
                block_finalized = Some(block_epoch < finalized_ep);
            }
        } else if beacon_blocks.is_empty() {
            cl_error = Some("BEACON_BLOCK_NOT_FOUND".to_string());
        }

        pair_evidence.insert(
            pair.clone(),
            ClVerifiedPairEvidence {
                source_pubkey: pair.source_pubkey.clone(),
                source_index: src_idx,
                target_pubkey: pair.target_pubkey.clone(),
                target_index: tgt_idx,
                withdrawal_credentials: src_creds,
                derived_source_address: derived_addr,
                beacon_slot,
                beacon_request_found,
                parent_state_absent,
                post_state_present,
                block_finalized,
                cl_error,
            },
        );
    }

    Ok(ClVerificationEvidence {
        validator_indices,
        validator_withdrawal_credentials: validator_credentials,
        pair_evidence,
        parent_states_pending,
        post_states_pending,
        beacon_blocks,
        finalized_epoch,
    })
}

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

/// Resolves the parent block's state root from its block root, or returns the block root as fallback.
async fn resolve_parent_state_root(client: &BeaconClient, parent_block_root: &str) -> String {
    if let Ok(Some(parent_block)) = client.get_beacon_block(parent_block_root).await {
        parent_block.data.message.state_root
    } else {
        parent_block_root.to_string()
    }
}

/// Extracts unique 0x-prefixed public keys from consolidation pairs in O(N) time.
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

/// Scans subsequent Beacon slots starting from estimated slots up to `max_scan_slots`.
async fn fetch_beacon_blocks_for_timestamps(
    client: &BeaconClient,
    timestamps: impl Iterator<Item = u64>,
    genesis_time: u64,
    max_scan_slots: u64,
) -> HashMap<u64, BeaconBlockResponse> {
    let mut blocks = HashMap::new();
    let starting_slots: HashSet<u64> = timestamps
        .filter_map(|ts| timestamp_to_slot(ts, genesis_time))
        .collect();

    for start_slot in starting_slots {
        let mut consecutive_misses = 0;
        for offset in 0..max_scan_slots {
            let slot = start_slot + offset;
            match client.get_beacon_block(&slot.to_string()).await {
                Ok(Some(block_resp)) => {
                    consecutive_misses = 0;
                    blocks.insert(slot, block_resp);
                }
                Ok(None) => {
                    consecutive_misses += 1;
                    // If we see 4 consecutive misses beyond the start slot, we have likely reached head
                    if offset > 4 && consecutive_misses >= 4 {
                        break;
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }
    }
    blocks
}

/// Checks if a validator pair is present in a `pending_consolidations` list.
fn is_pair_in_queue(
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

/// Finds the Beacon block that contains the exact consolidation request for a pair.
fn find_matching_beacon_block<'a>(
    blocks: &'a HashMap<u64, BeaconBlockResponse>,
    pair: &ConsolidationPair,
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
    derived_source_address: Option<&str>,
) -> (Option<&'a BeaconBlockResponse>, Option<u64>, bool) {
    let mut sorted_slots: Vec<u64> = blocks.keys().copied().collect();
    sorted_slots.sort_unstable();

    for slot in sorted_slots {
        if let Some(block_resp) = blocks.get(&slot)
            && let Some(ref requests) = block_resp.data.message.body.execution_requests
            && let Some(ref consolidations) = requests.consolidations
        {
            for req in consolidations {
                if matches_consolidation_request(
                    req,
                    pair,
                    src_idx,
                    tgt_idx,
                    derived_source_address,
                ) {
                    return (Some(block_resp), Some(slot), true);
                }
            }
        }
    }

    let first_slot = blocks.keys().copied().min();
    let first_block = first_slot.and_then(|s| blocks.get(&s));
    (first_block, first_slot, false)
}

/// Checks if an execution request matches a given consolidation pair (by pubkey, index, and optional source_address).
fn matches_consolidation_request(
    req: &ConsolidationRequestItem,
    pair: &ConsolidationPair,
    src_idx: Option<u64>,
    tgt_idx: Option<u64>,
    derived_source_address: Option<&str>,
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

    let match_address = match (&req.source_address, derived_source_address) {
        (Some(req_addr), Some(derived_addr)) => {
            req_addr.trim().to_lowercase() == derived_addr.trim().to_lowercase()
        }
        (Some(_), None) => false,
        (None, _) => true,
    };

    (match_pubkeys || match_indices) && match_address
}

/// Derives an Ethereum execution address from 0x01 (or 0x02) withdrawal credentials.
/// 32 bytes total: byte 0 is type (0x01), bytes 1..12 are 0x00, bytes 12..32 are the 20-byte address.
pub fn derive_address_from_credentials(credentials: &str) -> Option<String> {
    let clean = credentials
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if clean.len() != 64 {
        return None;
    }

    // Check if prefix is 01 (ETH1 withdrawal address)
    if !clean.starts_with("01") && !clean.starts_with("02") {
        return None;
    }

    // Last 40 hex chars (20 bytes) is the execution address
    let address_hex = &clean[24..64];
    Some(format!("0x{}", address_hex.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_address_from_credentials() {
        let creds = "0x0100000000000000000000001234567890abcdef1234567890abcdef12345678";
        let addr = derive_address_from_credentials(creds).expect("should derive address");
        assert_eq!(addr, "0x1234567890abcdef1234567890abcdef12345678");
    }

    #[test]
    fn test_non_eth1_credentials() {
        let bls_creds = "0x0000000000000000000000001234567890abcdef1234567890abcdef12345678";
        assert!(derive_address_from_credentials(bls_creds).is_none());
    }

    #[test]
    fn test_timestamp_to_slot() {
        let genesis = 1606824023;
        let slot = timestamp_to_slot(genesis + 24, genesis);
        assert_eq!(slot, Some(2));
    }
}
