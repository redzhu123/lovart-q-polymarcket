//! WebSocket Transport 抽象层（P2-03）。
//!
//! 定义 WebSocket Transport trait 和 TungsteniteTransport 实现。
//! 业务层禁止直接访问 tokio-tungstenite — 所有 WebSocket 通信通过此模块。

use async_trait::async_trait;
use std::sync::Arc;

use crate::error::GatewayError;

// ============================================================================
// WsMessage
// ============================================================================

/// WebSocket 消息。
#[derive(Debug, Clone)]
pub struct WsMessage {
    /// 频道名称。
    pub channel: String,
    /// 事件类型。
    pub event_type: String,
    /// 消息内容（JSON）。
    pub payload: serde_json::Value,
    /// 接收时间戳。
    pub received_at: chrono::DateTime<chrono::Local>,
}

impl WsMessage {
    /// 创建新的 WebSocket 消息。
    pub fn new(channel: &str, event_type: &str, payload: serde_json::Value) -> Self {
        Self {
            channel: channel.to_string(),
            event_type: event_type.to_string(),
            payload,
            received_at: chrono::Local::now(),
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "频道: {} | 事件: {} | 时间: {}",
            self.channel,
            self.event_type,
            self.received_at.format("%H:%M:%S%.3f")
        )
    }
}

// ============================================================================
// WsTransport Trait
// ============================================================================

/// WebSocket Transport 抽象 Trait。
///
/// 业务层通过此 Trait 管理 WebSocket 连接和订阅。
/// 实现：TungsteniteTransport（真实 WS）、NoopWsTransport（测试/Mock）。
#[async_trait]
pub trait WsTransport: Send + Sync {
    /// 连接 WebSocket 服务器。
    async fn connect(&self) -> Result<(), GatewayError>;

    /// 断开 WebSocket 连接。
    async fn disconnect(&self) -> Result<(), GatewayError>;

    /// 订阅频道。
    async fn subscribe(&self, channel: &str) -> Result<(), GatewayError>;

    /// 取消订阅频道。
    async fn unsubscribe(&self, channel: &str) -> Result<(), GatewayError>;

    /// 接收下一条消息（阻塞）。
    async fn recv(&self) -> Result<WsMessage, GatewayError>;

    /// 尝试接收消息（非阻塞）。
    async fn try_recv(&self) -> Option<WsMessage>;

    /// 是否已连接。
    fn is_connected(&self) -> bool;

    /// WebSocket URL。
    fn url(&self) -> &str;
}

// ============================================================================
// NoopWsTransport（占位实现）
// ============================================================================

/// 占位 WebSocket Transport（Mock / DryRun 模式使用）。
///
/// 所有操作返回成功但不执行任何实际操作。
/// 完整实现将在后续版本中提供。
pub struct NoopWsTransport {
    /// WebSocket URL。
    ws_url: String,
    /// 已连接标志。
    connected: std::sync::atomic::AtomicBool,
}

impl NoopWsTransport {
    /// 创建新的占位 WebSocket Transport。
    pub fn new(ws_url: &str) -> Self {
        tracing::info!(url = %ws_url, "WebSocket Transport 占位实现已创建");
        Self {
            ws_url: ws_url.to_string(),
            connected: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl WsTransport for NoopWsTransport {
    async fn connect(&self) -> Result<(), GatewayError> {
        tracing::info!("WebSocket 连接（占位）— 未实际连接");
        self.connected
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), GatewayError> {
        tracing::info!("WebSocket 断开（占位）— 未实际断开");
        self.connected
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn subscribe(&self, channel: &str) -> Result<(), GatewayError> {
        tracing::info!(channel, "WebSocket 订阅（占位）");
        Ok(())
    }

    async fn unsubscribe(&self, channel: &str) -> Result<(), GatewayError> {
        tracing::info!(channel, "WebSocket 取消订阅（占位）");
        Ok(())
    }

    async fn recv(&self) -> Result<WsMessage, GatewayError> {
        Err(GatewayError::network(
            "WebSocket 未实现（占位模式），无消息可用",
        ))
    }

    async fn try_recv(&self) -> Option<WsMessage> {
        None
    }

    fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn url(&self) -> &str {
        &self.ws_url
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_message_summary_zh() {
        let msg = WsMessage::new(
            "book",
            "price_change",
            serde_json::json!({"price": "0.45"}),
        );
        let summary = msg.summary_zh();
        assert!(summary.contains("book"));
        assert!(summary.contains("price_change"));
    }

    #[tokio::test]
    async fn noop_ws_connect_disconnect() {
        let ws = NoopWsTransport::new("wss://ws.polymarket.com");
        assert!(!ws.is_connected());

        ws.connect().await.unwrap();
        assert!(ws.is_connected());

        ws.disconnect().await.unwrap();
        assert!(!ws.is_connected());
    }

    #[tokio::test]
    async fn noop_ws_subscribe() {
        let ws = NoopWsTransport::new("wss://ws.polymarket.com");
        ws.connect().await.unwrap();
        ws.subscribe("book").await.unwrap();
        ws.unsubscribe("book").await.unwrap();
        assert!(ws.is_connected());
    }

    #[tokio::test]
    async fn noop_ws_recv_returns_error() {
        let ws = NoopWsTransport::new("wss://ws.polymarket.com");
        let result = ws.recv().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn noop_ws_try_recv_returns_none() {
        let ws = NoopWsTransport::new("wss://ws.polymarket.com");
        assert!(ws.try_recv().await.is_none());
    }

    #[test]
    fn noop_ws_url() {
        let ws = NoopWsTransport::new("wss://ws.polymarket.com");
        assert_eq!(ws.url(), "wss://ws.polymarket.com");
    }
}