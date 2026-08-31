use crate::ConsolidationPair;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisData {
    pub genesis_time: String,
    pub genesis_validators_root: String,
    pub genesis_fork_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisResponse {
    pub data: GenesisData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorData {
    pub index: String,
    pub status: String,
    pub validator: ValidatorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorDetails {
    pub pubkey: String,
    pub withdrawal_credentials: String,
    pub effective_balance: String,
    pub slashed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorsResponse {
    pub data: Vec<ValidatorData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationRequestItem {
    #[serde(alias = "source_address", alias = "sourceAddress", default)]
    pub source_address: Option<String>,
    #[serde(alias = "source_pubkey", alias = "sourcePubkey", default)]
    pub source_pubkey: Option<String>,
    #[serde(alias = "target_pubkey", alias = "targetPubkey", default)]
    pub target_pubkey: Option<String>,
    #[serde(alias = "source_index", alias = "sourceIndex", default)]
    pub source_index: Option<String>,
    #[serde(alias = "target_index", alias = "targetIndex", default)]
    pub target_index: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingConsolidationItem {
    pub source_index: String,
    pub target_index: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConsolidationsResponse {
    pub data: Vec<PendingConsolidationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub epoch: String,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalityCheckpointsData {
    pub previous_justified: Checkpoint,
    pub current_justified: Checkpoint,
    pub finalized: Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalityCheckpointsResponse {
    pub data: FinalityCheckpointsData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconBlockBody {
    pub execution_payload: Option<serde_json::Value>,
    pub execution_requests: Option<ExecutionRequests>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequests {
    pub consolidations: Option<Vec<ConsolidationRequestItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconBlockMessage {
    pub slot: String,
    pub proposer_index: String,
    pub parent_root: String,
    pub state_root: String,
    pub body: BeaconBlockBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconBlockData {
    pub message: BeaconBlockMessage,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconBlockResponse {
    pub data: BeaconBlockData,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClVerifiedPairEvidence {
    pub source_pubkey: String,
    pub source_index: Option<u64>,
    pub target_pubkey: String,
    pub target_index: Option<u64>,
    pub withdrawal_credentials: Option<String>,
    pub derived_source_address: Option<String>,
    pub beacon_slot: Option<u64>,
    pub beacon_request_found: bool,
    pub parent_state_absent: Option<bool>,
    pub post_state_present: Option<bool>,
    pub block_finalized: Option<bool>,
    pub cl_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClVerificationEvidence {
    pub validator_indices: HashMap<String, u64>,
    pub validator_withdrawal_credentials: HashMap<String, String>,
    pub pair_evidence: HashMap<ConsolidationPair, ClVerifiedPairEvidence>,
    pub parent_states_pending: HashMap<String, Vec<PendingConsolidationItem>>,
    pub post_states_pending: HashMap<String, Vec<PendingConsolidationItem>>,
    pub beacon_blocks: HashMap<u64, BeaconBlockResponse>,
    pub finalized_epoch: Option<u64>,
}
