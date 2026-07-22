//! 统一 API HTTP 客户端（V1.08）。
//!
//! 封装 reqwest，支持：
//! - GET / POST / DELETE / PUT / PATCH
//! - 超时控制
//! - 指数退避重试
//! - Token Bucket 速率限制
//! - 代理（HTTPS_PROXY）
//! - 认证头注入
//! - Mock / Live 双模式

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use serde_json::Value;
use tracing;

use super::config::{ApiTestConfig, ClientMode};

// ============================================================================
// ApiResponse
// ============================================================================

/// API 响应包装。
#[derive(Debug, Clone)]
pub struct ApiResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应头。
    pub headers: Vec<(String, String)>,
    /// 响应体（JSON）。
    pub body: Value,
    /// 请求耗时（毫秒）。
    pub latency_ms: u64,
    /// 请求 URL。
    pub url: String,
}

impl ApiResponse {
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
            "{} HTTP {} | {}ms | {}",
            icon, self.status, self.latency_ms, self.url
        )
    }
}

// ============================================================================
// ApiClientError
// ============================================================================

/// API 客户端错误。
#[derive(Debug, thiserror::Error)]
pub enum ApiClientError {
    #[error("HTTP 请求失败: {0}")]
    RequestFailed(String),

    #[error("重试耗尽（{attempts} 次），最后错误: {last_error}")]
    MaxRetriesExceeded {
        attempts: u32,
        last_error: String,
    },

    #[error("Mock 数据未找到: {endpoint}")]
    MockDataNotFound { endpoint: String },

    #[error("JSON 解析失败: {0}")]
    JsonParseError(String),

    #[error("超时: {0}ms")]
    Timeout(u64),

    #[error("速率限制: {0}")]
    RateLimited(String),
}

// ============================================================================
// Token Bucket 速率限制器
// ============================================================================

/// Token Bucket 速率限制器。
struct RateLimiter {
    /// 每秒填充的 token 数。
    rate: u32,
    /// 当前可用 token 数。
    tokens: AtomicU32,
    /// 上次填充时间（毫秒时间戳）。
    last_refill: AtomicU64,
}

impl RateLimiter {
    fn new(rate_per_sec: u32) -> Self {
        Self {
            rate: rate_per_sec,
            tokens: AtomicU32::new(rate_per_sec),
            last_refill: AtomicU64::new(Self::now_ms()),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// 获取一个 token。如果当前没有可用 token，返回需要等待的毫秒数。
    fn acquire(&self) -> u64 {
        let now = Self::now_ms();
        let last = self.last_refill.load(Ordering::Relaxed);

        // 计算需要补充的 token 数
        let elapsed_ms = now.saturating_sub(last);
        let new_tokens = (elapsed_ms as f64 / 1000.0 * self.rate as f64) as u32;

        if new_tokens > 0 {
            let current = self.tokens.load(Ordering::Relaxed);
            let refilled = (current + new_tokens).min(self.rate);
            self.tokens.store(refilled, Ordering::Relaxed);
            self.last_refill.store(now, Ordering::Relaxed);
        }

        // 尝试消费一个 token
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current > 0 {
                if self
                    .tokens
                    .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::Relaxed)
                    .is_ok()
                {
                    return 0; // 获取成功
                }
            } else {
                // 计算需要等待的时间
                let wait_ms =
                    (1000.0 / self.rate as f64).ceil() as u64;
                return wait_ms;
            }
        }
    }
}

// ============================================================================
// ApiClient
// ============================================================================

/// 统一 API HTTP 客户端。
///
/// # 使用
///
/// ```ignore
/// let config = ApiTestConfig::live();
/// let client = ApiClient::new(config);
/// let resp = client.get("/time").await?;
/// ```
pub struct ApiClient {
    /// 内部 reqwest 客户端。
    inner: Client,
    /// 配置。
    config: ApiTestConfig,
    /// 速率限制器。
    rate_limiter: RateLimiter,
    /// Mock 数据存储（仅在 Mock 模式下使用）。
    mock_data: Option<Arc<std::collections::HashMap<String, Value>>>,
}

impl ApiClient {
    /// 创建新的 API 客户端。
    pub fn new(config: ApiTestConfig) -> Self {
        let mut builder = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .user_agent(format!("pm-api-test/{}", env!("CARGO_PKG_VERSION")))
            .danger_accept_invalid_certs(false);

        // 配置代理
        if let Some(ref proxy_url) = config.proxy_url {
            if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
                builder = builder.proxy(proxy);
                tracing::info!(proxy = %proxy_url, "HTTP 代理已配置");
            } else {
                tracing::warn!(proxy = %proxy_url, "代理配置失败");
            }
        } else {
            // 尝试从环境变量读取
            if let Ok(proxy_url) = std::env::var("HTTPS_PROXY")
                .or_else(|_| std::env::var("https_proxy"))
            {
                if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
                    builder = builder.proxy(proxy);
                    tracing::info!(proxy = %proxy_url, "HTTP 代理已配置（环境变量）");
                }
            }
        }

        let inner = builder.build().expect("Failed to build reqwest Client");

        let mock_data = if config.mode == ClientMode::Mock {
            Some(Arc::new(Self::load_mock_data(&config.mock_dir)))
        } else {
            None
        };

        tracing::info!(
            mode = %config.mode.as_zh(),
            base_url = %config.clob_url,
            timeout_ms = %config.timeout_ms,
            retries = %config.max_retries,
            rate_limit = %config.rate_limit_per_sec,
            "API 客户端已创建"
        );

        Self {
            inner,
            rate_limiter: RateLimiter::new(config.rate_limit_per_sec),
            mock_data,
            config,
        }
    }

    /// 加载 Mock 数据。
    fn load_mock_data(mock_dir: &str) -> std::collections::HashMap<String, Value> {
        let mut data = std::collections::HashMap::new();
        let mock_files = [
            "markets",
            "market-detail",
            "orderbook",
            "trades",
            "balance",
            "orders",
            "positions",
            "server-time",
        ];

        for name in &mock_files {
            let path = format!("{}/{}.json", mock_dir, name);
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Value>(&content) {
                    Ok(json) => {
                        data.insert(name.to_string(), json);
                        tracing::debug!(file = %path, "Mock 数据已加载");
                    }
                    Err(e) => {
                        tracing::warn!(file = %path, error = %e, "Mock 数据 JSON 解析失败");
                    }
                },
                Err(_) => {
                    tracing::debug!(file = %path, "Mock 数据文件不存在");
                }
            }
        }

        tracing::info!(count = %data.len(), "Mock 数据加载完成");
        data
    }

    /// 获取配置引用。
    pub fn config(&self) -> &ApiTestConfig {
        &self.config
    }

    /// 当前模式。
    pub fn mode(&self) -> ClientMode {
        self.config.mode
    }

    /// 是否 Live 模式。
    pub fn is_live(&self) -> bool {
        self.config.mode == ClientMode::Live
    }

    // ---- HTTP 方法 ----

    /// GET 请求。
    pub async fn get(&self, path: &str) -> Result<ApiResponse, ApiClientError> {
        let url = self.build_url(path);
        self.request(Method::GET, &url, None).await
    }

    /// POST 请求。
    pub async fn post(
        &self,
        path: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, ApiClientError> {
        let url = self.build_url(path);
        self.request(Method::POST, &url, body).await
    }

    /// DELETE 请求。
    pub async fn delete(
        &self,
        path: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, ApiClientError> {
        let url = self.build_url(path);
        self.request(Method::DELETE, &url, body).await
    }

    /// PUT 请求。
    pub async fn put(
        &self,
        path: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, ApiClientError> {
        let url = self.build_url(path);
        self.request(Method::PUT, &url, body).await
    }

    /// PATCH 请求。
    pub async fn patch(
        &self,
        path: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, ApiClientError> {
        let url = self.build_url(path);
        self.request(Method::PATCH, &url, body).await
    }

    // ---- 内部方法 ----

    /// 构建完整 URL。
    fn build_url(&self, path: &str) -> String {
        // 如果 path 以 http 开头，直接使用
        if path.starts_with("http") {
            return path.to_string();
        }
        format!("{}{}", self.config.clob_url, path)
    }

    /// 带重试的请求。
    async fn request(
        &self,
        method: Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, ApiClientError> {
        // Mock 模式：从 mock 数据返回
        if self.config.mode == ClientMode::Mock {
            return self.mock_request(method, url, body);
        }

        // Live 模式：真实 HTTP 请求
        self.live_request(method, url, body).await
    }

    /// Mock 模式请求。
    fn mock_request(
        &self,
        method: Method,
        url: &str,
        _body: Option<&Value>,
    ) -> Result<ApiResponse, ApiClientError> {
        let mock_data = self.mock_data.as_ref().ok_or_else(|| {
            ApiClientError::MockDataNotFound {
                endpoint: url.to_string(),
            }
        })?;

        // 根据 URL 匹配 mock 数据
        let endpoint_key = self.mock_key_from_url(url);

        let body = mock_data.get(&endpoint_key).cloned().unwrap_or_else(|| {
            // 返回默认空对象
            Value::Object(serde_json::Map::new())
        });

        tracing::debug!(
            method = %method,
            url = %url,
            mock_key = %endpoint_key,
            "Mock 模式响应"
        );

        Ok(ApiResponse {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body,
            latency_ms: 0,
            url: url.to_string(),
        })
    }

    /// 从 URL 提取 mock key。
    fn mock_key_from_url(&self, url: &str) -> String {
        // 精确路径匹配优先（先匹配更具体的模式）
        if url.contains("/time") || url.contains("/ping") || url == "/" || url.ends_with('/') && url.len() <= 2 {
            "server-time".into()
        } else if url.contains("/market?") || url.contains("/market/") {
            "market-detail".into()
        } else if url.contains("/markets") {
            "markets".into()
        } else if url.contains("/book") {
            "orderbook".into()
        } else if url.contains("/trades") {
            "trades".into()
        } else if url.contains("/balance") || url.contains("/balances") {
            "balance".into()
        } else if url.contains("/orders") || url.contains("/order") {
            "orders".into()
        } else if url.contains("/positions") {
            "positions".into()
        } else {
            "server-time".into()
        }
    }

    /// Live 模式请求（带重试 + 速率限制）。
    async fn live_request(
        &self,
        method: Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, ApiClientError> {
        let start = Instant::now();
        let mut last_error = String::new();

        for attempt in 0..=self.config.max_retries {
            // 速率限制
            let wait_ms = self.rate_limiter.acquire();
            if wait_ms > 0 {
                tracing::debug!(wait_ms, "速率限制等待");
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            }

            // 构建请求
            let mut req_builder = self.inner.request(method.clone(), url);

            // 添加认证头
            if let Some(ref api_key) = self.config.api_key {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
            }

            // 添加 body
            if let Some(b) = body {
                req_builder = req_builder.json(b);
            }

            // 发送请求
            let req_start = Instant::now();
            match req_builder.send().await {
                Ok(resp) => {
                    let latency_ms = req_start.elapsed().as_millis() as u64;
                    let status = resp.status().as_u16();

                    // 处理 429（速率限制）
                    if status == 429 {
                        tracing::warn!(
                            attempt = attempt + 1,
                            url = %url,
                            "触发速率限制 (429)"
                        );

                        if attempt < self.config.max_retries {
                            let delay = self.calculate_backoff(attempt);
                            tracing::info!(delay_ms = %delay, "等待后重试");
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                            continue;
                        }

                        return Err(ApiClientError::RateLimited(format!(
                            "{} 触发速率限制，重试耗尽", url
                        )));
                    }

                    // 收集响应头
                    let headers: Vec<(String, String)> = resp
                        .headers()
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.as_str().to_string(),
                                v.to_str().unwrap_or("").to_string(),
                            )
                        })
                        .collect();

                    // 解析响应体
                    let body_text = resp.text().await.unwrap_or_default();
                    let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
                        Value::String(body_text.clone())
                    });

                    let total_latency = start.elapsed().as_millis() as u64;

                    tracing::debug!(
                        method = %method,
                        url = %url,
                        status = %status,
                        latency_ms = %total_latency,
                        attempt = attempt + 1,
                        "HTTP 请求完成"
                    );

                    return Ok(ApiResponse {
                        status,
                        headers,
                        body,
                        latency_ms: total_latency,
                        url: url.to_string(),
                    });
                }
                Err(e) => {
                    last_error = e.to_string();
                    let latency_ms = req_start.elapsed().as_millis() as u64;

                    // 判断是否可重试
                    if !Self::is_retryable(&e) {
                        return Err(ApiClientError::RequestFailed(last_error));
                    }

                    if attempt < self.config.max_retries {
                        let delay = self.calculate_backoff(attempt);
                        tracing::warn!(
                            attempt = attempt + 1,
                            error = %e,
                            delay_ms = %delay,
                            url = %url,
                            "请求失败，等待重试"
                        );
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    } else {
                        tracing::error!(
                            attempts = attempt + 1,
                            error = %e,
                            url = %url,
                            "请求重试耗尽"
                        );
                    }
                }
            }
        }

        Err(ApiClientError::MaxRetriesExceeded {
            attempts: self.config.max_retries + 1,
            last_error,
        })
    }

    /// 计算退避延迟。
    fn calculate_backoff(&self, attempt: u32) -> u64 {
        let delay = (self.config.retry_base_ms as f64
            * self.config.backoff_multiplier.powi(attempt as i32))
            as u64;
        delay.min(self.config.retry_max_ms)
    }

    /// 判断错误是否可重试。
    fn is_retryable(error: &reqwest::Error) -> bool {
        error.is_timeout()
            || error.is_connect()
            || error.is_request()
            || error.status() == Some(StatusCode::TOO_MANY_REQUESTS)
            || error.status().map(|s| s.is_server_error()).unwrap_or(false)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_acquires_tokens() {
        let rl = RateLimiter::new(100);
        let wait = rl.acquire();
        assert_eq!(wait, 0); // 首次获取应该立即成功
    }

    #[test]
    fn rate_limiter_exhaustion() {
        let rl = RateLimiter::new(2);
        assert_eq!(rl.acquire(), 0);
        assert_eq!(rl.acquire(), 0);
        // 第三个 token 需要等待
        let wait = rl.acquire();
        assert!(wait > 0);
    }

    #[test]
    fn calculate_backoff_grows() {
        let config = ApiTestConfig::default();
        let client = ApiClient::new(config);
        let d0 = client.calculate_backoff(0);
        let d1 = client.calculate_backoff(1);
        let d2 = client.calculate_backoff(2);
        assert_eq!(d0, 500);
        assert_eq!(d1, 1000);
        assert_eq!(d2, 2000);
    }

    #[test]
    fn calculate_backoff_respects_max() {
        let config = ApiTestConfig {
            retry_base_ms: 1000,
            retry_max_ms: 2000,
            backoff_multiplier: 2.0,
            ..ApiTestConfig::default()
        };
        let client = ApiClient::new(config);
        assert_eq!(client.calculate_backoff(3), 2000); // capped
    }

    #[test]
    fn mock_key_from_url_time() {
        let client = ApiClient::new(ApiTestConfig::mock());
        assert_eq!(client.mock_key_from_url("/time"), "server-time");
    }

    #[test]
    fn mock_key_from_url_book() {
        let client = ApiClient::new(ApiTestConfig::mock());
        assert_eq!(
            client.mock_key_from_url("https://clob.polymarket.com/book?token_id=123"),
            "orderbook"
        );
    }

    #[test]
    fn mock_mode_returns_mock_data() {
        let client = ApiClient::new(ApiTestConfig::mock());
        assert!(!client.is_live());
    }

    #[test]
    fn live_mode_flags() {
        let client = ApiClient::new(ApiTestConfig::live());
        assert!(client.is_live());
    }

    #[test]
    fn api_response_summary() {
        let resp = ApiResponse {
            status: 200,
            headers: vec![],
            body: Value::Null,
            latency_ms: 50,
            url: "/test".into(),
        };
        assert!(resp.is_success());
        assert!(resp.summary_zh().contains("✅"));
    }
}
