//! 分布式追踪模块：统一 Span、CorrelationId、RequestId 管理。
//!
//! 从 `pm-gateway::middleware::tracing_mw` 和 `pm-gateway::middleware::logger` 提取并统一。
//!
//! # 核心能力
//!
//! - [`TracingConfig`]：统一的追踪配置
//! - [`CorrelationId`]：跨服务关联 ID
//! - [`RequestId`]：单次请求 ID
//! - [`init_tracing`]：初始化 tracing subscriber

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_subscriber::EnvFilter;

/// 生成简短的随机 hex 字符串
fn random_hex(len: usize) -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
    let seed = hasher.finish();
    let chars: Vec<char> = "0123456789abcdef".chars().collect();
    let mut result = String::with_capacity(len);
    let mut s = seed;
    for _ in 0..len {
        result.push(chars[(s & 0xF) as usize]);
        s >>= 4;
    }
    result
}

/// 追踪配置
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// 日志级别（"trace", "debug", "info", "warn", "error"）
    pub level: String,
    /// 环境变量过滤器名称
    pub env_filter: String,
    /// 是否显示 target
    pub with_target: bool,
    /// 是否显示线程 ID
    pub with_thread_ids: bool,
    /// 是否显示行号
    pub with_line_number: bool,
    /// 是否启用结构化日志（JSON 输出，预留）
    pub structured: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            env_filter: "PM_INFRA_LOG".to_string(),
            with_target: false,
            with_thread_ids: false,
            with_line_number: false,
            structured: false,
        }
    }
}

/// 跨服务关联 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    /// 创建新的随机 CorrelationId
    pub fn new() -> Self {
        Self(random_hex(16))
    }

    /// 从字符串创建
    pub fn from_string(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 单次请求 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub String);

impl RequestId {
    /// 创建新的随机 RequestId
    pub fn new() -> Self {
        Self(random_hex(8))
    }

    /// 从字符串创建
    pub fn from_string(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 全局 CorrelationId 存储（用于跨 span 传播）
static CURRENT_CORRELATION_ID: Mutex<Option<CorrelationId>> = Mutex::new(None);

/// 获取当前上下文的 CorrelationId
pub fn current_correlation_id() -> Option<CorrelationId> {
    CURRENT_CORRELATION_ID
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 设置当前上下文的 CorrelationId
pub fn set_correlation_id(id: CorrelationId) {
    if let Ok(mut guard) = CURRENT_CORRELATION_ID.lock() {
        *guard = Some(id);
    }
}

/// 清除当前上下文的 CorrelationId
pub fn clear_correlation_id() {
    if let Ok(mut guard) = CURRENT_CORRELATION_ID.lock() {
        *guard = None;
    }
}

/// 使用指定配置初始化 tracing
///
/// 返回 Ok(()) 表示初始化成功。如果 tracing 已经初始化，则静默忽略。
pub fn init_tracing(config: &TracingConfig) -> anyhow::Result<()> {
    let env_filter = EnvFilter::try_from_env(&config.env_filter)
        .unwrap_or_else(|_| EnvFilter::new(&config.level))
        .add_directive("hyper=warn".parse()?)
        .add_directive("hyper_util=warn".parse()?)
        .add_directive("reqwest=warn".parse()?)
        .add_directive("tower=warn".parse()?)
        .add_directive("rustls=warn".parse()?);

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(config.with_target)
        .with_thread_ids(config.with_thread_ids)
        .with_line_number(config.with_line_number);

    // try_init 在已初始化时静默忽略
    let _ = subscriber.try_init();

    tracing::info!(
        "追踪系统已初始化: level={}, env_filter={}",
        config.level,
        config.env_filter
    );
    Ok(())
}

/// 使用默认配置初始化 tracing（读取 PM_INFRA_LOG 环境变量）
pub fn init_default_tracing() -> anyhow::Result<()> {
    init_tracing(&TracingConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_id_is_unique() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn correlation_id_from_string() {
        let id = CorrelationId::from_string("test-correlation-123");
        assert_eq!(id.0, "test-correlation-123");
    }

    #[test]
    fn request_id_is_unique() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn request_id_display() {
        let id = RequestId::from_string("abcd1234");
        assert_eq!(format!("{}", id), "abcd1234");
    }

    #[test]
    fn set_and_get_correlation_id() {
        let id = CorrelationId::from_string("test-id");
        set_correlation_id(id.clone());
        let current = current_correlation_id();
        assert!(current.is_some());
        assert_eq!(current.unwrap(), id);
        clear_correlation_id();
        assert!(current_correlation_id().is_none());
    }

    #[test]
    fn tracing_config_default_values() {
        let cfg = TracingConfig::default();
        assert_eq!(cfg.level, "info");
        assert_eq!(cfg.env_filter, "PM_INFRA_LOG");
        assert!(!cfg.with_target);
        assert!(!cfg.with_thread_ids);
    }
}
