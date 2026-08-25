use crate::models::ConsolidationPair;

/// EIP-7251 Consolidation Request Predeploy contract address on Ethereum.
pub const CONSOLIDATION_PREDEPLOY_ADDRESS: &str = "0x0000bbddc7ce488642fb579f8b00f3a590007251";

pub struct ConsolidationPredeploy;

impl ConsolidationPredeploy {
    /// Checks if a contract address matches the EIP-7251 consolidation predeploy.
    pub fn is_predeploy_address(address: &str) -> bool {
        let normalized = address.to_lowercase();
        normalized == CONSOLIDATION_PREDEPLOY_ADDRESS
            || normalized.ends_with("0000bbddc7ce488642fb579f8b00f3a590007251")
    }

    /// Decodes a 96-byte direct calldata payload sent to the EIP-7251 consolidation predeploy.
    /// Format: `source_pubkey` (48 bytes) ++ `target_pubkey` (48 bytes).
    pub fn decode_predeploy_calldata(data: &[u8]) -> Option<ConsolidationPair> {
        if data.len() == 96 {
            let source_bytes = &data[0..48];
            let target_bytes = &data[48..96];
            Some(ConsolidationPair::new(
                format!("0x{}", hex::encode(source_bytes)),
                format!("0x{}", hex::encode(target_bytes)),
            ))
        } else {
            None
        }
    }

    /// Scans arbitrary calldata (e.g. batch consolidation transactions or multicalls)
    /// for known manifest source->target pubkey byte sequences.
    pub fn match_pairs_in_calldata(
        calldata: &[u8],
        manifest_pairs: &[ConsolidationPair],
    ) -> Vec<ConsolidationPair> {
        let mut matched = Vec::new();
        let calldata_hex = hex::encode(calldata).to_lowercase();

        for pair in manifest_pairs {
            let src_clean = pair.source_pubkey.trim_start_matches("0x").to_lowercase();
            let tgt_clean = pair.target_pubkey.trim_start_matches("0x").to_lowercase();

            // Check if both pubkeys appear in sequence or independently in the calldata
            let combined_sequence = format!("{}{}", src_clean, tgt_clean);
            if calldata_hex.contains(&combined_sequence)
                || (calldata_hex.contains(&src_clean) && calldata_hex.contains(&tgt_clean))
            {
                matched.push(pair.clone());
            }
        }
        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_predeploy_calldata() {
        let mut data = vec![0u8; 96];
        data[0] = 0x8a;
        data[48] = 0x96;
        let pair = ConsolidationPredeploy::decode_predeploy_calldata(&data).expect("should decode");
        assert!(pair.source_pubkey.starts_with("0x8a"));
        assert!(pair.target_pubkey.starts_with("0x96"));
    }

    #[test]
    fn test_is_predeploy_address() {
        assert!(ConsolidationPredeploy::is_predeploy_address(
            "0x0000BBdDc7CE488642fb579F8B00f3a590007251"
        ));
        assert!(ConsolidationPredeploy::is_predeploy_address(
            "0x0000bbddc7ce488642fb579f8b00f3a590007251"
        ));
        assert!(!ConsolidationPredeploy::is_predeploy_address(
            "0x1111111111111111111111111111111111111111"
        ));
    }
}
