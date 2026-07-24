//! 市场框架错误类型。
//!
//! 统一所有市场框架操作可能返回的错误，
//! 提供中文错误描述，便于日志诊断和运维排查。

use thiserror::Error;

/// 市场框架错误枚举（P3.0）。
///
/// 覆盖插件注册、发现、健康检查、能力查询、元数据解析等所有错误场景。
#[derive(Debug, Error)]
pub enum MarketFrameworkError {
    /// 插件未找到。
    #[error("插件未找到: {name}")]
    PluginNotFound {
        /// 插件名称。
        name: String,
    },

    /// 插件已注册（重复注册）。
    #[error("插件已注册（重复注册）: {name}")]
    PluginAlreadyRegistered {
        /// 插件名称。
        name: String,
    },

    /// 插件初始化失败。
    #[error("插件初始化失败: {name}，原因: {reason}")]
    PluginInitFailed {
        /// 插件名称。
        name: String,
        /// 失败原因。
        reason: String,
    },

    /// 插件启动失败。
    #[error("插件启动失败: {name}，原因: {reason}")]
    PluginStartFailed {
        /// 插件名称。
        name: String,
        /// 失败原因。
        reason: String,
    },

    /// 插件停止失败。
    #[error("插件停止失败: {name}，原因: {reason}")]
    PluginStopFailed {
        /// 插件名称。
        name: String,
        /// 失败原因。
        reason: String,
    },

    /// 不支持的能力请求。
    #[error("市场 {market} 不支持能力: {capability}")]
    CapabilityNotSupported {
        /// 市场名称。
        market: String,
        /// 请求的能力。
        capability: String,
    },

    /// 健康检查失败。
    #[error("市场 {market} 健康检查失败: {detail}")]
    HealthCheckFailed {
        /// 市场名称。
        market: String,
        /// 详情。
        detail: String,
    },

    /// 元数据无效。
    #[error("市场元数据无效: {detail}")]
    InvalidMetadata {
        /// 详情。
        detail: String,
    },

    /// 发现过程失败。
    #[error("市场发现失败: {detail}")]
    DiscoveryFailed {
        /// 详情。
        detail: String,
    },

    /// 注册表操作失败。
    #[error("注册表操作失败: {detail}")]
    RegistryError {
        /// 详情。
        detail: String,
    },

    /// 连接失败。
    #[error("市场 {market} 连接失败: {detail}")]
    ConnectionFailed {
        /// 市场名称。
        market: String,
        /// 详情。
        detail: String,
    },

    /// 通用错误。
    #[error("市场框架错误: {detail}")]
    Generic {
        /// 详情。
        detail: String,
    },
}

impl MarketFrameworkError {
    /// 错误的简短中文摘要。
    pub fn summary_zh(&self) -> &str {
        match self {
            MarketFrameworkError::PluginNotFound { .. } => "插件未找到",
            MarketFrameworkError::PluginAlreadyRegistered { .. } => "插件重复注册",
            MarketFrameworkError::PluginInitFailed { .. } => "插件初始化失败",
            MarketFrameworkError::PluginStartFailed { .. } => "插件启动失败",
            MarketFrameworkError::PluginStopFailed { .. } => "插件停止失败",
            MarketFrameworkError::CapabilityNotSupported { .. } => "能力不支持",
            MarketFrameworkError::HealthCheckFailed { .. } => "健康检查失败",
            MarketFrameworkError::InvalidMetadata { .. } => "元数据无效",
            MarketFrameworkError::DiscoveryFailed { .. } => "发现失败",
            MarketFrameworkError::RegistryError { .. } => "注册表错误",
            MarketFrameworkError::ConnectionFailed { .. } => "连接失败",
            MarketFrameworkError::Generic { .. } => "通用错误",
        }
    }

    /// 错误码（用于监控告警）。
    pub fn error_code(&self) -> &'static str {
        match self {
            MarketFrameworkError::PluginNotFound { .. } => "MF-001",
            MarketFrameworkError::PluginAlreadyRegistered { .. } => "MF-002",
            MarketFrameworkError::PluginInitFailed { .. } => "MF-003",
            MarketFrameworkError::PluginStartFailed { .. } => "MF-004",
            MarketFrameworkError::PluginStopFailed { .. } => "MF-005",
            MarketFrameworkError::CapabilityNotSupported { .. } => "MF-006",
            MarketFrameworkError::HealthCheckFailed { .. } => "MF-007",
            MarketFrameworkError::InvalidMetadata { .. } => "MF-008",
            MarketFrameworkError::DiscoveryFailed { .. } => "MF-009",
            MarketFrameworkError::RegistryError { .. } => "MF-010",
            MarketFrameworkError::ConnectionFailed { .. } => "MF-011",
            MarketFrameworkError::Generic { .. } => "MF-999",
        }
    }
}

/// 市场框架 Result 类型别名。
pub type MarketFrameworkResult<T> = Result<T, MarketFrameworkError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_summary_zh() {
        let err = MarketFrameworkError::PluginNotFound {
            name: "test".into(),
        };
        assert_eq!(err.summary_zh(), "插件未找到");
    }

    #[test]
    fn error_code_format() {
        let err = MarketFrameworkError::CapabilityNotSupported {
            market: "Polymarket".into(),
            capability: "Perpetual".into(),
        };
        assert_eq!(err.error_code(), "MF-006");
    }

    #[test]
    fn error_display_contains_details() {
        let err = MarketFrameworkError::HealthCheckFailed {
            market: "Binance".into(),
            detail: "WebSocket 连接超时".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Binance"));
        assert!(msg.contains("WebSocket 连接超时"));
    }

    #[test]
    fn all_error_codes_unique() {
        let errors: Vec<Box<dyn Fn(String, String) -> MarketFrameworkError>> = vec![
            Box::new(|n, _d| MarketFrameworkError::PluginNotFound { name: n }),
            Box::new(|n, _| MarketFrameworkError::PluginAlreadyRegistered { name: n }),
            Box::new(|n, d| MarketFrameworkError::PluginInitFailed { name: n, reason: d }),
            Box::new(|n, d| MarketFrameworkError::PluginStartFailed { name: n, reason: d }),
            Box::new(|n, d| MarketFrameworkError::PluginStopFailed { name: n, reason: d }),
            Box::new(|m, c| MarketFrameworkError::CapabilityNotSupported {
                market: m,
                capability: c,
            }),
            Box::new(|m, d| MarketFrameworkError::HealthCheckFailed {
                market: m,
                detail: d,
            }),
            Box::new(|d, _| MarketFrameworkError::InvalidMetadata { detail: d }),
            Box::new(|d, _| MarketFrameworkError::DiscoveryFailed { detail: d }),
            Box::new(|d, _| MarketFrameworkError::RegistryError { detail: d }),
            Box::new(|m, d| MarketFrameworkError::ConnectionFailed {
                market: m,
                detail: d,
            }),
            Box::new(|d, _| MarketFrameworkError::Generic { detail: d }),
        ];

        let mut codes = std::collections::HashSet::new();
        for (i, factory) in errors.iter().enumerate() {
            let err = i.to_string(); // dummy
            let err = factory(err.clone(), err);
            assert!(
                codes.insert(err.error_code().to_string()),
                "重复错误码: {}",
                err.error_code()
            );
        }
    }
}
