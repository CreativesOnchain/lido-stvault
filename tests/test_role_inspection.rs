use serde_json::json;
use std::collections::HashMap;
use stvault_receipt::ElClient;
use stvault_receipt::lido::{LidoRoleInspector, lido_fee_exempt_role_hash};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_lido_fee_exempt_role_active() {
    let el_server = MockServer::start().await;

    // eth_call returning 1 (true)
    let call_res = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": "0x0000000000000000000000000000000000000000000000000000000000000001"
    });

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(call_res))
        .mount(&el_server)
        .await;

    let el_client = ElClient::new(el_server.uri());
    let dashboard_addr = "0x1234567890123456789012345678901234567890";
    let pubkey = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
    let creds = "0x01000000000000000000000070997970C51812dc3A010C7d01b50e0d17dc79C8";

    let mut source_credentials = HashMap::new();
    source_credentials.insert(pubkey.to_string(), Some(creds.to_string()));

    let report = LidoRoleInspector::check_fee_exempt_roles(
        &el_client,
        Some(dashboard_addr),
        &source_credentials,
        &[],
    )
    .await
    .expect("should check role");

    assert!(report.is_any_role_active());
    assert_eq!(report.role_hash, lido_fee_exempt_role_hash());
    assert!(report.notes.contains("WARNING"));
}

#[tokio::test]
async fn test_lido_fee_exempt_role_inactive() {
    let el_server = MockServer::start().await;

    // eth_call returning 0 (false)
    let call_res = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": "0x0000000000000000000000000000000000000000000000000000000000000000"
    });

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(call_res))
        .mount(&el_server)
        .await;

    let el_client = ElClient::new(el_server.uri());
    let dashboard_addr = "0x1234567890123456789012345678901234567890";
    let pubkey = "0x8a9233f81e69b07ef94dd6d9dfd7ab6c7e112d7c07dd5aa9e8a83d3e8e2e92c48858e37ab7b3117562ad846ef3294ee1";
    let creds = "0x01000000000000000000000070997970C51812dc3A010C7d01b50e0d17dc79C8";

    let mut source_credentials = HashMap::new();
    source_credentials.insert(pubkey.to_string(), Some(creds.to_string()));

    let report = LidoRoleInspector::check_fee_exempt_roles(
        &el_client,
        Some(dashboard_addr),
        &source_credentials,
        &[],
    )
    .await
    .expect("should check role");

    assert!(!report.is_any_role_active());
    assert!(report.notes.contains("NOT active"));
}
