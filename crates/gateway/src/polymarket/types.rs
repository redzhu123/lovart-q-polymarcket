//! Polymarket API 类型定义（V1.08 第三节）。
//!
//! Polymarket CLOB API 的请求/响应 JSON 类型。
//! 所有字段对应 Polymarket API 文档。

use serde::{Deserialize, Serialize};

// ============================================================================
// REST API 通用响应
// ============================================================================

/// Polymarket API 通用响应包装。
#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    /// 是否成功。
    #[serde(default)]
    pub success: bool,
    /// 错误信息。
    #[serde(default)]
    pub error: String,
    /// 数据。
    #[serde(default)]
    pub data: Option<T>,
}

// ============================================================================
// 订单相关
// ============================================================================

/// 下单请求（发送到 Polymarket API）。
#[derive(Debug, Clone, Serialize)]
pub struct CreateOrderRequest {
    /// 订单类型（GTC / IOC / FOK）。
    pub order_type: String,
    /// 买卖方向（BUY / SELL）。
    pub side: String,
    /// 价格（字符串格式，精度 4 位）。
    pub price: String,
    /// 数量（字符串格式）。
    pub size: String,
    /// Token ID（资产 ID）。
    pub token_id: String,
}

/// Polymarket 订单响应。
#[derive(Debug, Clone, Deserialize)]
pub struct OrderResponse {
    /// 订单 ID。
    pub id: String,
    /// 订单状态（LIVE / MATCHED / CANCELLED / EXPIRED）。
    pub status: String,
    /// 买卖方向。
    pub side: String,
    /// 价格。
    pub price: String,
    /// 原始数量。
    pub original_size: String,
    /// 已成交数量。
    pub size_matched: String,
    /// 剩余数量。
    pub size_remaining: String,
    /// 市场 ID。
    pub market: String,
    /// Token ID。
    pub token_id: String,
    /// 创建时间。
    pub created_at: String,
}

// ============================================================================
// 市场相关
// ============================================================================

/// Polymarket 市场信息。
#[derive(Debug, Clone, Deserialize)]
pub struct MarketResponse {
    /// 市场 ID（condition_id）。
    pub condition_id: String,
    /// 问题文本。
    pub question: String,
    /// 市场状态。
    #[serde(default)]
    pub closed: bool,
    /// Token 列表。
    #[serde(default)]
    pub tokens: Vec<TokenInfo>,
}

/// Token 信息。
#[derive(Debug, Clone, Deserialize)]
pub struct TokenInfo {
    /// Token ID。
    pub token_id: String,
    /// Token 类型（Yes/No）。
    #[serde(default)]
    pub outcome: String,
    /// 当前价格。
    #[serde(default)]
    pub price: String,
}

// ============================================================================
// 订单簿相关
// ============================================================================

/// 订单簿响应。
#[derive(Debug, Clone, Deserialize)]
pub struct OrderBookResponse {
    /// 市场 ID。
    pub market: String,
    /// Token ID。
    pub token_id: String,
    /// 买盘。
    #[serde(default)]
    pub bids: Vec<BookLevel>,
    /// 卖盘。
    #[serde(default)]
    pub asks: Vec<BookLevel>,
}

/// 订单簿档位。
#[derive(Debug, Clone, Deserialize)]
pub struct BookLevel {
    /// 价格。
    pub price: String,
    /// 数量。
    pub size: String,
}

// ============================================================================
// 余额相关
// ============================================================================

/// 余额响应。
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceResponse {
    /// 可用余额。
    #[serde(default)]
    pub available: String,
    /// 总余额。
    #[serde(default)]
    pub total: String,
    /// 已占用。
    #[serde(default)]
    pub locked: String,
}

// ============================================================================
// 错误响应
// ============================================================================

/// Polymarket API 错误。
#[derive(Debug, Clone, Deserialize)]
pub struct ApiError {
    /// 错误码。
    #[serde(default)]
    pub code: String,
    /// 错误消息。
    #[serde(default)]
    pub message: String,
    /// HTTP 状态码。
    #[serde(default)]
    pub status: u16,
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_order_request_serialization() {
        let req = CreateOrderRequest {
            order_type: "GTC".into(),
            side: "BUY".into(),
            price: "0.4500".into(),
            size: "100.00".into(),
            token_id: "token-123".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("GTC"));
        assert!(json.contains("BUY"));
        assert!(json.contains("0.4500"));
    }

    #[test]
    fn order_response_deserialization() {
        let json = r#"{
            "id": "order-1",
            "status": "LIVE",
            "side": "BUY",
            "price": "0.4500",
            "original_size": "100.00",
            "size_matched": "0",
            "size_remaining": "100.00",
            "market": "cond-1",
            "token_id": "token-1",
            "created_at": "2024-01-01T00:00:00Z"
        }"#;

        let resp: OrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "order-1");
        assert_eq!(resp.status, "LIVE");
        assert_eq!(resp.price, "0.4500");
    }

    #[test]
    fn market_response_deserialization() {
        let json = r#"{
            "condition_id": "cond-1",
            "question": "BTC > 100k in 2024?",
            "closed": false,
            "tokens": [
                {"token_id": "yes-token", "outcome": "Yes", "price": "0.45"},
                {"token_id": "no-token", "outcome": "No", "price": "0.55"}
            ]
        }"#;

        let resp: MarketResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.condition_id, "cond-1");
        assert!(!resp.closed);
        assert_eq!(resp.tokens.len(), 2);
    }

    #[test]
    fn balance_response_deserialization() {
        let json = r#"{
            "available": "9500.50",
            "total": "10000.00",
            "locked": "499.50"
        }"#;

        let resp: BalanceResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.available, "9500.50");
        assert_eq!(resp.total, "10000.00");
    }

    #[test]
    fn api_error_deserialization() {
        let json = r#"{
            "code": "INSUFFICIENT_BALANCE",
            "message": "余额不足",
            "status": 400
        }"#;

        let err: ApiError = serde_json::from_str(json).unwrap();
        assert_eq!(err.code, "INSUFFICIENT_BALANCE");
        assert!(err.message.contains("余额"));
    }
}
