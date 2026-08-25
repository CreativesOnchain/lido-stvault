use serde_json::json;
use stvault_receipt::models::ConsolidationStatus;
use stvault_receipt::{
    generate_csv_receipt, generate_json_receipt, generate_markdown_receipt, parse_manifest_str,
    BeaconClient, ElClient, EvidenceWriter, VerificationEngine,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SAMPLE_SRC: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
const SAMPLE_TGT: &str = "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488";

#[tokio::test]
async fn test_end_to_end_accepted_workflow() {
    let el_server = MockServer::start().await;
    let cl_server = MockServer::start().await;

    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";

    // 1. Mock EL RPC: eth_getTransactionReceipt
    let receipt_json = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "transactionHash": tx_hash,
            "blockNumber": "0x12a05f", // 1220703
            "blockHash": "0xabc123",
            "status": "0x1",
            "gasUsed": "0x5208",
            "from": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
            "to": "0x0000bbddc7ce488642fb579f8b00f3a590007251",
            "logs": []
        }
    });

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(receipt_json))
        .mount(&el_server)
        .await;

    // 2. Mock CL Beacon API:
    // Genesis
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/genesis"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "genesis_time": "1606824023",
                "genesis_validators_root": "0x00",
                "genesis_fork_version": "0x00"
            }
        })))
        .mount(&cl_server)
        .await;

    // Validators lookup
    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "index": "1001",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_SRC,
                        "withdrawal_credentials": "0x01",
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                },
                {
                    "index": "1002",
                    "status": "active_ongoing",
                    "validator": {
                        "pubkey": SAMPLE_TGT,
                        "withdrawal_credentials": "0x01",
                        "effective_balance": "32000000000",
                        "slashed": false
                    }
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Pending consolidations
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {
                    "source_index": "1001",
                    "target_index": "1002"
                }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Parse manifest
    let manifest_json = format!(
        r#"[ {{"source": "{}", "target": "{}"}} ]"#,
        SAMPLE_SRC, SAMPLE_TGT
    );
    let pairs = parse_manifest_str(&manifest_json).expect("should parse manifest");

    let el_client = ElClient::new(el_server.uri());
    let beacon_client = BeaconClient::new(cl_server.uri());

    let receipt = VerificationEngine::run_verification(
        &pairs,
        &[tx_hash.to_string()],
        &el_client,
        &beacon_client,
        Some("0x1234567890123456789012345678901234567890"),
    )
    .await
    .expect("verification should succeed");

    assert_eq!(receipt.summary.total_pairs, 1);
    assert_eq!(receipt.summary.accepted, 1);
    assert_eq!(receipt.summary.not_accepted, 0);
    assert_eq!(receipt.summary.indeterminate, 0);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Accepted);
    assert_eq!(receipt.pairs[0].source_index, Some(1001));
    assert_eq!(receipt.pairs[0].target_index, Some(1002));
    assert!(receipt.pairs[0].cl_pending_found);

    // Test receipts generation
    let md = generate_markdown_receipt(&receipt);
    assert!(md.contains("ALL CONSOLIDATION REQUESTS ACCEPTED"));

    let json_receipt = generate_json_receipt(&receipt).expect("json serialize");
    assert!(json_receipt.contains("ACCEPTED"));

    let csv_receipt = generate_csv_receipt(&receipt).expect("csv serialize");
    assert!(csv_receipt.contains("ACCEPTED"));

    // Test saving evidence to tempdir
    let temp_dir = tempfile::tempdir().expect("tempdir");
    EvidenceWriter::save_all(temp_dir.path(), &receipt, &md, &json_receipt, &csv_receipt)
        .expect("should save files");

    assert!(temp_dir.path().join("receipt_summary.md").exists());
    assert!(temp_dir.path().join("receipt.json").exists());
    assert!(temp_dir.path().join("consolidations.csv").exists());
    assert!(temp_dir
        .path()
        .join("evidence/verification_metadata.json")
        .exists());
}

#[tokio::test]
async fn test_rejected_consolidation_status() {
    let el_server = MockServer::start().await;
    let cl_server = MockServer::start().await;

    let tx_hash = "0x5b3a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc27e";

    // Mock EL RPC success
    let receipt_json = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "transactionHash": tx_hash,
            "blockNumber": "0x100",
            "blockHash": "0xdef456",
            "status": "0x1",
            "gasUsed": "0x5208",
            "from": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
            "to": "0x0000bbddc7ce488642fb579f8b00f3a590007251",
            "logs": []
        }
    });

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(receipt_json))
        .mount(&el_server)
        .await;

    // Validators lookup resolves indices
    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "index": "2001", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_SRC, "withdrawal_credentials": "0x01", "effective_balance": "32000000000", "slashed": false } },
                { "index": "2002", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_TGT, "withdrawal_credentials": "0x01", "effective_balance": "32000000000", "slashed": false } }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Pending consolidations is EMPTY (request was dropped by CL!)
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": []
        })))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(
        r#"[ {{"source": "{}", "target": "{}"}} ]"#,
        SAMPLE_SRC, SAMPLE_TGT
    );
    let pairs = parse_manifest_str(&manifest_json).expect("should parse manifest");

    let el_client = ElClient::new(el_server.uri());
    let beacon_client = BeaconClient::new(cl_server.uri());

    let receipt = VerificationEngine::run_verification(
        &pairs,
        &[tx_hash.to_string()],
        &el_client,
        &beacon_client,
        None,
    )
    .await
    .expect("verification should run");

    assert_eq!(receipt.summary.total_pairs, 1);
    assert_eq!(receipt.summary.accepted, 0);
    assert_eq!(receipt.summary.not_accepted, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::NotAccepted);
}

#[tokio::test]
async fn test_indeterminate_when_beacon_unreachable() {
    let el_server = MockServer::start().await;

    let tx_hash = "0x6c4a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc28f";

    // Mock EL RPC success
    let receipt_json = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "transactionHash": tx_hash,
            "blockNumber": "0x100",
            "blockHash": "0xdef456",
            "status": "0x1",
            "gasUsed": "0x5208",
            "from": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
            "to": "0x0000bbddc7ce488642fb579f8b00f3a590007251",
            "logs": []
        }
    });

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(receipt_json))
        .mount(&el_server)
        .await;

    let manifest_json = format!(
        r#"[ {{"source": "{}", "target": "{}"}} ]"#,
        SAMPLE_SRC, SAMPLE_TGT
    );
    let pairs = parse_manifest_str(&manifest_json).expect("should parse manifest");

    let el_client = ElClient::new(el_server.uri());
    // Point to non-existent Beacon API port
    let beacon_client = BeaconClient::new("http://127.0.0.1:59999");

    let receipt = VerificationEngine::run_verification(
        &pairs,
        &[tx_hash.to_string()],
        &el_client,
        &beacon_client,
        None,
    )
    .await
    .expect("verification should run");

    assert_eq!(receipt.summary.total_pairs, 1);
    assert_eq!(receipt.summary.indeterminate, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Indeterminate);
    assert_eq!(
        receipt.pairs[0].indeterminate_reason.as_deref(),
        Some("UNRESOLVED_VALIDATOR_OR_PRUNED_STATE")
    );
}
