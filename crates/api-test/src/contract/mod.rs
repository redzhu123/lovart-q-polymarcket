//! 合约测试模块（V1.08）。
//!
//! 每个 Polymarket API 端点对应一个合约测试文件。
//! 合约测试定义端点的期望行为：
//! - HTTP 方法 + 路径
//! - 期望状态码
//! - JSON Schema 名称
//! - 字段校验规则
//!
//! 支持 Mock 和 Live 两种测试模式。

pub mod auth;
pub mod balance;
pub mod health;
pub mod markets;
pub mod orderbook;
pub mod orders;
pub mod positions;
pub mod trades;

use crate::client::http::ApiClient;
use crate::validator::response::{ResponseValidator, ValidationResult};
use serde_json::Value;

/// 合约测试定义。
pub struct ContractTest {
    /// 接口名称（中文）。
    pub name: String,
    /// HTTP 方法。
    pub method: HttpMethod,
    /// API 路径。
    pub path: String,
    /// 是否需要认证。
    pub requires_auth: bool,
    /// 期望 HTTP 状态码。
    pub expected_status: u16,
    /// JSON Schema 名称。
    pub schema_name: String,
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

impl ContractTest {
    /// 创建新的合约测试。
    pub fn new(
        name: &str,
        method: HttpMethod,
        path: &str,
        requires_auth: bool,
        expected_status: u16,
        schema_name: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            method,
            path: path.to_string(),
            requires_auth,
            expected_status,
            schema_name: schema_name.to_string(),
        }
    }

    /// 执行 Mock 测试。
    pub async fn run_mock(
        &self,
        client: &ApiClient,
        validator: &ResponseValidator,
    ) -> ValidationResult {
        tracing::info!(
            "【合约测试-Mock】{} {} {}",
            self.method.as_str(),
            self.path,
            self.name
        );

        let response = match self.method {
            HttpMethod::Get => client.get(&self.path).await,
            HttpMethod::Post => client.post(&self.path, None).await,
            HttpMethod::Delete => client.delete(&self.path, None).await,
            HttpMethod::Put => client.put(&self.path, None).await,
            HttpMethod::Patch => client.patch(&self.path, None).await,
        };

        match response {
            Ok(resp) => {
                validator.validate(
                    &self.name,
                    &resp,
                    &self.schema_name,
                    self.expected_status,
                    None::<fn(&Value) -> Vec<crate::validator::field::FieldCheckResult>>,
                )
            }
            Err(e) => {
                let mut result = ValidationResult::new(&self.name);
                result.add_error(&format!("请求失败: {}", e));
                tracing::error!("{} 合约测试请求失败: {}", self.name, e);
                result
            }
        }
    }

    /// 执行 Live 测试（只在 client 为 Live 模式时发送真实请求）。
    pub async fn run_live(
        &self,
        client: &ApiClient,
        validator: &ResponseValidator,
    ) -> ValidationResult {
        if !client.is_live() {
            let mut result = ValidationResult::new(&self.name);
            result.add_warning("当前为 Mock 模式，跳过 Live 测试");
            tracing::info!("{} 跳过 Live 测试（Mock 模式）", self.name);
            return result;
        }

        tracing::info!(
            "【合约测试-Live】{} {} {}",
            self.method.as_str(),
            self.path,
            self.name
        );

        let response = match self.method {
            HttpMethod::Get => client.get(&self.path).await,
            HttpMethod::Post => client.post(&self.path, None).await,
            HttpMethod::Delete => client.delete(&self.path, None).await,
            HttpMethod::Put => client.put(&self.path, None).await,
            HttpMethod::Patch => client.patch(&self.path, None).await,
        };

        match response {
            Ok(resp) => {
                validator.validate(
                    &self.name,
                    &resp,
                    &self.schema_name,
                    self.expected_status,
                    None::<fn(&Value) -> Vec<crate::validator::field::FieldCheckResult>>,
                )
            }
            Err(e) => {
                let mut result = ValidationResult::new(&self.name);
                result.add_error(&format!("Live 请求失败: {}", e));
                tracing::error!("{} Live 测试请求失败: {}", self.name, e);
                result
            }
        }
    }
}
