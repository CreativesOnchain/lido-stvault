use serde_json::json;
use stvault_receipt::models::ConsolidationStatus;
use stvault_receipt::{
    BeaconClient, ElClient, EvidenceWriter, VerificationEngine, generate_csv_receipt,
    generate_json_receipt, generate_markdown_receipt, parse_manifest_str,
};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SAMPLE_SRC: &str = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
const SAMPLE_TGT: &str = "0x96b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c488";
const WITHDRAWAL_CREDS: &str = "0x01000000000000000000000070997970C51812dc3A010C7d01b50e0d17dc79C8";

fn encode_96byte_calldata(src: &str, tgt: &str) -> String {
    let s = src.trim().trim_start_matches("0x");
    let t = tgt.trim().trim_start_matches("0x");
    format!("0x{}{}", s, t)
}

/// Helper to set up standard EL and CL mock servers with Genesis & Finality Checkpoints
async fn setup_servers() -> (MockServer, MockServer) {
    let el_server = MockServer::start().await;
    let cl_server = MockServer::start().await;

    // Genesis (genesis_time = 1606824023)
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

    // Finality Checkpoints (epoch 10 finalized -> slots 0..351 finalized)
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/finality_checkpoints"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "previous_justified": { "epoch": "11", "root": "0x11" },
                "current_justified": { "epoch": "12", "root": "0x12" },
                "finalized": { "epoch": "10", "root": "0x10" }
            }
        })))
        .mount(&cl_server)
        .await;

    (el_server, cl_server)
}

/// Sets up standard EL JSON-RPC responses for a given transaction.
async fn mock_el_rpc(
    el_server: &MockServer,
    tx_hash: &str,
    block_num: u64,
    timestamp: u64,
    calldata: &str,
    status_success: bool,
) {
    let status_hex = if status_success { "0x1" } else { "0x0" };

    // eth_getTransactionReceipt
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionReceipt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "transactionHash": tx_hash,
                "blockNumber": format!("0x{:x}", block_num),
                "blockHash": "0xabc123",
                "status": status_hex,
                "gasUsed": "0x5208",
                "from": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007251",
                "logs": []
            }
        })))
        .mount(el_server)
        .await;

    // eth_getTransactionByHash
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getTransactionByHash"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "hash": tx_hash,
                "blockNumber": format!("0x{:x}", block_num),
                "blockHash": "0xabc123",
                "from": "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
                "to": "0x0000bbddc7ce488642fb579f8b00f3a590007251",
                "input": calldata
            }
        })))
        .mount(el_server)
        .await;

    // eth_getBlockByNumber
    Mock::given(method("POST"))
        .and(body_string_contains("eth_getBlockByNumber"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "number": format!("0x{:x}", block_num),
                "hash": "0xabc123",
                "timestamp": format!("0x{:x}", timestamp)
            }
        })))
        .mount(el_server)
        .await;
}

// 1. Accepted Finalized Request with Full State Delta Proof
#[tokio::test]
async fn test_accepted_finalized_request() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let calldata = encode_96byte_calldata(SAMPLE_SRC, SAMPLE_TGT);

    // Slot 100 timestamp: 1606824023 + (100 * 12) = 1606825223
    mock_el_rpc(&el_server, tx_hash, 100, 1606825223, &calldata, true).await;

    // Validators lookup
    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "index": "1001", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_SRC, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "1002", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_TGT, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } }
            ]
        })))
        .mount(&cl_server)
        .await;

    // Beacon block at slot 100 containing request
    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "42",
                    "parent_root": "0xparent_root_100",
                    "state_root": "0xstate_root_100",
                    "body": {
                        "execution_requests": {
                            "consolidations": [
                                { "source_pubkey": SAMPLE_SRC, "target_pubkey": SAMPLE_TGT, "source_index": "1001", "target_index": "1002" }
                            ]
                        }
                    }
                },
                "signature": "0x1234"
            }
        })))
        .mount(&cl_server)
        .await;

    // Parent state: absent
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xparent_root_100/pending_consolidations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    // Post state: present
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/100/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "source_index": "1001", "target_index": "1002" }]
        })))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(r#"{{ "{}": ["{}"] }}"#, SAMPLE_TGT, SAMPLE_SRC);
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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
    .expect("verification should succeed");

    assert_eq!(receipt.summary.accepted, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Accepted);
    assert_eq!(receipt.pairs[0].parent_state_absent, Some(true));
    assert_eq!(receipt.pairs[0].post_state_present, Some(true));
    assert_eq!(receipt.pairs[0].block_finalized, Some(true));

    // Test receipts generation and disk writing
    let md = generate_markdown_receipt(&receipt);
    assert!(md.contains("ALL CONSOLIDATION REQUESTS ACCEPTED"));

    let json_receipt = generate_json_receipt(&receipt).expect("json serialize");
    assert!(json_receipt.contains("ACCEPTED"));

    let csv_receipt = generate_csv_receipt(&receipt).expect("csv serialize");
    assert!(csv_receipt.contains("ACCEPTED"));

    let temp_dir = tempfile::tempdir().expect("tempdir");
    EvidenceWriter::save_all(temp_dir.path(), &receipt, &md, &json_receipt, &csv_receipt)
        .expect("should save files");

    assert!(temp_dir.path().join("receipt_summary.md").exists());
    assert!(temp_dir.path().join("receipt.json").exists());
    assert!(temp_dir.path().join("consolidations.csv").exists());
    assert!(
        temp_dir
            .path()
            .join("evidence/verification_metadata.json")
            .exists()
    );
}

// 2. Delayed Dequeue: Pair is no longer in head state queue, but proven via block parent/post delta
#[tokio::test]
async fn test_delayed_dequeue_accepted() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let calldata = encode_96byte_calldata(SAMPLE_SRC, SAMPLE_TGT);

    mock_el_rpc(&el_server, tx_hash, 100, 1606825223, &calldata, true).await;

    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "index": "1001", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_SRC, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "1002", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_TGT, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "42",
                    "parent_root": "0xparent_root_100",
                    "state_root": "0xstate_root_100",
                    "body": {
                        "execution_requests": {
                            "consolidations": [
                                { "source_pubkey": SAMPLE_SRC, "target_pubkey": SAMPLE_TGT }
                            ]
                        }
                    }
                },
                "signature": "0x1234"
            }
        })))
        .mount(&cl_server)
        .await;

    // Parent absent, Post present at block 100
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xparent_root_100/pending_consolidations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/100/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "source_index": "1001", "target_index": "1002" }]
        })))
        .mount(&cl_server)
        .await;

    // Head state is empty (already dequeued / processed after epochs!)
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/head/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(r#"{{ "{}": ["{}"] }}"#, SAMPLE_TGT, SAMPLE_SRC);
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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

    // Must be ACCEPTED because the block state transition proved it was added and finalized!
    assert_eq!(receipt.summary.accepted, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Accepted);
}

// 3. Pre-existing Pending Pair: Pair was already in parent state -> QUEUED
#[tokio::test]
async fn test_pre_existing_pending_pair() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let calldata = encode_96byte_calldata(SAMPLE_SRC, SAMPLE_TGT);

    mock_el_rpc(&el_server, tx_hash, 100, 1606825223, &calldata, true).await;

    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "index": "1001", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_SRC, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "1002", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_TGT, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "42",
                    "parent_root": "0xparent_root_100",
                    "state_root": "0xstate_root_100",
                    "body": {
                        "execution_requests": {
                            "consolidations": [
                                { "source_pubkey": SAMPLE_SRC, "target_pubkey": SAMPLE_TGT }
                            ]
                        }
                    }
                },
                "signature": "0x1234"
            }
        })))
        .mount(&cl_server)
        .await;

    // Both parent and post state have the pair (was already pending before this block)
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xparent_root_100/pending_consolidations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "source_index": "1001", "target_index": "1002" }]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/100/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "source_index": "1001", "target_index": "1002" }]
        })))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(r#"{{ "{}": ["{}"] }}"#, SAMPLE_TGT, SAMPLE_SRC);
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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

    assert_eq!(receipt.summary.queued, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Queued);
}

// 4. CL Rejection: Request in block body, but absent in post state -> NOT_ACCEPTED
#[tokio::test]
async fn test_cl_rejection_in_block_execution() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let calldata = encode_96byte_calldata(SAMPLE_SRC, SAMPLE_TGT);

    mock_el_rpc(&el_server, tx_hash, 100, 1606825223, &calldata, true).await;

    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "index": "1001", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_SRC, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "1002", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_TGT, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "42",
                    "parent_root": "0xparent_root_100",
                    "state_root": "0xstate_root_100",
                    "body": {
                        "execution_requests": {
                            "consolidations": [
                                { "source_pubkey": SAMPLE_SRC, "target_pubkey": SAMPLE_TGT }
                            ]
                        }
                    }
                },
                "signature": "0x1234"
            }
        })))
        .mount(&cl_server)
        .await;

    // Absent in parent, and STILL ABSENT in post state -> CL rejected during block execution!
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xparent_root_100/pending_consolidations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/100/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(r#"{{ "{}": ["{}"] }}"#, SAMPLE_TGT, SAMPLE_SRC);
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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

    assert_eq!(receipt.summary.not_accepted, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::NotAccepted);
}

// 5. Unsupported Endpoint / Pruned Historical State -> INDETERMINATE
#[tokio::test]
async fn test_unsupported_or_pruned_endpoint() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let calldata = encode_96byte_calldata(SAMPLE_SRC, SAMPLE_TGT);

    mock_el_rpc(&el_server, tx_hash, 100, 1606825223, &calldata, true).await;

    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "index": "1001", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_SRC, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "1002", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_TGT, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "42",
                    "parent_root": "0xparent_root_100",
                    "state_root": "0xstate_root_100",
                    "body": {
                        "execution_requests": {
                            "consolidations": [
                                { "source_pubkey": SAMPLE_SRC, "target_pubkey": SAMPLE_TGT }
                            ]
                        }
                    }
                },
                "signature": "0x1234"
            }
        })))
        .mount(&cl_server)
        .await;

    // 404 on parent state (pruned historical state!)
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xparent_root_100/pending_consolidations",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(r#"{{ "{}": ["{}"] }}"#, SAMPLE_TGT, SAMPLE_SRC);
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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

    assert_eq!(receipt.summary.indeterminate, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Indeterminate);
    assert_eq!(
        receipt.pairs[0].indeterminate_reason.as_deref(),
        Some("HISTORICAL_STATE_PRUNED_OR_UNAVAILABLE")
    );
}

// 6. Partial Batch Workflow
#[tokio::test]
async fn test_partial_batch_multi_pair_workflow() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let src_2 = "0xa4a233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee2";
    let tgt_2 = "0xb5b6e41b9d1bb8bb4be6fb98f6d7ab7b1a206a445e9bb5f5c1d683777d13e3db85be12aa219e27c73ffbb7be2e92c489";

    let mut calldata = encode_96byte_calldata(SAMPLE_SRC, SAMPLE_TGT);
    calldata.push_str(encode_96byte_calldata(src_2, tgt_2).trim_start_matches("0x"));

    mock_el_rpc(&el_server, tx_hash, 100, 1606825223, &calldata, true).await;

    Mock::given(method("POST"))
        .and(path("/eth/v1/beacon/states/head/validators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                { "index": "1001", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_SRC, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "1002", "status": "active_ongoing", "validator": { "pubkey": SAMPLE_TGT, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "2001", "status": "active_ongoing", "validator": { "pubkey": src_2, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } },
                { "index": "2002", "status": "active_ongoing", "validator": { "pubkey": tgt_2, "withdrawal_credentials": WITHDRAWAL_CREDS, "effective_balance": "32000000000", "slashed": false } }
            ]
        })))
        .mount(&cl_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/eth/v2/beacon/blocks/100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "message": {
                    "slot": "100",
                    "proposer_index": "42",
                    "parent_root": "0xparent_root_100",
                    "state_root": "0xstate_root_100",
                    "body": {
                        "execution_requests": {
                            "consolidations": [
                                { "source_pubkey": SAMPLE_SRC, "target_pubkey": SAMPLE_TGT },
                                { "source_pubkey": src_2, "target_pubkey": tgt_2 }
                            ]
                        }
                    }
                },
                "signature": "0x1234"
            }
        })))
        .mount(&cl_server)
        .await;

    // Parent state empty for both
    Mock::given(method("GET"))
        .and(path(
            "/eth/v1/beacon/states/0xparent_root_100/pending_consolidations",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&cl_server)
        .await;

    // Post state contains ONLY pair 1 (pair 2 failed consensus validation and was dropped!)
    Mock::given(method("GET"))
        .and(path("/eth/v1/beacon/states/100/pending_consolidations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "source_index": "1001", "target_index": "1002" }]
        })))
        .mount(&cl_server)
        .await;

    let manifest_json = format!(
        r#"[
            {{ "target_pubkey": "{}", "source_pubkeys": ["{}"] }},
            {{ "target_pubkey": "{}", "source_pubkeys": ["{}"] }}
        ]"#,
        SAMPLE_TGT, SAMPLE_SRC, tgt_2, src_2
    );
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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

    assert_eq!(receipt.summary.total_pairs, 2);
    assert_eq!(receipt.summary.accepted, 1);
    assert_eq!(receipt.summary.not_accepted, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Accepted);
    assert_eq!(receipt.pairs[1].status, ConsolidationStatus::NotAccepted);
}

// 7. Conflicting Endpoints / Missing EL Tx -> INDETERMINATE
#[tokio::test]
async fn test_conflicting_or_missing_el_tx() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let different_calldata = encode_96byte_calldata(
        "0x111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
        "0x222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
    );

    mock_el_rpc(
        &el_server,
        tx_hash,
        100,
        1606825223,
        &different_calldata,
        true,
    )
    .await;

    let manifest_json = format!(r#"{{ "{}": ["{}"] }}"#, SAMPLE_TGT, SAMPLE_SRC);
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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

    assert_eq!(receipt.summary.indeterminate, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::Indeterminate);
    assert_eq!(
        receipt.pairs[0].indeterminate_reason.as_deref(),
        Some("MISSING_EL_TRANSACTION")
    );
}

// 8. EL Tx Reverted On-Chain -> NOT_ACCEPTED
#[tokio::test]
async fn test_el_tx_reverted_not_accepted() {
    let (el_server, cl_server) = setup_servers().await;
    let tx_hash = "0x4a2a11b0c9535359a34a86b5da49b4c0bc06716035f29d20c7fdc6e9d72dc26d";
    let calldata = encode_96byte_calldata(SAMPLE_SRC, SAMPLE_TGT);

    // Status 0x0 (reverted)
    mock_el_rpc(&el_server, tx_hash, 100, 1606825223, &calldata, false).await;

    let manifest_json = format!(r#"{{ "{}": ["{}"] }}"#, SAMPLE_TGT, SAMPLE_SRC);
    let pairs = parse_manifest_str(&manifest_json).expect("should parse");

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

    assert_eq!(receipt.summary.not_accepted, 1);
    assert_eq!(receipt.pairs[0].status, ConsolidationStatus::NotAccepted);
}
