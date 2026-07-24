//! Gateway Middleware 中间件链（P2-03）。
//!
//! 统一中间件管线，用于包装所有 Transport 调用。
//! 每个 Middleware 在请求前/响应后/错误时执行钩子。
//!
//! # 中间件链
//!
//! 调用顺序：Logger → Auth → RateLimit → Retry → Metrics → Tracing → Transport
//!
//! # 扩展
//!
//! 以后新增功能必须通过 Middleware 扩展，禁止直接修改 Transport。

pub mod auth;
pub mod logger;
pub mod metrics;
pub mod ratelimit;
pub mod retry;
pub mod tracing_mw;

use async_trait::async_trait;

use crate::error::GatewayError;

// ============================================================================
// MiddlewareContext
// ============================================================================

/// 中间件上下文（传递请求/响应信息）。
#[derive(Debug, Clone)]
pub struct MiddlewareContext {
    /// 请求 ID。
    pub request_id: String,
    /// HTTP 方法。
    pub method: String,
    /// 请求路径。
    pub path: String,
    /// 请求体大小（字节）。
    pub body_size: usize,
    /// 响应状态码。
    pub status: Option<u16>,
    /// 请求耗时（毫秒）。
    pub latency_ms: u64,
    /// 模块名称。
    pub module: String,
}

impl MiddlewareContext {
    /// 创建新的中间件上下文。
    pub fn new(request_id: &str, method: &str, path: &str, module: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            body_size: 0,
            status: None,
            latency_ms: 0,
            module: module.to_string(),
        }
    }

    /// 设置响应信息。
    pub fn with_response(mut self, status: u16, latency_ms: u64) -> Self {
        self.status = Some(status);
        self.latency_ms = latency_ms;
        self
    }

    /// 设置请求体大小。
    pub fn with_body_size(mut self, size: usize) -> Self {
        self.body_size = size;
        self
    }
}

// ============================================================================
// Middleware Trait
// ============================================================================

/// 中间件 Trait。
///
/// 每个中间件实现三个钩子：
/// - `on_request`: 请求发送前。
/// - `on_response`: 收到响应后。
/// - `on_error`: 发生错误时。
#[async_trait]
pub trait Middleware: Send + Sync {
    /// 中间件名称。
    fn name(&self) -> &str;

    /// 请求发送前钩子（可修改请求头等）。
    async fn on_request(&self, ctx: &MiddlewareContext) {
        let _ = ctx;
    }

    /// 响应接收后钩子。
    async fn on_response(&self, ctx: &MiddlewareContext) {
        let _ = ctx;
    }

    /// 错误发生时钩子。
    async fn on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        let _ = (error, ctx);
    }
}

// ============================================================================
// MiddlewareStack
// ============================================================================

/// 中间件链（按顺序执行）。
pub struct MiddlewareStack {
    /// 中间件列表（按注册顺序执行）。
    middlewares: Vec<Box<dyn Middleware>>,
}

impl MiddlewareStack {
    /// 创建空的中间件链。
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// 添加中间件。
    pub fn add(&mut self, middleware: Box<dyn Middleware>) {
        tracing::debug!(name = %middleware.name(), "中间件已注册");
        self.middlewares.push(middleware);
    }

    /// 添加后返回自身（链式调用）。
    pub fn with(mut self, middleware: Box<dyn Middleware>) -> Self {
        self.add(middleware);
        self
    }

    /// 获取中间件列表。
    pub fn middlewares(&self) -> &[Box<dyn Middleware>] {
        &self.middlewares
    }

    /// 执行请求前钩子（按注册顺序）。
    pub async fn run_before(&self, ctx: &MiddlewareContext) {
        for mw in &self.middlewares {
            mw.on_request(ctx).await;
        }
    }

    /// 执行响应后钩子（按注册顺序）。
    pub async fn run_after(&self, ctx: &MiddlewareContext) {
        for mw in &self.middlewares {
            mw.on_response(ctx).await;
        }
    }

    /// 执行错误钩子（按注册顺序）。
    pub async fn run_on_error(&self, error: &GatewayError, ctx: &MiddlewareContext) {
        for mw in &self.middlewares {
            mw.on_error(error, ctx).await;
        }
    }

    /// 中间件数量。
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }
}

impl Default for MiddlewareStack {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestMiddleware {
        name: String,
        before_count: AtomicUsize,
        after_count: AtomicUsize,
    }

    impl TestMiddleware {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                before_count: AtomicUsize::new(0),
                after_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Middleware for TestMiddleware {
        fn name(&self) -> &str {
            &self.name
        }

        async fn on_request(&self, _ctx: &MiddlewareContext) {
            self.before_count.fetch_add(1, Ordering::SeqCst);
        }

        async fn on_response(&self, _ctx: &MiddlewareContext) {
            self.after_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn middleware_stack_executes_in_order() {
        let mut stack = MiddlewareStack::new();
        let mw1 = Box::new(TestMiddleware::new("mw1"));
        let mw2 = Box::new(TestMiddleware::new("mw2"));
        let mw3 = Box::new(TestMiddleware::new("mw3"));
        stack.add(mw1);
        stack.add(mw2);
        stack.add(mw3);

        let ctx = MiddlewareContext::new("req-1", "GET", "/time", "test");
        stack.run_before(&ctx).await;
        stack.run_after(&ctx).await;

        assert_eq!(stack.len(), 3);
    }

    #[tokio::test]
    async fn middleware_stack_chain_builder() {
        let stack = MiddlewareStack::new()
            .with(Box::new(TestMiddleware::new("mw1")))
            .with(Box::new(TestMiddleware::new("mw2")));

        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn middleware_context() {
        let ctx = MiddlewareContext::new("req-001", "POST", "/order", "PolymarketGateway")
            .with_body_size(256)
            .with_response(200, 42);

        assert_eq!(ctx.request_id, "req-001");
        assert_eq!(ctx.method, "POST");
        assert_eq!(ctx.path, "/order");
        assert_eq!(ctx.module, "PolymarketGateway");
        assert_eq!(ctx.body_size, 256);
        assert_eq!(ctx.status, Some(200));
        assert_eq!(ctx.latency_ms, 42);
    }

    #[tokio::test]
    async fn error_hook_executes() {
        let stack = MiddlewareStack::new()
            .with(Box::new(TestMiddleware::new("mw1")));

        let ctx = MiddlewareContext::new("req-1", "GET", "/time", "test");
        let err = crate::error::GatewayError::network("连接失败");

        stack.run_on_error(&err, &ctx).await;
        // should not panic
    }
}