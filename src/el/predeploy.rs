use crate::models::ConsolidationPair;

/// EIP-7251 Consolidation Request Predeploy contract address on Ethereum.
pub const CONSOLIDATION_PREDEPLOY_ADDRESS: &str = "0x0000bbddc7ce488642fb579f8b00f3a590007251";

/// Length of a BLS12-381 public key in bytes.
pub const PUBKEY_BYTE_LEN: usize = 48;

/// Expected calldata length for a direct consolidation request (source + target).
pub const PREDEPLOY_CALLDATA_LEN: usize = PUBKEY_BYTE_LEN * 2; // 96 bytes

pub struct ConsolidationPredeploy;

impl ConsolidationPredeploy {
    /// Checks if an address matches the EIP-7251 consolidation predeploy address (zero-allocation).
    pub fn is_predeploy_address(address: &str) -> bool {
        let clean = address
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");

        clean.eq_ignore_ascii_case("0000bbddc7ce488642fb579f8b00f3a590007251")
    }

    /// Decodes a 96-byte direct calldata payload sent to the EIP-7251 consolidation predeploy.
    /// Format: `source_pubkey` (48 bytes) ++ `target_pubkey` (48 bytes).
    pub fn decode_predeploy_calldata(data: &[u8]) -> Option<ConsolidationPair> {
        if data.len() != PREDEPLOY_CALLDATA_LEN {
            return None;
        }

        let source_bytes = &data[0..PUBKEY_BYTE_LEN];
        let target_bytes = &data[PUBKEY_BYTE_LEN..PREDEPLOY_CALLDATA_LEN];

        Some(ConsolidationPair::new(
            format!("0x{}", hex::encode(source_bytes)),
            format!("0x{}", hex::encode(target_bytes)),
        ))
    }

    /// Scans calldata for exact 96-byte consecutive chunks `[source_pubkey (48B) || target_pubkey (48B)]`.
    /// Does NOT perform loose independent search.
    pub fn match_pairs_in_calldata(
        calldata: &[u8],
        manifest_pairs: &[ConsolidationPair],
    ) -> Vec<ConsolidationPair> {
        if calldata.len() < PREDEPLOY_CALLDATA_LEN {
            return Vec::new();
        }

        let mut matched = Vec::with_capacity(manifest_pairs.len());

        for pair in manifest_pairs {
            if exact_pair_sequence_in_bytes(calldata, pair) {
                matched.push(pair.clone());
            }
        }

        matched
    }
}

/// Checks if an exact consecutive 96-byte sequence (source ++ target) appears in the calldata bytes.
fn exact_pair_sequence_in_bytes(calldata: &[u8], pair: &ConsolidationPair) -> bool {
    let src_clean = pair
        .source_pubkey
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    let tgt_clean = pair
        .target_pubkey
        .trim_start_matches("0x")
        .trim_start_matches("0X");

    let Ok(src_bytes) = hex::decode(src_clean) else {
        return false;
    };
    let Ok(tgt_bytes) = hex::decode(tgt_clean) else {
        return false;
    };

    if src_bytes.len() != PUBKEY_BYTE_LEN || tgt_bytes.len() != PUBKEY_BYTE_LEN {
        return false;
    }

    let mut combined = [0u8; PREDEPLOY_CALLDATA_LEN];
    combined[..PUBKEY_BYTE_LEN].copy_from_slice(&src_bytes);
    combined[PUBKEY_BYTE_LEN..].copy_from_slice(&tgt_bytes);

    contains_subslice(calldata, &combined)
}

/// Fast search for a needle subslice inside haystack bytes.
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_predeploy_calldata() {
        let mut data = vec![0u8; PREDEPLOY_CALLDATA_LEN];
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
        assert!(ConsolidationPredeploy::is_predeploy_address(
            "0000bbddc7ce488642fb579f8b00f3a590007251"
        ));
        assert!(!ConsolidationPredeploy::is_predeploy_address(
            "0x1111111111111111111111111111111111111111"
        ));
    }

    #[test]
    fn test_exact_sequence_matching() {
        let pair = ConsolidationPair::new(
            "0x8a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
            "0x9b0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002",
        );

        let mut calldata = vec![0x12, 0x34, 0x56, 0x78];
        let src_bytes = hex::decode(pair.source_pubkey.trim_start_matches("0x")).unwrap();
        let tgt_bytes = hex::decode(pair.target_pubkey.trim_start_matches("0x")).unwrap();
        calldata.extend_from_slice(&src_bytes);
        calldata.extend_from_slice(&tgt_bytes);

        let matches =
            ConsolidationPredeploy::match_pairs_in_calldata(&calldata, std::slice::from_ref(&pair));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], pair);
    }

    #[test]
    fn test_rejects_loose_non_consecutive_bytes() {
        let pair = ConsolidationPair::new(
            "0x8a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
            "0x9b0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002",
        );

        // Put source, then some arbitrary filler bytes, then target (NOT consecutive 96 bytes)
        let mut calldata = vec![0x12, 0x34];
        let src_bytes = hex::decode(pair.source_pubkey.trim_start_matches("0x")).unwrap();
        let tgt_bytes = hex::decode(pair.target_pubkey.trim_start_matches("0x")).unwrap();
        calldata.extend_from_slice(&src_bytes);
        calldata.extend_from_slice(&[0xff, 0xfe, 0xfd]); // gap
        calldata.extend_from_slice(&tgt_bytes);

        let matches =
            ConsolidationPredeploy::match_pairs_in_calldata(&calldata, std::slice::from_ref(&pair));
        assert_eq!(matches.len(), 0, "Non-consecutive bytes must not match");
    }
}
