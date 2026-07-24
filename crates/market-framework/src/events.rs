//! 统一市场事件（P3.0 第九节）。
//!
//! 新增市场事件类型：
//! - `MarketRegistered` — 市场已注册
//! - `MarketRemoved` — 市场已移除
//! - `MarketConnected` — 市场已连接
//! - `MarketDisconnected` — 市场已断开
//! - `MarketHealthChanged` — 市场健康状态变化
//! - `MarketCapabilityChanged` — 市场能力变化
//!
//! 这些事件通过统一的 EventBus 发布。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::capability::MarketCapability;
use crate::health::MarketHealthStatus;

// ============================================================================
// MarketEvent
// ============================================================================

/// 市场事件枚举（P3.0 第九节）。
///
/// 所有市场相关事件通过此枚举统一表示。
/// 集成到 pm-infrastructure 的 EventBus 中。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum MarketEvent {
    /// 市场插件已注册。
    ///
    /// 触发时机：MarketRegistry::register() 成功时。
    MarketRegistered {
        /// 插件 ID。
        plugin_id: String,
        /// 插件名称。
        plugin_name: String,
        /// 能力数量。
        capability_count: usize,
        /// 时间戳。
        timestamp: DateTime<Local>,
    },

    /// 市场插件已移除。
    ///
    /// 触发时机：MarketRegistry::unregister() 成功时。
    MarketRemoved {
        /// 插件 ID。
        plugin_id: String,
        /// 插件名称。
        plugin_name: String,
        /// 原因。
        reason: String,
        /// 时间戳。
        timestamp: DateTime<Local>,
    },

    /// 市场已连接。
    ///
    /// 触发时机：MarketPlugin::connect() 成功时。
    MarketConnected {
        /// 插件 ID。
        plugin_id: String,
        /// 插件名称。
        plugin_name: String,
        /// 连接耗时（毫秒）。
        latency_ms: u64,
        /// 时间戳。
        timestamp: DateTime<Local>,
    },

    /// 市场已断开。
    ///
    /// 触发时机：MarketPlugin::disconnect() 成功时。
    MarketDisconnected {
        /// 插件 ID。
        plugin_id: String,
        /// 插件名称。
        plugin_name: String,
        /// 原因。
        reason: String,
        /// 时间戳。
        timestamp: DateTime<Local>,
    },

    /// 市场健康状态变化。
    ///
    /// 触发时机：MarketPlugin::health() 返回与之前不同的状态时。
    MarketHealthChanged {
        /// 插件 ID。
        plugin_id: String,
        /// 插件名称。
        plugin_name: String,
        /// 旧状态。
        old_status: MarketHealthStatus,
        /// 新状态。
        new_status: MarketHealthStatus,
        /// 详情。
        detail: String,
        /// 时间戳。
        timestamp: DateTime<Local>,
    },

    /// 市场能力变化。
    ///
    /// 触发时机：市场的能力集合发生变化时（如升级后新增能力）。
    MarketCapabilityChanged {
        /// 插件 ID。
        plugin_id: String,
        /// 插件名称。
        plugin_name: String,
        /// 新增能力。
        added: Vec<MarketCapability>,
        /// 移除能力。
        removed: Vec<MarketCapability>,
        /// 时间戳。
        timestamp: DateTime<Local>,
    },

    /// 市场发现完成。
    ///
    /// 触发时机：Discovery::discover_all() 完成时。
    MarketDiscoveryCompleted {
        /// 发现的插件数量。
        discovered_count: usize,
        /// 插件 ID 列表。
        plugin_ids: Vec<String>,
        /// 时间戳。
        timestamp: DateTime<Local>,
    },
}

impl MarketEvent {
    /// 创建 MarketRegistered 事件。
    pub fn market_registered(plugin_id: impl Into<String>, plugin_name: impl Into<String>) -> Self {
        MarketEvent::MarketRegistered {
            plugin_id: plugin_id.into(),
            plugin_name: plugin_name.into(),
            capability_count: 0,
            timestamp: Local::now(),
        }
    }

    /// 创建 MarketRemoved 事件。
    pub fn market_removed(
        plugin_id: impl Into<String>,
        plugin_name: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        MarketEvent::MarketRemoved {
            plugin_id: plugin_id.into(),
            plugin_name: plugin_name.into(),
            reason: reason.into(),
            timestamp: Local::now(),
        }
    }

    /// 事件名称（英文）。
    pub fn event_name(&self) -> &'static str {
        match self {
            MarketEvent::MarketRegistered { .. } => "MarketRegistered",
            MarketEvent::MarketRemoved { .. } => "MarketRemoved",
            MarketEvent::MarketConnected { .. } => "MarketConnected",
            MarketEvent::MarketDisconnected { .. } => "MarketDisconnected",
            MarketEvent::MarketHealthChanged { .. } => "MarketHealthChanged",
            MarketEvent::MarketCapabilityChanged { .. } => "MarketCapabilityChanged",
            MarketEvent::MarketDiscoveryCompleted { .. } => "MarketDiscoveryCompleted",
        }
    }

    /// 事件名称（中文）。
    pub fn event_name_zh(&self) -> &'static str {
        match self {
            MarketEvent::MarketRegistered { .. } => "市场已注册",
            MarketEvent::MarketRemoved { .. } => "市场已移除",
            MarketEvent::MarketConnected { .. } => "市场已连接",
            MarketEvent::MarketDisconnected { .. } => "市场已断开",
            MarketEvent::MarketHealthChanged { .. } => "市场健康状态变化",
            MarketEvent::MarketCapabilityChanged { .. } => "市场能力变化",
            MarketEvent::MarketDiscoveryCompleted { .. } => "市场发现完成",
        }
    }

    /// 事件时间戳。
    pub fn timestamp(&self) -> DateTime<Local> {
        match self {
            MarketEvent::MarketRegistered { timestamp, .. }
            | MarketEvent::MarketRemoved { timestamp, .. }
            | MarketEvent::MarketConnected { timestamp, .. }
            | MarketEvent::MarketDisconnected { timestamp, .. }
            | MarketEvent::MarketHealthChanged { timestamp, .. }
            | MarketEvent::MarketCapabilityChanged { timestamp, .. }
            | MarketEvent::MarketDiscoveryCompleted { timestamp, .. } => *timestamp,
        }
    }

    /// 事件详情（中文）。
    pub fn detail_zh(&self) -> String {
        match self {
            MarketEvent::MarketRegistered {
                plugin_id,
                plugin_name,
                ..
            } => {
                format!("市场插件已注册: {} ({})", plugin_name, plugin_id)
            }
            MarketEvent::MarketRemoved {
                plugin_id,
                plugin_name,
                reason,
                ..
            } => {
                format!(
                    "市场插件已移除: {} ({})，原因: {}",
                    plugin_name, plugin_id, reason
                )
            }
            MarketEvent::MarketConnected {
                plugin_name,
                latency_ms,
                ..
            } => {
                format!("市场已连接: {}（{}ms）", plugin_name, latency_ms)
            }
            MarketEvent::MarketDisconnected {
                plugin_name,
                reason,
                ..
            } => {
                format!("市场已断开: {}，原因: {}", plugin_name, reason)
            }
            MarketEvent::MarketHealthChanged {
                plugin_name,
                old_status,
                new_status,
                detail,
                ..
            } => {
                format!(
                    "市场健康状态变化: {} 由 {} 变为 {} — {}",
                    plugin_name,
                    old_status.as_zh(),
                    new_status.as_zh(),
                    detail
                )
            }
            MarketEvent::MarketCapabilityChanged {
                plugin_name,
                added,
                removed,
                ..
            } => {
                let mut parts = vec![format!("市场能力变化: {}", plugin_name)];
                if !added.is_empty() {
                    let added_str: Vec<String> =
                        added.iter().map(|c| c.as_zh().to_string()).collect();
                    parts.push(format!("  新增: {}", added_str.join(", ")));
                }
                if !removed.is_empty() {
                    let removed_str: Vec<String> =
                        removed.iter().map(|c| c.as_zh().to_string()).collect();
                    parts.push(format!("  移除: {}", removed_str.join(", ")));
                }
                parts.join("\n")
            }
            MarketEvent::MarketDiscoveryCompleted {
                discovered_count, ..
            } => {
                format!("市场发现完成: 共发现 {} 个市场", discovered_count)
            }
        }
    }

    /// 关联的插件 ID。
    pub fn plugin_id(&self) -> Option<&str> {
        match self {
            MarketEvent::MarketRegistered { plugin_id, .. }
            | MarketEvent::MarketRemoved { plugin_id, .. }
            | MarketEvent::MarketConnected { plugin_id, .. }
            | MarketEvent::MarketDisconnected { plugin_id, .. }
            | MarketEvent::MarketHealthChanged { plugin_id, .. }
            | MarketEvent::MarketCapabilityChanged { plugin_id, .. } => Some(plugin_id),
            MarketEvent::MarketDiscoveryCompleted { .. } => None,
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
    fn event_names_zh() {
        let evt = MarketEvent::market_registered("id", "name");
        assert_eq!(evt.event_name_zh(), "市场已注册");
        assert_eq!(evt.event_name(), "MarketRegistered");
    }

    #[test]
    fn event_detail_zh() {
        let evt = MarketEvent::market_removed("id-1", "测试市场", "手动注销");
        let detail = evt.detail_zh();
        assert!(detail.contains("测试市场"));
        assert!(detail.contains("id-1"));
        assert!(detail.contains("手动注销"));
    }

    #[test]
    fn event_plugin_id() {
        let evt = MarketEvent::market_registered("plugin-1", "名称");
        assert_eq!(evt.plugin_id(), Some("plugin-1"));
    }

    #[test]
    fn discovery_event_no_plugin_id() {
        let evt = MarketEvent::MarketDiscoveryCompleted {
            discovered_count: 5,
            plugin_ids: vec!["a".into(), "b".into()],
            timestamp: Local::now(),
        };
        assert_eq!(evt.plugin_id(), None);
        assert!(evt.detail_zh().contains("5 个"));
    }

    #[test]
    fn health_changed_event() {
        let evt = MarketEvent::MarketHealthChanged {
            plugin_id: "p1".into(),
            plugin_name: "市场".into(),
            old_status: MarketHealthStatus::Healthy,
            new_status: MarketHealthStatus::Unhealthy,
            detail: "REST 超时".into(),
            timestamp: Local::now(),
        };
        let detail = evt.detail_zh();
        assert!(detail.contains("健康"));
        assert!(detail.contains("异常"));
        assert!(detail.contains("REST 超时"));
    }

    #[test]
    fn capability_changed_event() {
        let evt = MarketEvent::MarketCapabilityChanged {
            plugin_id: "p1".into(),
            plugin_name: "市场".into(),
            added: vec![MarketCapability::Margin],
            removed: vec![],
            timestamp: Local::now(),
        };
        let detail = evt.detail_zh();
        assert!(detail.contains("保证金"));
    }
}
