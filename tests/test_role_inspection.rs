use serde_json::json;
use stvault_receipt::lido::{node_operator_fee_exempt_role_hash, LidoRoleInspector};
use stvault_receipt::ElClient;
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
    let operator_addr = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";

    let report = LidoRoleInspector::check_fee_exempt_role(
        &el_client,
        Some(dashboard_addr),
        Some(operator_addr),
        true,
    )
    .await
    .expect("should check role");

    assert_eq!(report.role_active, Some(true));
    assert_eq!(report.role_hash, node_operator_fee_exempt_role_hash());
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
    let operator_addr = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";

    let report = LidoRoleInspector::check_fee_exempt_role(
        &el_client,
        Some(dashboard_addr),
        Some(operator_addr),
        true,
    )
    .await
    .expect("should check role");

    assert_eq!(report.role_active, Some(false));
    assert!(report.notes.contains("NOT active"));
}
