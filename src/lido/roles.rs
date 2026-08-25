use crate::el::client::ElClient;
use crate::error::Result;
use crate::models::LidoFeeExemptionReport;
use alloy_primitives::keccak256;

/// OpenZeppelin AccessControl hasRole(bytes32,address) selector = 0x91d14854
pub const HAS_ROLE_SELECTOR: &str = "91d14854";

/// Computes keccak256 role hash for "NODE_OPERATOR_FEE_EXEMPT_ROLE"
pub fn node_operator_fee_exempt_role_hash() -> String {
    let hash = keccak256(b"NODE_OPERATOR_FEE_EXEMPT_ROLE");
    format!("0x{}", hex::encode(hash))
}

pub struct LidoRoleInspector;

impl LidoRoleInspector {
    /// Checks whether the specified operator account currently holds NODE_OPERATOR_FEE_EXEMPT_ROLE
    /// on the stVault Dashboard or ACL contract.
    pub async fn check_fee_exempt_role(
        el_client: &ElClient,
        contract_address: Option<&str>,
        operator_address: Option<&str>,
        fee_exemption_observed: bool,
    ) -> Result<LidoFeeExemptionReport> {
        let role_hash = node_operator_fee_exempt_role_hash();

        let contract = match contract_address {
            Some(addr) if !addr.trim().is_empty() => addr.trim(),
            _ => {
                return Ok(LidoFeeExemptionReport {
                    st_vault_dashboard: None,
                    operator_address: operator_address.map(|s| s.to_string()),
                    role_hash,
                    role_active: None,
                    fee_exemption_observed,
                    notes: "stVault Dashboard / ACL address was not provided; fee exemption role inspection skipped.".to_string(),
                });
            }
        };

        let operator = match operator_address {
            Some(addr) if !addr.trim().is_empty() => addr.trim(),
            _ => {
                return Ok(LidoFeeExemptionReport {
                    st_vault_dashboard: Some(contract.to_string()),
                    operator_address: None,
                    role_hash,
                    role_active: None,
                    fee_exemption_observed,
                    notes: "Operator address was not provided; cannot query hasRole on stVault Dashboard.".to_string(),
                });
            }
        };

        // Prepare ABI encoded calldata for hasRole(bytes32,address):
        // selector: 4 bytes
        // role: 32 bytes
        // account: 32 bytes (left-padded address)
        let role_clean = role_hash.trim_start_matches("0x");
        let op_clean = operator.trim_start_matches("0x").trim_start_matches("0X");
        let op_padded = format!("{:0>64}", op_clean);

        let calldata = format!("0x{}{}{}", HAS_ROLE_SELECTOR, role_clean, op_padded);

        match el_client.eth_call(contract, &calldata).await {
            Ok(ret) => {
                let ret_clean = ret.trim_start_matches("0x");
                let is_active = ret_clean.ends_with('1');

                let notes = if is_active {
                    "WARNING: Temporary NODE_OPERATOR_FEE_EXEMPT_ROLE is currently ACTIVE. Ensure it is revoked if consolidation workflow is finished.".to_string()
                } else {
                    "NODE_OPERATOR_FEE_EXEMPT_ROLE is NOT active (revoked or never granted)."
                        .to_string()
                };

                Ok(LidoFeeExemptionReport {
                    st_vault_dashboard: Some(contract.to_string()),
                    operator_address: Some(operator.to_string()),
                    role_hash,
                    role_active: Some(is_active),
                    fee_exemption_observed,
                    notes,
                })
            }
            Err(e) => Ok(LidoFeeExemptionReport {
                st_vault_dashboard: Some(contract.to_string()),
                operator_address: Some(operator.to_string()),
                role_hash,
                role_active: None,
                fee_exemption_observed,
                notes: format!("Failed to query hasRole on contract '{}': {}", contract, e),
            }),
        }
    }
}
