use crate::cl::{
    BeaconClient, ClVerificationEvidence, ClVerifiedPairEvidence, verify_consensus_layer,
};
use crate::el::{ElClient, ElVerificationEvidence, ElVerifiedTx, verify_execution_layer};
use crate::error::Result;
use crate::lido::LidoRoleInspector;
use crate::models::{
    ConsolidationPair, ConsolidationStatus, PairVerificationResult, VerificationReceipt,
    VerificationSummary,
};
use chrono::Utc;
use std::collections::HashMap;

pub struct VerificationEngine;

impl VerificationEngine {
    /// Executes full cross-layer verification across Execution and Consensus layers.
    pub async fn run_verification(
        manifest_pairs: &[ConsolidationPair],
        tx_hashes: &[String],
        el_client: &ElClient,
        beacon_client: &BeaconClient,
        st_vault_dashboard: Option<&str>,
    ) -> Result<VerificationReceipt> {
        // Step 1: Execute Execution Layer verification
        let el_evidence = verify_execution_layer(el_client, tx_hashes, manifest_pairs).await?;
        let el_block_timestamps = extract_el_block_timestamps(&el_evidence);

        // Step 2: Execute Consensus Layer state delta verification
        let cl_evidence =
            verify_consensus_layer(beacon_client, manifest_pairs, &el_block_timestamps).await?;

        // Step 3: Extract withdrawal credentials for every source validator
        let mut source_credentials = HashMap::with_capacity(manifest_pairs.len());
        for pair in manifest_pairs {
            let src_norm = pair.source_pubkey.to_lowercase();
            let creds = cl_evidence
                .validator_withdrawal_credentials
                .get(&src_norm)
                .cloned();
            source_credentials.insert(src_norm, creds);
        }

        let receipts: Vec<_> = el_evidence
            .verified_txs
            .values()
            .map(|v| v.receipt.clone())
            .collect();

        // Step 4: Audit Lido fee exemption role for derived accounts
        let fee_exemption = LidoRoleInspector::check_fee_exempt_roles(
            el_client,
            st_vault_dashboard,
            &source_credentials,
            &receipts,
        )
        .await?;

        // Step 5: Deterministically evaluate each consolidation pair
        let mut results = Vec::with_capacity(manifest_pairs.len());
        let mut summary = VerificationSummary {
            total_pairs: manifest_pairs.len(),
            ..Default::default()
        };

        for pair in manifest_pairs {
            let pair_result = build_pair_verification_result(pair, &el_evidence, &cl_evidence);
            summary.record_status(pair_result.status);
            results.push(pair_result);
        }

        Ok(VerificationReceipt {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: Utc::now(),
            el_rpc_url: el_client.rpc_url().to_string(),
            cl_beacon_url: beacon_client.base_url().to_string(),
            summary,
            fee_exemption,
            pairs: results,
        })
    }
}

// -----------------------------------------------------------------------------
// Helper Functions & State Machine Logic
// -----------------------------------------------------------------------------

/// Extracts execution block timestamps for correlating with consensus slots.
fn extract_el_block_timestamps(el_evidence: &ElVerificationEvidence) -> HashMap<u64, u64> {
    let mut map = HashMap::with_capacity(el_evidence.verified_txs.len());
    for tx in el_evidence.verified_txs.values() {
        map.insert(tx.block_number, tx.block_timestamp);
    }
    map
}

/// Builds a `PairVerificationResult` by cross-referencing execution and consensus state delta evidence.
fn build_pair_verification_result(
    pair: &ConsolidationPair,
    el_evidence: &ElVerificationEvidence,
    cl_evidence: &ClVerificationEvidence,
) -> PairVerificationResult {
    let el_tx_hash = el_evidence.pair_to_tx_map.get(pair).cloned();
    let el_tx = el_tx_hash
        .as_ref()
        .and_then(|h| el_evidence.verified_txs.get(h));

    let cl_pair = cl_evidence.pair_evidence.get(pair);
    let source_index = cl_pair.and_then(|c| c.source_index);
    let target_index = cl_pair.and_then(|c| c.target_index);
    let withdrawal_credentials = cl_pair.and_then(|c| c.withdrawal_credentials.clone());
    let derived_source_address = cl_pair.and_then(|c| c.derived_source_address.clone());
    let beacon_slot = cl_pair.and_then(|c| c.beacon_slot);
    let beacon_request_found = cl_pair.map(|c| c.beacon_request_found).unwrap_or(false);
    let parent_state_absent = cl_pair.and_then(|c| c.parent_state_absent);
    let post_state_present = cl_pair.and_then(|c| c.post_state_present);
    let block_finalized = cl_pair.and_then(|c| c.block_finalized);

    let (status, details, indeterminate_reason) = evaluate_pair_status(el_tx, cl_pair);

    PairVerificationResult {
        source_pubkey: pair.source_pubkey.clone(),
        source_index,
        target_pubkey: pair.target_pubkey.clone(),
        target_index,
        withdrawal_credentials,
        derived_source_address,
        el_tx_hash,
        el_block_number: el_tx.map(|t| t.block_number),
        el_predeploy_found: el_tx
            .map(|t| t.predeploy_interaction_detected)
            .unwrap_or(false),
        beacon_slot,
        beacon_request_found,
        parent_state_absent,
        post_state_present,
        block_finalized,
        status,
        details,
        indeterminate_reason,
    }
}

/// Pure deterministic state evaluation rules for a single consolidation pair.
pub fn evaluate_pair_status(
    el_tx: Option<&ElVerifiedTx>,
    cl_pair: Option<&ClVerifiedPairEvidence>,
) -> (ConsolidationStatus, String, Option<String>) {
    let Some(tx) = el_tx else {
        return (
            ConsolidationStatus::Indeterminate,
            "No corresponding execution layer transaction could be matched with this consolidation pair.".to_string(),
            Some("MISSING_EL_TRANSACTION".to_string()),
        );
    };

    if !tx.status_success {
        return (
            ConsolidationStatus::NotAccepted,
            format!(
                "Execution layer transaction '{}' reverted on-chain (status = 0).",
                tx.tx_hash
            ),
            None,
        );
    }

    let Some(cl) = cl_pair else {
        return (
            ConsolidationStatus::Indeterminate,
            format!(
                "Consensus layer evidence unavailable for pair with EL tx '{}'.",
                tx.tx_hash
            ),
            Some("MISSING_CL_EVIDENCE".to_string()),
        );
    };

    if let Some(ref err) = cl.cl_error {
        return (
            ConsolidationStatus::Indeterminate,
            format!("Consensus layer verification could not complete: {}.", err),
            Some(err.clone()),
        );
    }

    if cl.source_index.is_none() || cl.target_index.is_none() {
        return (
            ConsolidationStatus::Indeterminate,
            format!(
                "Could not resolve validator indices on Consensus Layer for EL tx '{}'.",
                tx.tx_hash
            ),
            Some("UNRESOLVED_VALIDATOR_INDICES".to_string()),
        );
    }

    // Exact state delta verification logic
    match (
        cl.beacon_request_found,
        cl.parent_state_absent,
        cl.post_state_present,
        cl.block_finalized,
    ) {
        // Case 1: Exact proof of inclusion, delta transition (absent before, present after), and finalized block -> ACCEPTED
        (true, Some(true), Some(true), Some(true)) => (
            ConsolidationStatus::Accepted,
            format!(
                "Verified: Request included in finalized Beacon block (slot {}) and proven newly transitioned (absent in parent state, present in post state).",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 2: In block, newly transitioned, but not yet finalized -> QUEUED
        (true, Some(true), Some(true), Some(false)) => (
            ConsolidationStatus::Queued,
            format!(
                "Request included in Beacon block (slot {}) and queued in post state, awaiting block finalization.",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 3: In block, but state was already pending in parent state (pre-existing pair, not newly accepted by this block)
        (true, Some(false), Some(true), _) => (
            ConsolidationStatus::Queued,
            format!(
                "Consolidation pair was already present in parent state prior to Beacon slot {}; request is queued.",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 4: Request was in block execution requests, but absent in post state -> CL REJECTION (failed validation rules during block execution)
        (true, Some(true), Some(false), _) => (
            ConsolidationStatus::NotAccepted,
            format!(
                "Request was included in Beacon block (slot {}), but was rejected during block state transition (absent in post state).",
                cl.beacon_slot.unwrap_or_default()
            ),
            None,
        ),

        // Case 5: Request not found in the estimated beacon block body
        (false, _, _, _) => (
            ConsolidationStatus::Queued,
            format!(
                "EL tx '{}' succeeded, but request has not yet appeared in subsequent Beacon block bodies; awaiting block proposer inclusion.",
                tx.tx_hash
            ),
            None,
        ),

        // Default fail-closed
        _ => (
            ConsolidationStatus::Indeterminate,
            "State delta proof could not be definitively evaluated from Consensus Layer data."
                .to_string(),
            Some("INCONCLUSIVE_STATE_DELTA".to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::el::TxReceipt;

    fn mock_el_tx(success: bool) -> ElVerifiedTx {
        ElVerifiedTx {
            tx_hash: "0x111".to_string(),
            status_success: success,
            block_number: 100,
            block_hash: "0xabc".to_string(),
            block_timestamp: 1700000000,
            from: "0xoperator".to_string(),
            to: Some("0x0000bbddc7ce488642fb579f8b00f3a590007251".to_string()),
            predeploy_interaction_detected: true,
            matched_manifest_pairs: vec![],
            receipt: TxReceipt {
                transaction_hash: "0x111".to_string(),
                block_number: 100,
                block_hash: "0xabc".to_string(),
                status: success,
                gas_used: 21000,
                from: "0xoperator".to_string(),
                to: Some("0x0000bbddc7ce488642fb579f8b00f3a590007251".to_string()),
                logs: vec![],
                raw: serde_json::Value::Null,
            },
            details: None,
        }
    }

    fn mock_cl_evidence(
        beacon_req: bool,
        parent_absent: Option<bool>,
        post_present: Option<bool>,
        finalized: Option<bool>,
    ) -> ClVerifiedPairEvidence {
        ClVerifiedPairEvidence {
            source_pubkey: "0x01".to_string(),
            source_index: Some(10),
            target_pubkey: "0x02".to_string(),
            target_index: Some(20),
            withdrawal_credentials: Some(
                "0x0100000000000000000000001111111111111111111111111111111111111111".to_string(),
            ),
            derived_source_address: Some("0x1111111111111111111111111111111111111111".to_string()),
            beacon_slot: Some(500),
            beacon_request_found: beacon_req,
            parent_state_absent: parent_absent,
            post_state_present: post_present,
            block_finalized: finalized,
            cl_error: None,
        }
    }

    #[test]
    fn test_status_accepted_with_exact_state_delta_and_finality() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(true), Some(true), Some(true));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Accepted);
    }

    #[test]
    fn test_status_queued_when_not_yet_finalized() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(true), Some(true), Some(false));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Queued);
    }

    #[test]
    fn test_status_not_accepted_when_rejected_in_post_state() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(true), Some(false), Some(true));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::NotAccepted);
    }

    #[test]
    fn test_status_queued_when_pre_existing_in_parent_state() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, Some(false), Some(true), Some(true));
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Queued);
    }

    #[test]
    fn test_status_indeterminate_when_state_pruned() {
        let el_tx = mock_el_tx(true);
        let mut cl = mock_cl_evidence(true, None, None, None);
        cl.cl_error = Some("HISTORICAL_STATE_PRUNED_OR_UNAVAILABLE".to_string());
        let (status, _, reason) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Indeterminate);
        assert_eq!(
            reason.as_deref(),
            Some("HISTORICAL_STATE_PRUNED_OR_UNAVAILABLE")
        );
    }
}
