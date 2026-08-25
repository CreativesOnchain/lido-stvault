use crate::ConsolidationPair;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxReceipt {
    pub transaction_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub status: bool,
    pub gas_used: u64,
    pub from: String,
    pub to: Option<String>,
    pub logs: Vec<TxLog>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxDetails {
    pub hash: String,
    pub block_number: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub input: String,
    pub value: String,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDetails {
    pub number: u64,
    pub hash: String,
    pub timestamp: u64,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElVerifiedTx {
    pub tx_hash: String,
    pub status_success: bool,
    pub block_number: u64,
    pub block_hash: String,
    pub block_timestamp: u64,
    pub from: String,
    pub to: Option<String>,
    pub predeploy_interaction_detected: bool,
    pub matched_manifest_pairs: Vec<ConsolidationPair>,
    pub receipt: TxReceipt,
    pub details: Option<TxDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElVerificationEvidence {
    pub verified_txs: HashMap<String, ElVerifiedTx>,
    pub pair_to_tx_map: HashMap<ConsolidationPair, String>,
    pub raw_receipts: HashMap<String, serde_json::Value>,
    pub raw_blocks: HashMap<u64, BlockDetails>,
}
