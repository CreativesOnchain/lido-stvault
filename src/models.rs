use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a source -> target validator consolidation pair from the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsolidationPair {
    /// 48-byte BLS public key of source validator (0x-prefixed hex string)
    pub source_pubkey: String,
    /// 48-byte BLS public key of target validator (0x-prefixed hex string)
    pub target_pubkey: String,
}

impl ConsolidationPair {
    pub fn new(source: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source_pubkey: normalize_pubkey(&source.into()),
            target_pubkey: normalize_pubkey(&target.into()),
        }
    }
}

/// Normalizes a public key to lowercase 0x-prefixed hex string.
pub fn normalize_pubkey(pubkey: &str) -> String {
    let trimmed = pubkey.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        trimmed.to_lowercase()
    } else {
        format!("0x{}", trimmed).to_lowercase()
    }
}

/// Verification status for a consolidation pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsolidationStatus {
    /// Request verified in EL, included in Beacon block, and accepted into CL pending_consolidations queue.
    Accepted,
    /// Request submitted on EL and/or included in Beacon block, but pending CL state update.
    Queued,
    /// Request was dropped or rejected by consensus rules (e.g. invalid credentials, queue full, inactive).
    NotAccepted,
    /// Status cannot be verified with certainty due to missing blocks, RPC error, reorg, or unsupported API.
    Indeterminate,
}

impl std::fmt::Display for ConsolidationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted => write!(f, "ACCEPTED"),
            Self::Queued => write!(f, "QUEUED"),
            Self::NotAccepted => write!(f, "NOT_ACCEPTED"),
            Self::Indeterminate => write!(f, "INDETERMINATE"),
        }
    }
}

/// Detailed verification result for a single consolidation pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairVerificationResult {
    pub source_pubkey: String,
    pub source_index: Option<u64>,
    pub target_pubkey: String,
    pub target_index: Option<u64>,
    pub el_tx_hash: Option<String>,
    pub el_block_number: Option<u64>,
    pub el_predeploy_found: bool,
    pub beacon_slot: Option<u64>,
    pub beacon_request_found: bool,
    pub cl_pending_found: bool,
    pub status: ConsolidationStatus,
    pub details: String,
    pub indeterminate_reason: Option<String>,
}

/// Status report for Lido's NODE_OPERATOR_FEE_EXEMPT_ROLE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LidoFeeExemptionReport {
    pub st_vault_dashboard: Option<String>,
    pub operator_address: Option<String>,
    pub role_hash: String,
    pub role_active: Option<bool>,
    pub fee_exemption_observed: bool,
    pub notes: String,
}

/// Overall verification summary metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationSummary {
    pub total_pairs: usize,
    pub accepted: usize,
    pub queued: usize,
    pub not_accepted: usize,
    pub indeterminate: usize,
}

/// Complete machine-readable verification receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReceipt {
    pub tool_version: String,
    pub timestamp: DateTime<Utc>,
    pub el_rpc_url: String,
    pub cl_beacon_url: String,
    pub summary: VerificationSummary,
    pub fee_exemption: LidoFeeExemptionReport,
    pub pairs: Vec<PairVerificationResult>,
}
