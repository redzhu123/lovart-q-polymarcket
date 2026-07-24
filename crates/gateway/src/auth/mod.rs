//! Gateway 认证模块（P2-03）。
//!
//! 统一管理认证头注入，支持多种认证方式。
//! 当前实现：Bearer Token（Polymarket）、Noop（Mock）。

use std::collections::HashMap;

/// 认证提供者 Trait。
///
/// 负责为每个请求生成认证头。
/// 实现：BearerTokenAuth（Polymarket）、NoopAuth（Mock）。
pub trait AuthProvider: Send + Sync {
    /// 名称。
    fn name(&self) -> &str;

    /// 生成认证头（键值对）。
    fn headers(&self) -> HashMap<String, String>;

    /// 是否已认证。
    fn is_authenticated(&self) -> bool;
}

// ============================================================================
// PolymarketAuth（Bearer Token）
// ============================================================================

/// Polymarket API 认证（Bearer Token）。
///
/// 从环境变量读取 API 密钥，注入 `Authorization: Bearer <key>` 头。
pub struct PolymarketAuth {
    /// API 密钥。
    api_key: String,
    /// 环境变量名。
    #[allow(dead_code)]
    api_key_env: String,
}

impl PolymarketAuth {
    /// 创建新的 Polymarket 认证。
    ///
    /// `api_key_env` 是环境变量名（如 `POLYMARKET_API_KEY`）。
    pub fn new(api_key_env: &str) -> Self {
        let api_key = std::env::var(api_key_env).unwrap_or_default();
        if api_key.is_empty() {
            tracing::warn!(
                env_var = %api_key_env,
                "API 密钥未设置（环境变量为空），认证请求将失败"
            );
        } else {
            tracing::info!(
                env_var = %api_key_env,
                "API 密钥已加载（{} 字符）",
                api_key.len()
            );
        }

        Self {
            api_key,
            api_key_env: api_key_env.to_string(),
        }
    }

    /// 获取 API 密钥（如有）。
    pub fn api_key(&self) -> Option<&str> {
        if self.api_key.is_empty() {
            None
        } else {
            Some(&self.api_key)
        }
    }
}

impl AuthProvider for PolymarketAuth {
    fn name(&self) -> &str {
        "PolymarketAuth"
    }

    fn headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        if !self.api_key.is_empty() {
            headers.insert(
                "Authorization".to_string(),
                format!("Bearer {}", self.api_key),
            );
        }
        headers
    }

    fn is_authenticated(&self) -> bool {
        !self.api_key.is_empty()
    }
}

// ============================================================================
// NoopAuth（Mock / 测试用）
// ============================================================================

/// 空认证（Mock / 测试模式）。
///
/// 不注入任何认证头，始终返回已认证。
pub struct NoopAuth;

impl AuthProvider for NoopAuth {
    fn name(&self) -> &str {
        "NoopAuth"
    }

    fn headers(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    fn is_authenticated(&self) -> bool {
        true
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_auth_returns_empty_headers() {
        let auth = NoopAuth;
        assert!(auth.is_authenticated());
        assert!(auth.headers().is_empty());
        assert_eq!(auth.name(), "NoopAuth");
    }

    #[test]
    fn polymarket_auth_without_key() {
        let auth = PolymarketAuth::new("DEFINITELY_NOT_SET_P2_03");
        assert!(!auth.is_authenticated());
        assert!(auth.headers().is_empty());
        assert!(auth.api_key().is_none());
    }

    #[test]
    fn polymarket_auth_name() {
        let auth = PolymarketAuth::new("DEFINITELY_NOT_SET_P2_03");
        assert_eq!(auth.name(), "PolymarketAuth");
    }
}
