//! 错误类型测试（P2-03）。
//!
//! 验证 GatewayError 各变体的创建、显示、可重试性。

use pm_gateway::GatewayError;

fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();
}

// ============================================================================
// 错误创建测试
// ============================================================================

#[test]
fn network_error_creation() {
    init_logging();
    let err = GatewayError::network("连接被拒绝");
    assert_eq!(err.code(), "GW_NET_001");
    assert!(err.message_zh().contains("连接被拒绝"));
    assert!(err.is_retryable());
    assert_eq!(err.kind_zh(), "网络错误");
}

#[test]
fn authentication_error_creation() {
    init_logging();
    let err = GatewayError::authentication("API 密钥无效");
    assert_eq!(err.code(), "GW_AUTH_001");
    assert!(!err.is_retryable());
    assert_eq!(err.kind_zh(), "认证失败");
}

#[test]
fn rate_limit_error_creation() {
    init_logging();
    let err = GatewayError::rate_limit("触发限流", 5000);
    assert_eq!(err.code(), "GW_RATE_001");
    assert!(err.is_retryable());

    if let GatewayError::RateLimitError { retry_after_ms, .. } = &err {
        assert_eq!(*retry_after_ms, 5000);
    } else {
        panic!("Expected RateLimitError");
    }
}

#[test]
fn validation_error_creation() {
    init_logging();
    let err = GatewayError::validation("价格超出范围");
    assert_eq!(err.code(), "GW_VAL_001");
    assert!(!err.is_retryable());
}

#[test]
fn exchange_error_creation() {
    init_logging();
    let err = GatewayError::exchange("余额不足");
    assert_eq!(err.code(), "GW_EXCH_001");
    assert!(!err.is_retryable());
}

#[test]
fn timeout_error_creation() {
    init_logging();
    let err = GatewayError::timeout("请求超时", 10000);
    assert_eq!(err.code(), "GW_TO_001");
    assert!(err.is_retryable());

    if let GatewayError::TimeoutError { timeout_ms, .. } = &err {
        assert_eq!(*timeout_ms, 10000);
    } else {
        panic!("Expected TimeoutError");
    }
}

#[test]
fn serialization_error_creation() {
    init_logging();
    let err = GatewayError::serialization("字段缺失");
    assert_eq!(err.code(), "GW_SER_001");
    assert!(!err.is_retryable());
}

// ============================================================================
// 错误显示和转换测试
// ============================================================================

#[test]
fn error_display_includes_code_and_suggestion() {
    init_logging();
    let err = GatewayError::network("连接失败");
    let display = format!("{}", err);
    assert!(display.contains("GW_NET_001"));
    assert!(display.contains("连接失败"));
    assert!(display.contains("建议"));
}

#[test]
fn error_implements_std_error() {
    init_logging();
    let err = GatewayError::network("test");
    let _: &dyn std::error::Error = &err;
}

#[test]
fn all_error_kinds_have_chinese_names() {
    init_logging();
    assert_eq!(GatewayError::network("").kind_zh(), "网络错误");
    assert_eq!(GatewayError::authentication("").kind_zh(), "认证失败");
    assert_eq!(GatewayError::rate_limit("", 0).kind_zh(), "速率限制");
    assert_eq!(GatewayError::validation("").kind_zh(), "参数校验失败");
    assert_eq!(GatewayError::exchange("").kind_zh(), "交易所错误");
    assert_eq!(GatewayError::timeout("", 0).kind_zh(), "请求超时");
    assert_eq!(GatewayError::serialization("").kind_zh(), "序列化错误");
}

#[test]
fn retryable_classification() {
    init_logging();
    // 可重试
    assert!(GatewayError::network("").is_retryable());
    assert!(GatewayError::rate_limit("", 1000).is_retryable());
    assert!(GatewayError::timeout("", 1000).is_retryable());

    // 不可重试
    assert!(!GatewayError::authentication("").is_retryable());
    assert!(!GatewayError::validation("").is_retryable());
    assert!(!GatewayError::exchange("").is_retryable());
    assert!(!GatewayError::serialization("").is_retryable());
}

#[test]
fn error_to_failed_result() {
    init_logging();
    let err = GatewayError::network("网络故障");
    let result = err.to_failed_result("order-123", 100);
    assert!(!result.success);
    assert!(result.message.contains("网络错误"));
    assert!(result.message.contains("网络故障"));
    assert_eq!(result.latency_ms, 100);
}

#[test]
fn error_from_api_client_error() {
    init_logging();
    let api_err = pm_api_test::client::http::ApiClientError::RequestFailed("连接超时".into());
    let gw_err: GatewayError = api_err.into();
    assert_eq!(gw_err.code(), "GW_NET_001");
    assert!(gw_err.is_retryable());
}
