use crate::el::client::ElClient;
use crate::error::Result;
use crate::models::LidoFeeExemptionReport;
use alloy_primitives::keccak256;

/// OpenZeppelin AccessControl `hasRole(bytes32,address)` 4-byte selector (`0x91d14854`).
pub const HAS_ROLE_SELECTOR: &str = "91d14854";

/// Lido role name for operator fee exemption.
pub const NODE_OPERATOR_FEE_EXEMPT_ROLE_NAME: &str = "NODE_OPERATOR_FEE_EXEMPT_ROLE";

/// Computes the keccak256 role hash for `"NODE_OPERATOR_FEE_EXEMPT_ROLE"`.
pub fn node_operator_fee_exempt_role_hash() -> String {
    let hash = keccak256(NODE_OPERATOR_FEE_EXEMPT_ROLE_NAME.as_bytes());
    format!("0x{}", hex::encode(hash))
}

pub struct LidoRoleInspector;

impl LidoRoleInspector {
    /// Checks whether the specified operator account currently holds `NODE_OPERATOR_FEE_EXEMPT_ROLE`
    /// on the stVault Dashboard or Lido ACL contract.
    pub async fn check_fee_exempt_role(
        el_client: &ElClient,
        contract_address: Option<&str>,
        operator_address: Option<&str>,
        fee_exemption_observed: bool,
    ) -> Result<LidoFeeExemptionReport> {
        let role_hash = node_operator_fee_exempt_role_hash();

        let Some(contract) = contract_address.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(LidoFeeExemptionReport {
                st_vault_dashboard: None,
                operator_address: operator_address.map(ToString::to_string),
                role_hash,
                role_active: None,
                fee_exemption_observed,
                notes: "stVault Dashboard / ACL address was not provided; fee exemption role inspection skipped.".to_string(),
            });
        };

        let Some(operator) = operator_address.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(LidoFeeExemptionReport {
                st_vault_dashboard: Some(contract.to_string()),
                operator_address: None,
                role_hash,
                role_active: None,
                fee_exemption_observed,
                notes:
                    "Operator address was not provided; cannot query hasRole on stVault Dashboard."
                        .to_string(),
            });
        };

        let calldata = encode_has_role_calldata(&role_hash, operator);

        match el_client.eth_call(contract, &calldata).await {
            Ok(ret) => {
                let is_active = parse_bool_return(&ret);
                Ok(LidoFeeExemptionReport {
                    st_vault_dashboard: Some(contract.to_string()),
                    operator_address: Some(operator.to_string()),
                    role_hash,
                    role_active: Some(is_active),
                    fee_exemption_observed,
                    notes: role_status_message(is_active).to_string(),
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

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

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

/// Returns a descriptive note based on the active role state.
fn role_status_message(is_active: bool) -> &'static str {
    if is_active {
        "WARNING: Temporary NODE_OPERATOR_FEE_EXEMPT_ROLE is currently ACTIVE. Ensure it is revoked if consolidation workflow is finished."
    } else {
        "NODE_OPERATOR_FEE_EXEMPT_ROLE is NOT active (revoked or never granted)."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
