//! API 客户端模块。
//!
//! 提供统一的 HTTP 客户端，支持 GET/POST/DELETE/PUT/PATCH，
//! 自动处理超时、重试、速率限制、代理、认证头。
//! 支持 Mock 和 Live 两种模式。

pub mod config;
pub mod http;

pub use config::ApiTestConfig;
pub use http::ApiClient;
