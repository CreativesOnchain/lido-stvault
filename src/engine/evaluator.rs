use crate::cl::{verify_consensus_layer, BeaconClient};
use crate::el::{verify_execution_layer, ElClient};
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
    pub async fn run_verification(
        manifest_pairs: &[ConsolidationPair],
        tx_hashes: &[String],
        el_client: &ElClient,
        beacon_client: &BeaconClient,
        st_vault_dashboard: Option<&str>,
    ) -> Result<VerificationReceipt> {
        // Step 1: Execute EL verification
        let el_evidence = verify_execution_layer(el_client, tx_hashes, manifest_pairs).await?;

        // Extract block timestamps for beacon block correlation
        let mut el_block_timestamps = HashMap::new();
        for tx in el_evidence.verified_txs.values() {
            el_block_timestamps.insert(tx.block_number, tx.block_timestamp);
        }

        // Step 2: Execute CL verification
        let cl_evidence =
            verify_consensus_layer(beacon_client, manifest_pairs, &el_block_timestamps).await?;

        // Step 3: Extract operator address from first valid EL tx (if any)
        let operator_address = el_evidence.verified_txs.values().next().map(|tx| tx.from.as_str());

        // Step 4: Check Lido fee exemption role
        let fee_exemption = LidoRoleInspector::check_fee_exempt_role(
            el_client,
            st_vault_dashboard,
            operator_address,
            !el_evidence.verified_txs.is_empty(),
        )
        .await?;

        // Step 5: Evaluate each pair deterministically
        let mut results = Vec::with_capacity(manifest_pairs.len());
        let mut summary =
            VerificationSummary { total_pairs: manifest_pairs.len(), ..Default::default() };

        for pair in manifest_pairs {
            let el_tx_hash = el_evidence.pair_to_tx_map.get(pair).cloned();
            let el_tx = el_tx_hash.as_ref().and_then(|h| el_evidence.verified_txs.get(h));

            let cl_pair = cl_evidence.pair_evidence.get(pair);

            let source_index = cl_pair.and_then(|c| c.source_index);
            let target_index = cl_pair.and_then(|c| c.target_index);
            let beacon_slot = cl_pair.and_then(|c| c.beacon_slot);
            let beacon_request_found = cl_pair.map(|c| c.beacon_request_found).unwrap_or(false);
            let cl_pending_found = cl_pair.map(|c| c.cl_pending_found).unwrap_or(false);

            let (status, details, indeterminate_reason) = match (el_tx, cl_pair) {
                // Case: EL tx is missing or failed
                (None, _) => (
                    ConsolidationStatus::Indeterminate,
                    "No corresponding execution layer transaction could be matched with this consolidation pair.".to_string(),
                    Some("MISSING_EL_TRANSACTION".to_string()),
                ),
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
                // Case: EL succeeded, beacon request seen or queued, but pending_consolidations not yet populated
                (Some(tx), Some(_cl)) if beacon_request_found => (
                    ConsolidationStatus::Queued,
                    format!(
                        "Request confirmed in EL tx '{}' and included in Beacon Block body, awaiting consensus state transition.",
                        tx.tx_hash
                    ),
                    None,
                ),
                // Case: EL succeeded and indices resolved, but not in beacon requests or pending consolidations
                (Some(tx), Some(_cl)) if source_index.is_some() && target_index.is_some() => (
                    ConsolidationStatus::NotAccepted,
                    format!(
                        "EL tx '{}' succeeded, but consolidation request was not found in the Consensus Layer pending queue.",
                        tx.tx_hash
                    ),
                    None,
                ),
                // Case: Indices could not be resolved or beacon data is missing
                (Some(tx), _) => (
                    ConsolidationStatus::Indeterminate,
                    format!(
                        "EL tx '{}' succeeded, but Consensus Layer validator indices or state could not be verified.",
                        tx.tx_hash
                    ),
                    Some("UNRESOLVED_VALIDATOR_OR_PRUNED_STATE".to_string()),
                ),
            };

            match status {
                ConsolidationStatus::Accepted => summary.accepted += 1,
                ConsolidationStatus::Queued => summary.queued += 1,
                ConsolidationStatus::NotAccepted => summary.not_accepted += 1,
                ConsolidationStatus::Indeterminate => summary.indeterminate += 1,
            }

            results.push(PairVerificationResult {
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
            });
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
