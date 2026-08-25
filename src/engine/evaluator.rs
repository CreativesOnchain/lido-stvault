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

        // Step 2: Execute Consensus Layer verification
        let cl_evidence =
            verify_consensus_layer(beacon_client, manifest_pairs, &el_block_timestamps).await?;

        // Step 3: Extract operator address from the first valid EL transaction
        let operator_address = el_evidence
            .verified_txs
            .values()
            .next()
            .map(|tx| tx.from.as_str());

        // Step 4: Check Lido fee exemption role
        let fee_exemption = LidoRoleInspector::check_fee_exempt_role(
            el_client,
            st_vault_dashboard,
            operator_address,
            !el_evidence.verified_txs.is_empty(),
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

/// Builds a `PairVerificationResult` by cross-referencing execution and consensus evidence.
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
    let beacon_slot = cl_pair.and_then(|c| c.beacon_slot);
    let beacon_request_found = cl_pair.map(|c| c.beacon_request_found).unwrap_or(false);
    let cl_pending_found = cl_pair.map(|c| c.cl_pending_found).unwrap_or(false);

    let (status, details, indeterminate_reason) = evaluate_pair_status(el_tx, cl_pair);

    PairVerificationResult {
        source_pubkey: pair.source_pubkey.clone(),
        source_index,
        target_pubkey: pair.target_pubkey.clone(),
        target_index,
        el_tx_hash,
        el_block_number: el_tx.map(|t| t.block_number),
        el_predeploy_found: el_tx
            .map(|t| t.predeploy_interaction_detected)
            .unwrap_or(false),
        beacon_slot,
        beacon_request_found,
        cl_pending_found,
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
    let cl_pending_found = cl_pair.map(|c| c.cl_pending_found).unwrap_or(false);
    let beacon_request_found = cl_pair.map(|c| c.beacon_request_found).unwrap_or(false);
    let source_index = cl_pair.and_then(|c| c.source_index);
    let target_index = cl_pair.and_then(|c| c.target_index);

    match (el_tx, cl_pair) {
        // Case: EL tx is missing
        (None, _) => (
            ConsolidationStatus::Indeterminate,
            "No corresponding execution layer transaction could be matched with this consolidation pair.".to_string(),
            Some("MISSING_EL_TRANSACTION".to_string()),
        ),

        // Case: EL tx reverted on-chain
        (Some(tx), _) if !tx.status_success => (
            ConsolidationStatus::NotAccepted,
            format!("Execution layer transaction '{}' reverted on-chain (status = 0).", tx.tx_hash),
            None,
        ),

        // Case: EL succeeded and CL pending consolidation found
        (Some(tx), Some(_cl)) if cl_pending_found => (
            ConsolidationStatus::Accepted,
            format!(
                "Request confirmed in EL tx '{}' (block {}) and accepted into Consensus pending_consolidations queue.",
                tx.tx_hash, tx.block_number
            ),
            None,
        ),

        // Case: EL succeeded and beacon request seen in block body (awaiting state transition)
        (Some(tx), Some(_cl)) if beacon_request_found => (
            ConsolidationStatus::Queued,
            format!(
                "Request confirmed in EL tx '{}' and included in Beacon Block body, awaiting consensus state transition.",
                tx.tx_hash
            ),
            None,
        ),

        // Case: EL succeeded and validator indices exist, but request is missing from consensus queue
        (Some(tx), Some(_cl)) if source_index.is_some() && target_index.is_some() => (
            ConsolidationStatus::NotAccepted,
            format!(
                "EL tx '{}' succeeded, but consolidation request was not found in the Consensus Layer pending queue.",
                tx.tx_hash
            ),
            None,
        ),

        // Case: Indices could not be resolved or beacon state is pruned/unreachable
        (Some(tx), _) => (
            ConsolidationStatus::Indeterminate,
            format!(
                "EL tx '{}' succeeded, but Consensus Layer validator indices or state could not be verified.",
                tx.tx_hash
            ),
            Some("UNRESOLVED_VALIDATOR_OR_PRUNED_STATE".to_string()),
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
            to: Some("0xpredeploy".to_string()),
            predeploy_interaction_detected: true,
            matched_manifest_pairs: vec![],
            receipt: TxReceipt {
                transaction_hash: "0x111".to_string(),
                block_number: 100,
                block_hash: "0xabc".to_string(),
                status: success,
                gas_used: 21000,
                from: "0xoperator".to_string(),
                to: Some("0xpredeploy".to_string()),
                logs: vec![],
                raw: serde_json::Value::Null,
            },
            details: None,
        }
    }

    fn mock_cl_evidence(
        pending: bool,
        beacon_req: bool,
        has_indices: bool,
    ) -> ClVerifiedPairEvidence {
        ClVerifiedPairEvidence {
            source_pubkey: "0x01".to_string(),
            source_index: if has_indices { Some(10) } else { None },
            target_pubkey: "0x02".to_string(),
            target_index: if has_indices { Some(20) } else { None },
            beacon_slot: Some(500),
            beacon_request_found: beacon_req,
            cl_pending_found: pending,
        }
    }

    #[test]
    fn test_status_missing_el_tx() {
        let (status, _, reason) = evaluate_pair_status(None, None);
        assert_eq!(status, ConsolidationStatus::Indeterminate);
        assert_eq!(reason.as_deref(), Some("MISSING_EL_TRANSACTION"));
    }

    #[test]
    fn test_status_el_reverted() {
        let el_tx = mock_el_tx(false);
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), None);
        assert_eq!(status, ConsolidationStatus::NotAccepted);
    }

    #[test]
    fn test_status_accepted_when_pending_found() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(true, false, true);
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Accepted);
    }

    #[test]
    fn test_status_queued_when_beacon_request_found() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(false, true, true);
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Queued);
    }

    #[test]
    fn test_status_not_accepted_when_indices_exist_but_not_in_queue() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(false, false, true);
        let (status, _, _) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::NotAccepted);
    }

    #[test]
    fn test_status_indeterminate_when_indices_missing() {
        let el_tx = mock_el_tx(true);
        let cl = mock_cl_evidence(false, false, false);
        let (status, _, reason) = evaluate_pair_status(Some(&el_tx), Some(&cl));
        assert_eq!(status, ConsolidationStatus::Indeterminate);
        assert_eq!(
            reason.as_deref(),
            Some("UNRESOLVED_VALIDATOR_OR_PRUNED_STATE")
        );
    }
}
