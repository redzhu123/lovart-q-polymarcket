//! Gateway 统一错误类型（P2-03）。
//!
//! 所有 Gateway 错误通过此类型统一包装。
//! 每个错误包含：错误码、中文错误信息、中文建议处理方式。
//!
//! # 禁止
//!
//! - 禁止使用 String 作为错误类型。
//! - 禁止使用 anyhow::anyhow! 在 Gateway 公共接口中。
//! - 所有错误必须携带错误码和中文消息。

use crate::types::GatewayResult;

// ============================================================================
// GatewayError
// ============================================================================

/// Gateway 统一错误类型。
///
/// 所有 Gateway 操作返回此错误，包含错误码、中文消息和中文建议处理方式。
#[derive(Debug, Clone)]
pub enum GatewayError {
    /// 网络错误（连接失败、DNS 解析失败、TLS 握手失败等）。
    NetworkError {
        /// 错误码（如 `GW_NET_001`）。
        code: &'static str,
        /// 中文错误消息。
        message: String,
        /// 中文建议处理方式。
        suggestion: String,
    },

    /// 认证失败（API 密钥无效、签名错误、权限不足）。
    AuthenticationError {
        code: &'static str,
        message: String,
        suggestion: String,
    },

    /// 速率限制（触发交易所或本地速率限制）。
    RateLimitError {
        code: &'static str,
        message: String,
        suggestion: String,
        /// 建议等待时间（毫秒）。
        retry_after_ms: u64,
    },

    /// 参数校验失败（请求参数不合法）。
    ValidationError {
        code: &'static str,
        message: String,
        suggestion: String,
    },

    /// 交易所返回错误（业务逻辑错误，如余额不足、市场已关闭）。
    ExchangeError {
        code: &'static str,
        message: String,
        suggestion: String,
    },

    /// 请求超时。
    TimeoutError {
        code: &'static str,
        message: String,
        suggestion: String,
        /// 超时时间（毫秒）。
        timeout_ms: u64,
    },

    /// 序列化/反序列化错误（JSON 解析失败、字段缺失）。
    SerializationError {
        code: &'static str,
        message: String,
        suggestion: String,
    },
}

impl GatewayError {
    /// 创建网络错误。
    pub fn network(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::NetworkError {
            code: "GW_NET_001",
            message: msg.clone(),
            suggestion: format!(
                "请检查网络连接和 API 地址是否正确。错误详情: {}",
                msg
            ),
        }
    }

    /// 创建认证错误。
    pub fn authentication(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::AuthenticationError {
            code: "GW_AUTH_001",
            message: msg.clone(),
            suggestion: format!(
                "请检查 API 密钥是否已正确配置（环境变量），以及密钥是否有效。错误详情: {}",
                msg
            ),
        }
    }

    /// 创建速率限制错误。
    pub fn rate_limit(message: impl Into<String>, retry_after_ms: u64) -> Self {
        let msg = message.into();
        Self::RateLimitError {
            code: "GW_RATE_001",
            message: msg.clone(),
            suggestion: format!(
                "请求频率过高，请等待 {} 毫秒后重试。错误详情: {}",
                retry_after_ms, msg
            ),
            retry_after_ms,
        }
    }

    /// 创建参数校验错误。
    pub fn validation(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::ValidationError {
            code: "GW_VAL_001",
            message: msg.clone(),
            suggestion: format!(
                "请检查请求参数是否合法（价格范围、数量、市场 ID 等）。错误详情: {}",
                msg
            ),
        }
    }

    /// 创建交易所错误。
    pub fn exchange(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::ExchangeError {
            code: "GW_EXCH_001",
            message: msg.clone(),
            suggestion: format!(
                "交易所返回错误，请检查账户状态、余额和市场状态。错误详情: {}",
                msg
            ),
        }
    }

    /// 创建超时错误。
    pub fn timeout(message: impl Into<String>, timeout_ms: u64) -> Self {
        let msg = message.into();
        Self::TimeoutError {
            code: "GW_TO_001",
            message: msg.clone(),
            suggestion: format!(
                "请求超时（{}ms），请检查网络延迟或增加超时配置。错误详情: {}",
                timeout_ms, msg
            ),
            timeout_ms,
        }
    }

    /// 创建序列化错误。
    pub fn serialization(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self::SerializationError {
            code: "GW_SER_001",
            message: msg.clone(),
            suggestion: format!(
                "数据格式错误，请检查 JSON 结构是否与 API 文档一致。错误详情: {}",
                msg
            ),
        }
    }

    /// 错误码。
    pub fn code(&self) -> &'static str {
        match self {
            Self::NetworkError { code, .. }
            | Self::AuthenticationError { code, .. }
            | Self::RateLimitError { code, .. }
            | Self::ValidationError { code, .. }
            | Self::ExchangeError { code, .. }
            | Self::TimeoutError { code, .. }
            | Self::SerializationError { code, .. } => code,
        }
    }

    /// 中文错误消息。
    pub fn message_zh(&self) -> &str {
        match self {
            Self::NetworkError { message, .. }
            | Self::AuthenticationError { message, .. }
            | Self::RateLimitError { message, .. }
            | Self::ValidationError { message, .. }
            | Self::ExchangeError { message, .. }
            | Self::TimeoutError { message, .. }
            | Self::SerializationError { message, .. } => message,
        }
    }

    /// 中文建议处理方式。
    pub fn suggestion_zh(&self) -> &str {
        match self {
            Self::NetworkError { suggestion, .. }
            | Self::AuthenticationError { suggestion, .. }
            | Self::RateLimitError { suggestion, .. }
            | Self::ValidationError { suggestion, .. }
            | Self::ExchangeError { suggestion, .. }
            | Self::TimeoutError { suggestion, .. }
            | Self::SerializationError { suggestion, .. } => suggestion,
        }
    }

    /// 错误类型中文名称。
    pub fn kind_zh(&self) -> &'static str {
        match self {
            Self::NetworkError { .. } => "网络错误",
            Self::AuthenticationError { .. } => "认证失败",
            Self::RateLimitError { .. } => "速率限制",
            Self::ValidationError { .. } => "参数校验失败",
            Self::ExchangeError { .. } => "交易所错误",
            Self::TimeoutError { .. } => "请求超时",
            Self::SerializationError { .. } => "序列化错误",
        }
    }

    /// 是否可重试。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkError { .. }
                | Self::RateLimitError { .. }
                | Self::TimeoutError { .. }
        )
    }

    /// 转换为 GatewayResult::failed。
    pub fn to_failed_result(&self, order_id: &str, latency_ms: u64) -> GatewayResult {
        GatewayResult::failed(
            order_id,
            &format!("[{}] {}", self.kind_zh(), self.message_zh()),
            latency_ms,
        )
    }
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} — 建议: {}",
            self.code(),
            self.message_zh(),
            self.suggestion_zh()
        )
    }
}

impl std::error::Error for GatewayError {}

impl From<pm_api_test::client::http::ApiClientError> for GatewayError {
    fn from(err: pm_api_test::client::http::ApiClientError) -> Self {
        match err {
            pm_api_test::client::http::ApiClientError::RequestFailed(msg) => {
                Self::network(msg)
            }
            pm_api_test::client::http::ApiClientError::MaxRetriesExceeded {
                attempts,
                last_error,
            } => Self::network(format!(
                "重试耗尽（{}次），最后错误: {}",
                attempts, last_error
            )),
            pm_api_test::client::http::ApiClientError::MockDataNotFound { endpoint } => {
                Self::validation(format!("Mock 数据未找到: {}", endpoint))
            }
            pm_api_test::client::http::ApiClientError::JsonParseError(msg) => {
                Self::serialization(msg)
            }
            pm_api_test::client::http::ApiClientError::Timeout(ms) => {
                Self::timeout(format!("请求超时: {}ms", ms), ms)
            }
            pm_api_test::client::http::ApiClientError::RateLimited(msg) => {
                Self::rate_limit(msg, 1000)
            }
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_error_has_code() {
        let err = GatewayError::network("连接被拒绝");
        assert_eq!(err.code(), "GW_NET_001");
        assert!(err.message_zh().contains("连接被拒绝"));
        assert!(err.suggestion_zh().contains("网络连接"));
        assert!(err.is_retryable());
    }

    #[test]
    fn auth_error_has_code() {
        let err = GatewayError::authentication("API 密钥无效");
        assert_eq!(err.code(), "GW_AUTH_001");
        assert!(!err.is_retryable());
    }

    #[test]
    fn rate_limit_error_has_retry_after() {
        let err = GatewayError::rate_limit("触发限流", 5000);
        assert_eq!(err.code(), "GW_RATE_001");
        assert!(err.is_retryable());
        if let GatewayError::RateLimitError { retry_after_ms, .. } = &err {
            assert_eq!(*retry_after_ms, 5000);
        }
    }

    #[test]
    fn validation_error_not_retryable() {
        let err = GatewayError::validation("价格超出范围");
        assert_eq!(err.code(), "GW_VAL_001");
        assert!(!err.is_retryable());
    }

    #[test]
    fn exchange_error_display() {
        let err = GatewayError::exchange("余额不足");
        let display = format!("{}", err);
        assert!(display.contains("GW_EXCH_001"));
        assert!(display.contains("余额不足"));
        assert!(display.contains("建议"));
    }

    #[test]
    fn timeout_error() {
        let err = GatewayError::timeout("请求超时", 10000);
        assert!(err.is_retryable());
        assert_eq!(err.code(), "GW_TO_001");
    }

    #[test]
    fn serialization_error() {
        let err = GatewayError::serialization("字段 'price' 缺失");
        assert_eq!(err.code(), "GW_SER_001");
        assert!(!err.is_retryable());
    }

    #[test]
    fn to_failed_result() {
        let err = GatewayError::network("连接失败");
        let result = err.to_failed_result("order-1", 42);
        assert!(!result.success);
        assert!(result.message.contains("网络错误"));
        assert!(result.message.contains("连接失败"));
    }

    #[test]
    fn from_api_client_error() {
        let api_err =
            pm_api_test::client::http::ApiClientError::RequestFailed("连接超时".into());
        let gw_err: GatewayError = api_err.into();
        assert_eq!(gw_err.code(), "GW_NET_001");
        assert!(gw_err.is_retryable());
    }

    #[test]
    fn kind_zh_is_chinese() {
        assert_eq!(GatewayError::network("").kind_zh(), "网络错误");
        assert_eq!(GatewayError::authentication("").kind_zh(), "认证失败");
        assert_eq!(GatewayError::rate_limit("", 0).kind_zh(), "速率限制");
        assert_eq!(GatewayError::validation("").kind_zh(), "参数校验失败");
        assert_eq!(GatewayError::exchange("").kind_zh(), "交易所错误");
        assert_eq!(GatewayError::timeout("", 0).kind_zh(), "请求超时");
        assert_eq!(GatewayError::serialization("").kind_zh(), "序列化错误");
    }

    #[test]
    fn error_implements_std_error() {
        let err = GatewayError::network("测试");
        let _: &dyn std::error::Error = &err;
    }
}