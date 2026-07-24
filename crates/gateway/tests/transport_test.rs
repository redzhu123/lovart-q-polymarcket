//! Transport 集成测试（P2-03）。
//!
//! 验证 HTTP Transport 和 WebSocket Transport 在 Mock 模式下工作。

use pm_gateway::transport::rest::{
    HttpMethod, HttpRequest, HttpResponse, HttpTransport, ReqwestTransport,
};
use pm_gateway::transport::websocket::{NoopWsTransport, WsTransport};

fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();
}

// ============================================================================
// REST Transport 测试
// ============================================================================

#[tokio::test]
async fn rest_transport_mock_mode_creation() {
    init_logging();
    let config = pm_api_test::client::config::ApiTestConfig::mock();
    let transport = ReqwestTransport::new(config);

    assert!(!transport.is_connected());
    assert!(transport.rate_limiter().is_some());
    assert!(!transport.is_live());
}

#[tokio::test]
async fn rest_transport_connect_disconnect() {
    init_logging();
    let config = pm_api_test::client::config::ApiTestConfig::mock();
    let transport = ReqwestTransport::new(config);

    transport.connect().await.unwrap();
    assert!(transport.is_connected());

    transport.disconnect().await.unwrap();
    assert!(!transport.is_connected());
}

#[tokio::test]
async fn rest_transport_get_request() {
    init_logging();
    let config = pm_api_test::client::config::ApiTestConfig::mock();
    let transport = ReqwestTransport::new(config);

    let req = HttpRequest::get("/time");
    let resp = transport.send(req).await.unwrap();

    assert!(resp.is_success());
    assert_eq!(resp.status, 200);
    assert!(!resp.request_id.is_empty());
}

#[tokio::test]
async fn rest_transport_get_markets() {
    init_logging();
    let config = pm_api_test::client::config::ApiTestConfig::mock();
    let transport = ReqwestTransport::new(config);

    let req = HttpRequest::get("/markets");
    let resp = transport.send(req).await.unwrap();

    assert!(resp.is_success());
    assert!(resp.body.is_array());
}

#[tokio::test]
async fn rest_transport_post_request() {
    init_logging();
    let config = pm_api_test::client::config::ApiTestConfig::mock();
    let transport = ReqwestTransport::new(config);

    let body = serde_json::json!({"key": "value"});
    let req = HttpRequest::post("/order", body);
    let resp = transport.send(req).await.unwrap();

    // Mock 模式返回 200 + 空对象
    assert!(resp.is_success() || resp.status == 200);
}

#[tokio::test]
async fn rest_transport_unique_request_ids() {
    init_logging();
    let req1 = HttpRequest::get("/time");
    let req2 = HttpRequest::get("/time");
    assert_ne!(req1.request_id, req2.request_id);

    let custom = HttpRequest::get("/time").with_request_id("custom-id");
    assert_eq!(custom.request_id, "custom-id");
}

#[tokio::test]
async fn http_request_method_display() {
    init_logging();
    assert_eq!(HttpMethod::Get.as_str(), "GET");
    assert_eq!(HttpMethod::Post.as_str(), "POST");
    assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
    assert_eq!(HttpMethod::Put.as_str(), "PUT");
    assert_eq!(HttpMethod::Patch.as_str(), "PATCH");

    assert_eq!(format!("{}", HttpMethod::Get), "GET");
}

#[tokio::test]
async fn http_response_summary_zh() {
    init_logging();
    let resp = HttpResponse {
        status: 200,
        body: serde_json::Value::Null,
        headers: vec![],
        latency_ms: 42,
        url: "/time".into(),
        request_id: "req-001".into(),
    };
    let s = resp.summary_zh();
    assert!(s.contains("200"));
    assert!(s.contains("42ms"));
    assert!(s.contains("req-001"));
    assert!(s.contains("✅"));
}

// ============================================================================
// WebSocket Transport 测试
// ============================================================================

#[tokio::test]
async fn ws_transport_noop_connect() {
    init_logging();
    let ws = NoopWsTransport::new("wss://ws.polymarket.com");
    assert!(!ws.is_connected());

    ws.connect().await.unwrap();
    assert!(ws.is_connected());
}

#[tokio::test]
async fn ws_transport_noop_subscribe() {
    init_logging();
    let ws = NoopWsTransport::new("wss://ws.polymarket.com");
    ws.connect().await.unwrap();

    ws.subscribe("book:0x1234").await.unwrap();
    ws.unsubscribe("book:0x1234").await.unwrap();
}

#[tokio::test]
async fn ws_transport_noop_recv_returns_error() {
    init_logging();
    let ws = NoopWsTransport::new("wss://ws.polymarket.com");

    let result = ws.recv().await;
    assert!(result.is_err());

    assert!(ws.try_recv().await.is_none());
}

#[tokio::test]
async fn ws_transport_url() {
    init_logging();
    let ws = NoopWsTransport::new("wss://example.com/ws");
    assert_eq!(ws.url(), "wss://example.com/ws");
}
