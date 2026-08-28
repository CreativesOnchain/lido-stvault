use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents a source -> target validator consolidation pair from the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsolidationPair {
    /// 48-byte BLS public key of source validator (0x-prefixed lowercase hex string)
    pub source_pubkey: String,
    /// 48-byte BLS public key of target validator (0x-prefixed lowercase hex string)
    pub target_pubkey: String,
}

impl ConsolidationPair {
    /// Creates a new `ConsolidationPair` with normalized, lowercase `0x`-prefixed public keys.
    pub fn new(source: impl AsRef<str>, target: impl AsRef<str>) -> Self {
        Self {
            source_pubkey: normalize_pubkey(source.as_ref()),
            target_pubkey: normalize_pubkey(target.as_ref()),
        }
    }
}

/// Normalizes a public key string to a lowercase, `0x`-prefixed hex string.
pub fn normalize_pubkey(pubkey: &str) -> String {
    let trimmed = pubkey.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        trimmed.to_lowercase()
    } else {
        let mut s = String::with_capacity(trimmed.len() + 2);
        s.push_str("0x");
        s.push_str(&trimmed.to_lowercase());
        s
    }
}

/// Cross-layer verification status for a consolidation pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsolidationStatus {
    /// Exact request proven included in a finalized Beacon block and newly transitioned
    /// (absent in parent state, present in post state).
    Accepted,
    /// Request verified on EL and/or Beacon block, pending consensus epoch processing/finalization.
    Queued,
    /// Request failed consensus validation rules or was rejected during block execution.
    NotAccepted,
    /// Status cannot be verified with certainty (e.g. historical state pruned, endpoint unsupported, missing receipt).
    Indeterminate,
}

impl ConsolidationStatus {
    /// Returns `true` if the request is definitively confirmed by state delta proof.
    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// Returns `true` if the request is queued awaiting state processing.
    pub fn is_queued(self) -> bool {
        matches!(self, Self::Queued)
    }

    /// Returns `true` if the request failed or was not accepted.
    pub fn is_rejected(self) -> bool {
        matches!(self, Self::NotAccepted)
    }

    /// Returns `true` if the request state could not be resolved.
    pub fn is_indeterminate(self) -> bool {
        matches!(self, Self::Indeterminate)
    }
}

impl fmt::Display for ConsolidationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    pub withdrawal_credentials: Option<String>,
    pub derived_source_address: Option<String>,
    pub el_tx_hash: Option<String>,
    pub el_block_number: Option<u64>,
    pub el_predeploy_found: bool,
    pub beacon_slot: Option<u64>,
    pub beacon_request_found: bool,
    pub parent_state_absent: Option<bool>,
    pub post_state_present: Option<bool>,
    pub block_finalized: Option<bool>,
    pub status: ConsolidationStatus,
    pub details: String,
    pub indeterminate_reason: Option<String>,
}

/// Audit report for a specific source validator's derived withdrawal credential account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFeeAudit {
    pub source_pubkey: String,
    pub withdrawal_credentials: Option<String>,
    pub derived_address: Option<String>,
    pub role_active: Option<bool>,
}

/// Status report for Lido's `vaults.NodeOperatorFee.FeeExemptRole`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LidoFeeExemptionReport {
    pub st_vault_dashboard: Option<String>,
    pub role_name: String,
    pub role_hash: String,
    pub audited_sources: Vec<SourceFeeAudit>,
    pub fee_exemption_observed: bool,
    pub notes: String,
}

impl LidoFeeExemptionReport {
    /// Returns `true` if any audited source validator address currently has the role active.
    pub fn is_any_role_active(&self) -> bool {
        self.audited_sources
            .iter()
            .any(|s| s.role_active == Some(true))
    }
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

impl VerificationSummary {
    /// Records a pair's verification status into the metric counters.
    pub fn record_status(&mut self, status: ConsolidationStatus) {
        match status {
            ConsolidationStatus::Accepted => self.accepted += 1,
            ConsolidationStatus::Queued => self.queued += 1,
            ConsolidationStatus::NotAccepted => self.not_accepted += 1,
            ConsolidationStatus::Indeterminate => self.indeterminate += 1,
        }
    }

    /// Returns `true` if every pair in the manifest was verified as `Accepted`.
    pub fn is_all_accepted(&self) -> bool {
        self.accepted == self.total_pairs && self.total_pairs > 0
    }

    /// Returns `true` if any pair was rejected or returned indeterminate.
    pub fn has_attention_items(&self) -> bool {
        self.not_accepted > 0 || self.indeterminate > 0
    }

    /// Returns the acceptance percentage (0.0% to 100.0%).
    pub fn acceptance_percentage(&self) -> f64 {
        if self.total_pairs > 0 {
            (self.accepted as f64 / self.total_pairs as f64) * 100.0
        } else {
            0.0
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pubkey() {
        assert_eq!(normalize_pubkey("0xABCD"), "0xabcd");
        assert_eq!(normalize_pubkey("0XABCD"), "0xabcd");
        assert_eq!(normalize_pubkey("abcd"), "0xabcd");
        assert_eq!(normalize_pubkey("  0x1234  "), "0x1234");
    }

    #[test]
    fn test_verification_summary_metrics() {
        let mut summary = VerificationSummary {
            total_pairs: 3,
            ..Default::default()
        };

        summary.record_status(ConsolidationStatus::Accepted);
        summary.record_status(ConsolidationStatus::Accepted);
        summary.record_status(ConsolidationStatus::Queued);

        assert_eq!(summary.accepted, 2);
        assert_eq!(summary.queued, 1);
        assert!(!summary.is_all_accepted());
        assert_eq!(summary.acceptance_percentage(), (2.0 / 3.0) * 100.0);

        let all_ok = VerificationSummary {
            total_pairs: 2,
            accepted: 2,
            ..Default::default()
        };
        assert!(all_ok.is_all_accepted());
        assert!(!all_ok.has_attention_items());
    }
}
