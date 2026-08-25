use crate::error::{AppError, Result};
use crate::models::ConsolidationPair;
use serde::Deserialize;
use std::path::Path;

/// Expected length of a BLS public key in hex characters (48 bytes = 96 hex characters).
pub const BLS_PUBKEY_HEX_LEN: usize = 96;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ManifestFormat {
    /// Format 1: Direct list of pair items (`[{ "source": "...", "target": "..." }]`)
    DirectList(Vec<PairItem>),
    /// Format 2: Object containing a "pairs" list (`{ "pairs": [...] }`)
    PairsWrapper { pairs: Vec<PairItem> },
    /// Format 3: Object containing a "consolidations" list (`{ "consolidations": [...] }`)
    ConsolidationsWrapper { consolidations: Vec<PairItem> },
}

impl ManifestFormat {
    /// Unwraps the parsed manifest format into a uniform list of `PairItem`s.
    fn into_pairs(self) -> Vec<PairItem> {
        match self {
            Self::DirectList(list) => list,
            Self::PairsWrapper { pairs } => pairs,
            Self::ConsolidationsWrapper { consolidations } => consolidations,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PairItem {
    #[serde(
        alias = "source",
        alias = "source_pubkey",
        alias = "sourcePubkey",
        alias = "source_validator_pubkey"
    )]
    source: String,
    #[serde(
        alias = "target",
        alias = "target_pubkey",
        alias = "targetPubkey",
        alias = "target_validator_pubkey"
    )]
    target: String,
}

/// Parses and validates a Lido stVault consolidation manifest file (JSON or YAML).
pub fn parse_manifest_file<P: AsRef<Path>>(path: P) -> Result<Vec<ConsolidationPair>> {
    let p = path.as_ref();
    let content = std::fs::read_to_string(p).map_err(|e| {
        AppError::Manifest(format!(
            "Failed to read manifest file '{}': {}",
            p.display(),
            e
        ))
    })?;
    parse_manifest_str(&content)
}

/// Parses and validates a Lido stVault consolidation manifest string (JSON or YAML).
pub fn parse_manifest_str(content: &str) -> Result<Vec<ConsolidationPair>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(AppError::Manifest("Manifest content is empty".to_string()));
    }

    // Try parsing as JSON first, fallback to YAML
    let raw_pairs: Vec<PairItem> = serde_json::from_str::<ManifestFormat>(trimmed)
        .map(ManifestFormat::into_pairs)
        .or_else(|_| {
            serde_yaml::from_str::<ManifestFormat>(trimmed).map(ManifestFormat::into_pairs)
        })
        .map_err(|_| {
            AppError::Manifest(
                "Unsupported manifest structure. Expected JSON/YAML list of pairs or object with 'pairs'/'consolidations' key."
                    .to_string(),
            )
        })?;

    if raw_pairs.is_empty() {
        return Err(AppError::Manifest(
            "Manifest contains zero consolidation pairs".to_string(),
        ));
    }

    let mut result = Vec::with_capacity(raw_pairs.len());
    for (i, item) in raw_pairs.into_iter().enumerate() {
        validate_pubkey(&item.source, &format!("item[{}] source", i))?;
        validate_pubkey(&item.target, &format!("item[{}] target", i))?;
        result.push(ConsolidationPair::new(item.source, item.target));
    }

    Ok(result)
}

/// Validates that a BLS public key is a valid 48-byte hex string (zero heap allocations).
fn validate_pubkey(pubkey: &str, field_desc: &str) -> Result<()> {
    let cleaned = pubkey
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");

    if cleaned.len() != BLS_PUBKEY_HEX_LEN {
        return Err(AppError::Manifest(format!(
            "Invalid BLS public key length for {} (expected 48 bytes / 96 hex characters, got {} characters): '{}'",
            field_desc,
            cleaned.len(),
            pubkey
        )));
    }

    // Zero-allocation stack buffer hex validation
    let mut buf = [0u8; 48];
    hex::decode_to_slice(cleaned, &mut buf).map_err(|e| {
        AppError::Manifest(format!(
            "Invalid hex characters in BLS public key for {}: {}",
            field_desc, e
        ))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SOURCE: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
    const SAMPLE_TARGET: &str = "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488";

    #[test]
    fn test_parse_json_direct_list() {
        let json = format!(
            r#"[ {{"source": "{}", "target": "{}"}} ]"#,
            SAMPLE_SOURCE, SAMPLE_TARGET
        );
        let pairs = parse_manifest_str(&json).expect("should parse");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].source_pubkey, SAMPLE_SOURCE.to_lowercase());
        assert_eq!(pairs[0].target_pubkey, SAMPLE_TARGET.to_lowercase());
    }

    #[test]
    fn test_parse_json_pairs_wrapper() {
        let json = format!(
            r#"{{ "pairs": [ {{"source_pubkey": "{}", "target_pubkey": "{}"}} ] }}"#,
            SAMPLE_SOURCE, SAMPLE_TARGET
        );
        let pairs = parse_manifest_str(&json).expect("should parse");
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn test_parse_yaml_format() {
        let yaml = format!(
            "consolidations:\n  - source: \"{}\"\n    target: \"{}\"",
            SAMPLE_SOURCE, SAMPLE_TARGET
        );
        let pairs = parse_manifest_str(&yaml).expect("should parse");
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn test_invalid_pubkey_length() {
        let json = r#"[ {"source": "0x1234", "target": "0x5678"} ]"#;
        assert!(parse_manifest_str(json).is_err());
    }

    #[test]
    fn test_invalid_hex_characters() {
        let invalid_pubkey = "0xzz9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
        let json = format!(
            r#"[ {{"source": "{}", "target": "{}"}} ]"#,
            invalid_pubkey, SAMPLE_TARGET
        );
        let res = parse_manifest_str(&json);
        assert!(res.is_err());
    }
}
