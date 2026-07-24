//! REST Transport 抽象层（P2-03）。
//!
//! 定义 HTTP Transport trait 和 ReqwestTransport 实现。
//! 业务层禁止直接访问 reqwest — 所有 HTTP 通信通过此模块。

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;

use crate::error::GatewayError;
use crate::ratelimit::RateLimiter;

/// 简单 UUID v4 生成（避免 uuid crate 依赖）。
fn simple_uuid() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 16] = rand::RngExt::random(&mut rng);
    format!(
        "{:08x}{:04x}{:04x}{:04x}{:08x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u32::from_be_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]),
    )
}

// ============================================================================
// HttpRequest / HttpResponse
// ============================================================================

/// HTTP 请求。
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// HTTP 方法。
    pub method: HttpMethod,
    /// 请求路径（如 "/time"）。
    pub path: String,
    /// 请求体（JSON），可为空。
    pub body: Option<serde_json::Value>,
    /// 请求头。
    pub headers: Vec<(String, String)>,
    /// 请求 ID（用于日志追踪）。
    pub request_id: String,
}

impl HttpRequest {
    /// 创建 GET 请求。
    pub fn get(path: &str) -> Self {
        Self {
            method: HttpMethod::Get,
            path: path.to_string(),
            body: None,
            headers: Vec::new(),
            request_id: simple_uuid(),
        }
    }

    /// 创建 POST 请求。
    pub fn post(path: &str, body: serde_json::Value) -> Self {
        Self {
            method: HttpMethod::Post,
            path: path.to_string(),
            body: Some(body),
            headers: Vec::new(),
            request_id: simple_uuid(),
        }
    }

    /// 创建 DELETE 请求。
    pub fn delete(path: &str) -> Self {
        Self {
            method: HttpMethod::Delete,
            path: path.to_string(),
            body: None,
            headers: Vec::new(),
            request_id: simple_uuid(),
        }
    }

    /// 添加请求头。
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// 设置请求 ID。
    pub fn with_request_id(mut self, id: &str) -> Self {
        self.request_id = id.to_string();
        self
    }
}

/// HTTP 方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
    Put,
    Patch,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// HTTP 响应。
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应体（JSON）。
    pub body: serde_json::Value,
    /// 响应头。
    pub headers: Vec<(String, String)>,
    /// 请求耗时（毫秒）。
    pub latency_ms: u64,
    /// 请求 URL。
    pub url: String,
    /// 请求 ID。
    pub request_id: String,
}

impl HttpResponse {
    /// 是否成功（2xx）。
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// 是否触发限流（429）。
    pub fn is_rate_limited(&self) -> bool {
        self.status == 429
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let icon = if self.is_success() { "✅" } else { "❌" };
        format!(
            "{} HTTP {} | {}ms | {} [{}]",
            icon, self.status, self.latency_ms, self.url, self.request_id
        )
    }
}

// ============================================================================
// HttpTransport Trait
// ============================================================================

/// HTTP Transport 抽象 Trait。
///
/// 业务层通过此 Trait 发送 HTTP 请求，不直接访问 reqwest。
/// 实现：ReqwestTransport（真实 HTTP）、MockTransport（测试）。
#[async_trait]
pub trait HttpTransport: Send + Sync {
    /// 发送 HTTP 请求并返回响应。
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, GatewayError>;

    /// 基础 URL。
    fn base_url(&self) -> &str;

    /// 是否已连接。
    fn is_connected(&self) -> bool;

    /// 连接（初始化传输层）。
    async fn connect(&self) -> Result<(), GatewayError>;

    /// 断开连接（清理资源）。
    async fn disconnect(&self) -> Result<(), GatewayError>;

    /// 使用当前限流器（如果有的话）。
    fn rate_limiter(&self) -> Option<&RateLimiter>;
}

// ============================================================================
// ReqwestTransport（真实 HTTP 实现）
// ============================================================================

/// 基于 pm-api-test::ApiClient 的 HTTP Transport 实现。
///
/// 复用 P2-01 已验证的 ApiClient，提供 Mock/Live 双模式切换。
pub struct ReqwestTransport {
    /// P2-01 已验证的 API 客户端。
    client: pm_api_test::client::http::ApiClient,
    /// 基础 URL。
    base_url: String,
    /// 速率限制器。
    rate_limiter: RateLimiter,
    /// 是否已连接。
    connected: std::sync::atomic::AtomicBool,
}

impl ReqwestTransport {
    /// 创建新的 ReqwestTransport。
    pub fn new(config: pm_api_test::client::config::ApiTestConfig) -> Self {
        let base_url = config.clob_url.clone();
        let rate_limiter = RateLimiter::new(config.rate_limit_per_sec, 60);

        tracing::info!(
            base_url = %base_url,
            mode = %config.mode.as_zh(),
            "HTTP Transport 已创建"
        );

        Self {
            client: pm_api_test::client::http::ApiClient::new(config),
            base_url,
            rate_limiter,
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 获取内部 ApiClient 引用（供 Middleware 使用）。
    pub fn client(&self) -> &pm_api_test::client::http::ApiClient {
        &self.client
    }

    /// 当前模式。
    pub fn is_live(&self) -> bool {
        self.client.is_live()
    }

    /// 构建完整 URL。
    fn build_url(&self, path: &str) -> String {
        if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}{}", self.base_url, path)
        }
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, GatewayError> {
        let start = Instant::now();

        // 速率限制检查
        let wait_ms = self.rate_limiter.acquire();
        if wait_ms > 0 {
            tracing::debug!(
                wait_ms,
                request_id = %req.request_id,
                "速率限制等待"
            );
            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        }

        let url = self.build_url(&req.path);

        // 通过 pm-api-test 的 ApiClient 发送请求
        let result = match req.method {
            HttpMethod::Get => self.client.get(&req.path).await,
            HttpMethod::Post => {
                let body = req.body.as_ref();
                self.client.post(&req.path, body).await
            }
            HttpMethod::Delete => {
                let body = req.body.as_ref();
                self.client.delete(&req.path, body).await
            }
            HttpMethod::Put => {
                let body = req.body.as_ref();
                self.client.put(&req.path, body).await
            }
            HttpMethod::Patch => {
                let body = req.body.as_ref();
                self.client.patch(&req.path, body).await
            }
        };

        match result {
            Ok(api_resp) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                Ok(HttpResponse {
                    status: api_resp.status,
                    body: api_resp.body,
                    headers: api_resp.headers,
                    latency_ms,
                    url,
                    request_id: req.request_id,
                })
            }
            Err(api_err) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                tracing::error!(
                    error = %api_err,
                    request_id = %req.request_id,
                    latency_ms,
                    "HTTP 请求失败"
                );
                Err(GatewayError::from(api_err))
            }
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn connect(&self) -> Result<(), GatewayError> {
        tracing::info!("HTTP Transport 已连接");
        self.connected
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), GatewayError> {
        tracing::info!("HTTP Transport 已断开");
        self.connected
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn rate_limiter(&self) -> Option<&RateLimiter> {
        Some(&self.rate_limiter)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_request_get() {
        let req = HttpRequest::get("/time");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.path, "/time");
        assert!(!req.request_id.is_empty());
    }

    #[test]
    fn http_request_post() {
        let body = serde_json::json!({"key": "value"});
        let req = HttpRequest::post("/order", body);
        assert_eq!(req.method, HttpMethod::Post);
        assert!(req.body.is_some());
    }

    #[test]
    fn http_request_with_header() {
        let req = HttpRequest::get("/time")
            .with_header("Authorization", "Bearer test")
            .with_request_id("req-001");
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.request_id, "req-001");
    }

    #[test]
    fn http_request_unique_ids() {
        let req1 = HttpRequest::get("/time");
        let req2 = HttpRequest::get("/time");
        assert_ne!(req1.request_id, req2.request_id);
    }

    #[test]
    fn http_response_is_success() {
        let resp = HttpResponse {
            status: 200,
            body: serde_json::Value::Null,
            headers: vec![],
            latency_ms: 42,
            url: "/time".into(),
            request_id: "req-001".into(),
        };
        assert!(resp.is_success());
        assert!(!resp.is_rate_limited());
        assert!(resp.summary_zh().contains("✅"));
    }

    #[test]
    fn http_response_rate_limited() {
        let resp = HttpResponse {
            status: 429,
            body: serde_json::Value::Null,
            headers: vec![],
            latency_ms: 10,
            url: "/order".into(),
            request_id: "req-002".into(),
        };
        assert!(!resp.is_success());
        assert!(resp.is_rate_limited());
    }

    #[test]
    fn http_method_display() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
    }

    #[test]
    fn reqwest_transport_creates_in_mock_mode() {
        let config = pm_api_test::client::config::ApiTestConfig::mock();
        let transport = ReqwestTransport::new(config);
        assert!(!transport.is_live());
        assert!(!transport.is_connected());
        assert!(transport.rate_limiter().is_some());
    }
}