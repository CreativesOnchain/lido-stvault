use serde::{Deserialize, Serialize};

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
    #[serde(alias = "source_pubkey", alias = "sourcePubkey", alias = "source_address")]
    pub source_pubkey: Option<String>,
    #[serde(alias = "target_pubkey", alias = "targetPubkey")]
    pub target_pubkey: Option<String>,
    #[serde(alias = "source_index", alias = "sourceIndex")]
    pub source_index: Option<String>,
    #[serde(alias = "target_index", alias = "targetIndex")]
    pub target_index: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConsolidationItem {
    pub source_index: String,
    pub target_index: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConsolidationsResponse {
    pub data: Vec<PendingConsolidationItem>,
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
