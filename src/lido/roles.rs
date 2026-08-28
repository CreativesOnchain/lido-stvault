use crate::el::client::ElClient;
use crate::el::types::TxReceipt;
use crate::error::Result;
use crate::models::{LidoFeeExemptionReport, SourceFeeAudit};
use alloy_primitives::keccak256;
use std::collections::HashMap;

/// OpenZeppelin AccessControl `hasRole(bytes32,address)` 4-byte selector (`0x91d14854`).
pub const HAS_ROLE_SELECTOR: &str = "91d14854";

/// Lido exact role name for operator fee exemption.
pub const LIDO_FEE_EXEMPT_ROLE_NAME: &str = "vaults.NodeOperatorFee.FeeExemptRole";

/// Computes the keccak256 role hash for `"vaults.NodeOperatorFee.FeeExemptRole"`.
pub fn lido_fee_exempt_role_hash() -> String {
    let hash = keccak256(LIDO_FEE_EXEMPT_ROLE_NAME.as_bytes());
    format!("0x{}", hex::encode(hash))
}

pub struct LidoRoleInspector;

impl LidoRoleInspector {
    /// Audits `vaults.NodeOperatorFee.FeeExemptRole` for each unique account derived from
    /// each source validator's withdrawal credentials.
    pub async fn check_fee_exempt_roles(
        el_client: &ElClient,
        contract_address: Option<&str>,
        source_credentials: &HashMap<String, Option<String>>,
        receipts: &[TxReceipt],
    ) -> Result<LidoFeeExemptionReport> {
        let role_hash = lido_fee_exempt_role_hash();
        let role_name = LIDO_FEE_EXEMPT_ROLE_NAME.to_string();

        let fee_exemption_observed = detect_fee_exemption_events(contract_address, receipts);

        let Some(contract) = contract_address.map(str::trim).filter(|s| !s.is_empty()) else {
            let audited_sources = source_credentials
                .iter()
                .map(|(pubkey, creds)| {
                    let derived = creds
                        .as_deref()
                        .and_then(crate::cl::verifier::derive_address_from_credentials);
                    SourceFeeAudit {
                        source_pubkey: pubkey.clone(),
                        withdrawal_credentials: creds.clone(),
                        derived_address: derived,
                        role_active: None,
                    }
                })
                .collect();

            return Ok(LidoFeeExemptionReport {
                st_vault_dashboard: None,
                role_name,
                role_hash,
                audited_sources,
                fee_exemption_observed,
                notes: "stVault Dashboard / ACL address was not provided; fee exemption role inspection skipped.".to_string(),
            });
        };

        // Cache role lookups per derived address to avoid redundant RPC calls
        let mut address_role_cache: HashMap<String, Option<bool>> = HashMap::new();
        let mut audited_sources = Vec::with_capacity(source_credentials.len());

        for (pubkey, creds) in source_credentials {
            let derived = creds
                .as_deref()
                .and_then(crate::cl::verifier::derive_address_from_credentials);

            let role_active = if let Some(ref addr) = derived {
                if let Some(&cached) = address_role_cache.get(addr) {
                    cached
                } else {
                    let is_active = query_has_role(el_client, contract, &role_hash, addr).await;
                    address_role_cache.insert(addr.clone(), is_active);
                    is_active
                }
            } else {
                None
            };

            audited_sources.push(SourceFeeAudit {
                source_pubkey: pubkey.clone(),
                withdrawal_credentials: creds.clone(),
                derived_address: derived,
                role_active,
            });
        }

        let any_active = audited_sources.iter().any(|s| s.role_active == Some(true));

        let notes = if any_active {
            "WARNING: vaults.NodeOperatorFee.FeeExemptRole is currently ACTIVE on one or more source validator accounts. Ensure it is revoked after consolidation workflow completes."
        } else {
            "vaults.NodeOperatorFee.FeeExemptRole is NOT active on audited source validator accounts (revoked or never granted)."
        };

        Ok(LidoFeeExemptionReport {
            st_vault_dashboard: Some(contract.to_string()),
            role_name,
            role_hash,
            audited_sources,
            fee_exemption_observed,
            notes: notes.to_string(),
        })
    }
}

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

/// Queries `hasRole(bytes32,address)` for a specific address.
async fn query_has_role(
    el_client: &ElClient,
    contract: &str,
    role_hash: &str,
    address: &str,
) -> Option<bool> {
    let calldata = encode_has_role_calldata(role_hash, address);
    el_client
        .eth_call(contract, &calldata)
        .await
        .ok()
        .map(|ret| parse_bool_return(&ret))
}

/// Encodes ABI calldata for `hasRole(bytes32,address)`:
/// - 4 bytes: `0x91d14854`
/// - 32 bytes: role hash
/// - 32 bytes: left-padded 20-byte account address
pub fn encode_has_role_calldata(role_hash_hex: &str, operator_address: &str) -> String {
    let role_clean = role_hash_hex
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    let op_clean = operator_address
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    let op_padded = format!("{:0>64}", op_clean);

    format!("0x{}{}{}", HAS_ROLE_SELECTOR, role_clean, op_padded)
}

/// Parses standard 32-byte ABI boolean output from `eth_call`.
pub fn parse_bool_return(output_hex: &str) -> bool {
    let clean = output_hex
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    clean.trim_start_matches('0') == "1"
}

/// Checks if any receipt explicitly targeted the Dashboard or emitted fee exemption logs.
fn detect_fee_exemption_events(dashboard_addr: Option<&str>, receipts: &[TxReceipt]) -> bool {
    let Some(dashboard) = dashboard_addr else {
        return false;
    };
    let dash_clean = dashboard.trim().to_lowercase();

    for receipt in receipts {
        if receipt.to.as_deref().map(|s| s.to_lowercase()) == Some(dash_clean.clone()) {
            return true;
        }
        for log in &receipt.logs {
            if log.address.to_lowercase() == dash_clean {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_hash_computation() {
        let hash = lido_fee_exempt_role_hash();
        assert!(hash.starts_with("0x"));
        assert_eq!(hash.len(), 66);
    }

    #[test]
    fn test_encode_has_role_calldata() {
        let role = "0x8a00000000000000000000000000000000000000000000000000000000000001";
        let op = "0x1234567890abcdef1234567890abcdef12345678";
        let calldata = encode_has_role_calldata(role, op);

        assert!(calldata.starts_with("0x91d14854"));
        assert!(
            calldata.ends_with("0000000000000000000000001234567890abcdef1234567890abcdef12345678")
        );
    }

    #[test]
    fn test_parse_bool_return() {
        assert!(parse_bool_return(
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        ));
        assert!(!parse_bool_return(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        ));
        assert!(!parse_bool_return("0x0"));
    }
}
