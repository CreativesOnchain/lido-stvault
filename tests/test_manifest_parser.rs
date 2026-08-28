use std::io::Write;
use stvault_receipt::manifest::{parse_manifest_file, parse_manifest_str};
use tempfile::NamedTempFile;

const PUBKEY_A: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
const PUBKEY_B: &str = "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488";
const PUBKEY_C: &str = "0xa4a233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee2";
const PUBKEY_D: &str = "0xb5b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c489";

#[test]
fn test_parse_official_lido_target_map_file() {
    let mut file = NamedTempFile::new().unwrap();
    let content = format!(
        r#"{{
            "{}": [
                "{}",
                "{}"
            ]
        }}"#,
        PUBKEY_B, PUBKEY_A, PUBKEY_C
    );
    file.write_all(content.as_bytes()).unwrap();

    let pairs = parse_manifest_file(file.path()).expect("should parse official format");
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].source_pubkey, PUBKEY_A.to_lowercase());
    assert_eq!(pairs[0].target_pubkey, PUBKEY_B.to_lowercase());
    assert_eq!(pairs[1].source_pubkey, PUBKEY_C.to_lowercase());
    assert_eq!(pairs[1].target_pubkey, PUBKEY_B.to_lowercase());
}

#[test]
fn test_parse_multi_pair_json_manifest_file() {
    let mut file = NamedTempFile::new().unwrap();
    let content = format!(
        r#"[
            {{"source": "{}", "target": "{}"}},
            {{"source_pubkey": "{}", "target_pubkey": "{}"}}
        ]"#,
        PUBKEY_A, PUBKEY_B, PUBKEY_C, PUBKEY_D
    );
    file.write_all(content.as_bytes()).unwrap();

    let pairs = parse_manifest_file(file.path()).expect("should parse");
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].source_pubkey, PUBKEY_A.to_lowercase());
    assert_eq!(pairs[0].target_pubkey, PUBKEY_B.to_lowercase());
    assert_eq!(pairs[1].source_pubkey, PUBKEY_C.to_lowercase());
    assert_eq!(pairs[1].target_pubkey, PUBKEY_D.to_lowercase());
}

#[test]
fn test_parse_empty_manifest_fails() {
    assert!(parse_manifest_str("").is_err());
    assert!(parse_manifest_str("[]").is_err());
    assert!(parse_manifest_str("pairs: []").is_err());
}

#[test]
fn test_parse_malformed_hex_fails() {
    let bad_json = r#"[ {"source": "0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ", "target": "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488"} ]"#;
    assert!(parse_manifest_str(bad_json).is_err());
}
