//! Gateway Adapter（V1.08 第四节）。
//!
//! JSON ↔ Order ↔ Execution 统一转换。
//! Execution 禁止直接使用 serde — 全部经 Adapter 转换。

use chrono::{DateTime, Local};
use pm_core::Side;
use pm_execution::order::{Direction, Order, OrderStatus};

use crate::types::{Balance, GatewayResult, OrderRequest, Position};

// ============================================================================
// OrderRequest ↔ Execution Order
// ============================================================================

/// 将 OrderRequest 转换为 Execution Order。
pub fn request_to_order(request: &OrderRequest, order_id: &str, now: DateTime<Local>) -> Order {
    Order::new(
        order_id.to_string(),
        request.client_order_id.clone(),
        request.market_id.clone(),
        "gateway".to_string(),
        request.direction,
        request.side,
        request.price,
        request.quantity,
        request.strategy_id.clone(),
        request.risk_id.clone(),
        request.opportunity_id.clone(),
        now,
    )
}

/// 将 Execution Order 转换为 OrderRequest。
pub fn order_to_request(order: &Order) -> OrderRequest {
    OrderRequest::new(
        &order.market_id,
        order.direction,
        order.side,
        order.price,
        order.quantity,
        &order.strategy_id,
        &order.risk_id,
        &order.opportunity_id,
    )
    .with_client_order_id(&order.client_order_id)
}

// ============================================================================
// GatewayResult → Order 更新
// ============================================================================

/// 将 GatewayResult 应用到 Execution Order。
pub fn apply_result_to_order(order: &mut Order, result: &GatewayResult, now: DateTime<Local>) {
    order.transition(OrderStatus::Submitted, "已提交到 Gateway", now);

    if result.success {
        match result.status {
            OrderStatus::Filled => {
                order.transition(OrderStatus::Accepted, "Gateway 已接受", now);
                order.transition(OrderStatus::Filled, "完全成交", now);
                order.update_fill(result.filled, result.avg_price.unwrap_or(order.price), 0.0);
            }
            OrderStatus::PartiallyFilled => {
                order.transition(OrderStatus::Accepted, "Gateway 已接受", now);
                order.transition(OrderStatus::PartiallyFilled, "部分成交", now);
                order.update_fill(result.filled, result.avg_price.unwrap_or(order.price), 0.0);
            }
            OrderStatus::Cancelled => {
                order.transition(OrderStatus::Cancelled, &result.message, now);
            }
            _ => {
                order.transition(result.status, &result.message, now);
                if result.filled > 0.0 {
                    order.update_fill(result.filled, result.avg_price.unwrap_or(order.price), 0.0);
                }
            }
        }
    } else {
        order.transition(result.status, &result.message, now);
    }
}

// ============================================================================
// JSON ↔ Rust 类型（Polymarket API）
// ============================================================================

/// Polymarket REST API 订单响应 JSON。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PolymarketOrderJson {
    /// 订单 ID。
    pub id: String,
    /// 市场 ID（condition_id / token_id）。
    pub market: String,
    /// 资产 ID（token_id）。
    #[serde(default)]
    pub asset_id: String,
    /// 买卖方向（BUY / SELL）。
    pub side: String,
    /// 订单价格。
    pub price: String,
    /// 原始数量。
    pub original_size: String,
    /// 已成交数量。
    #[serde(default)]
    pub size_matched: String,
    /// 订单状态。
    pub status: String,
    /// 订单类型。
    #[serde(default)]
    pub r#type: String,
    /// 创建时间。
    #[serde(default)]
    pub created_at: String,
}

impl PolymarketOrderJson {
    /// 转换为 GatewayResult。
    pub fn to_gateway_result(&self) -> GatewayResult {
        let filled: f64 = self.size_matched.parse().unwrap_or(0.0);
        let quantity: f64 = self.original_size.parse().unwrap_or(0.0);
        let price: f64 = self.price.parse().unwrap_or(0.0);

        let status = match self.status.to_uppercase().as_str() {
            "LIVE" | "OPEN" | "ACTIVE" => OrderStatus::Accepted,
            "MATCHED" | "FILLED" | "CLOSED" => OrderStatus::Filled,
            "CANCELLED" | "CANCELED" => OrderStatus::Cancelled,
            "EXPIRED" => OrderStatus::Expired,
            _ => OrderStatus::Accepted,
        };

        GatewayResult {
            success: status != OrderStatus::Rejected,
            gateway_order_id: self.id.clone(),
            status,
            filled,
            remaining: (quantity - filled).max(0.0),
            avg_price: if filled > 0.0 { Some(price) } else { None },
            message: format!("Polymarket 订单: {}", self.status),
            latency_ms: 0,
        }
    }
}

/// Polymarket REST API 余额响应 JSON。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PolymarketBalanceJson {
    /// 可用余额。
    #[serde(default)]
    pub available: String,
    /// 总余额。
    #[serde(default)]
    pub total: String,
    /// 已占用。
    #[serde(default)]
    pub locked: String,
    /// 货币。
    #[serde(default)]
    pub currency: String,
}

impl PolymarketBalanceJson {
    /// 转换为 Balance。
    pub fn to_balance(&self, account_id: &str) -> Balance {
        Balance {
            account_id: account_id.to_string(),
            available: self.available.parse().unwrap_or(0.0),
            total: self.total.parse().unwrap_or(0.0),
            locked: self.locked.parse().unwrap_or(0.0),
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            currency: if self.currency.is_empty() {
                "USDC".into()
            } else {
                self.currency.clone()
            },
            updated_at: Some(Local::now()),
        }
    }
}

/// Polymarket REST API 持仓响应 JSON。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PolymarketPositionJson {
    /// 持仓 ID。
    #[serde(default)]
    pub id: String,
    /// 市场 ID。
    #[serde(default)]
    pub condition_id: String,
    /// Token ID。
    #[serde(default)]
    pub token_id: String,
    /// 数量。
    #[serde(default)]
    pub size: String,
    /// 平均入场价。
    #[serde(default)]
    pub avg_price: String,
    /// 当前价格。
    #[serde(default)]
    pub current_price: String,
    /// 未实现盈亏。
    #[serde(default)]
    pub unrealized_pnl: String,
    /// 已实现盈亏。
    #[serde(default)]
    pub realized_pnl: String,
}

impl PolymarketPositionJson {
    /// 转换为 Position。
    pub fn to_position(&self) -> Position {
        let quantity: f64 = self.size.parse().unwrap_or(0.0);
        let avg_price: f64 = self.avg_price.parse().unwrap_or(0.0);
        let mark_price: f64 = self.current_price.parse().unwrap_or(0.0);

        Position {
            position_id: self.id.clone(),
            market_id: self.condition_id.clone(),
            question: self.token_id.clone(),
            direction: Direction::Yes, // 默认，实际由 token 决定
            quantity,
            avg_entry_price: avg_price,
            mark_price,
            unrealized_pnl: self.unrealized_pnl.parse().unwrap_or(0.0),
            realized_pnl: self.realized_pnl.parse().unwrap_or(0.0),
            cost_basis: quantity * avg_price,
            market_value: quantity * mark_price,
            updated_at: Some(Local::now()),
        }
    }
}

// ============================================================================
// 方向映射（Polymarket Side ↔ Direction）
// ============================================================================

/// 将 Polymarket Side 字符串映射为 (Direction, Side)。
pub fn parse_polymarket_side(side: &str) -> (Direction, Side) {
    match side.to_uppercase().as_str() {
        "BUY" => (Direction::Yes, Side::Buy),
        "SELL" => (Direction::No, Side::Sell),
        _ => (Direction::Yes, Side::Buy),
    }
}

/// 将 (Direction, Side) 映射为 Polymarket Side 字符串。
pub fn to_polymarket_side(direction: Direction, side: Side) -> &'static str {
    match (direction, side) {
        (Direction::Yes, Side::Buy) => "BUY",
        (Direction::No, Side::Sell) => "SELL",
        (Direction::Yes, Side::Sell) => "SELL",
        (Direction::No, Side::Buy) => "BUY",
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_to_order_conversion() {
        let req = OrderRequest::new(
            "mkt-1",
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            "S1",
            "R1",
            "O1",
        );
        let now = Local::now();
        let order = request_to_order(&req, "EX-001", now);
        assert_eq!(order.market_id, "mkt-1");
        assert_eq!(order.direction, Direction::Yes);
        assert_eq!(order.price, 0.45);
        assert!((order.quantity - 100.0).abs() < 1e-9);
    }

    #[test]
    fn order_to_request_roundtrip() {
        let req = OrderRequest::new(
            "mkt-1",
            Direction::No,
            Side::Sell,
            0.50,
            200.0,
            "S2",
            "R2",
            "O2",
        );
        let now = Local::now();
        let order = request_to_order(&req, "EX-001", now);
        let req2 = order_to_request(&order);
        assert_eq!(req2.market_id, "mkt-1");
        assert_eq!(req2.direction, Direction::No);
        assert_eq!(req2.price, 0.50);
    }

    #[test]
    fn apply_fill_result_to_order() {
        let req = OrderRequest::new(
            "mkt-1",
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            "S",
            "R",
            "O",
        );
        let now = Local::now();
        let mut order = request_to_order(&req, "EX-001", now);

        let result = GatewayResult::filled("GW-001", 100.0, 0.452, 10);
        apply_result_to_order(&mut order, &result, now);

        assert_eq!(order.status, OrderStatus::Filled);
        assert!((order.filled - 100.0).abs() < 1e-9);
        assert!((order.avg_fill_price - 0.452).abs() < 1e-9);
    }

    #[test]
    fn parse_polymarket_side_mapping() {
        let (d, s) = parse_polymarket_side("BUY");
        assert_eq!(d, Direction::Yes);
        assert_eq!(s, Side::Buy);

        let (d, s) = parse_polymarket_side("SELL");
        assert_eq!(d, Direction::No);
        assert_eq!(s, Side::Sell);
    }

    #[test]
    fn to_polymarket_side_mapping() {
        assert_eq!(to_polymarket_side(Direction::Yes, Side::Buy), "BUY");
        assert_eq!(to_polymarket_side(Direction::No, Side::Sell), "SELL");
    }

    #[test]
    fn polymarket_order_json_parsing() {
        let json = PolymarketOrderJson {
            id: "order-123".into(),
            market: "cond-456".into(),
            asset_id: "token-789".into(),
            side: "BUY".into(),
            price: "0.45".into(),
            original_size: "100.0".into(),
            size_matched: "100.0".into(),
            status: "FILLED".into(),
            r#type: "GTC".into(),
            created_at: "2024-01-01T00:00:00Z".into(),
        };

        let result = json.to_gateway_result();
        assert!(result.success);
        assert_eq!(result.status, OrderStatus::Filled);
        assert!((result.filled - 100.0).abs() < 1e-9);
    }

    #[test]
    fn polymarket_balance_json_parsing() {
        let json = PolymarketBalanceJson {
            available: "9500.50".into(),
            total: "10000.00".into(),
            locked: "499.50".into(),
            currency: "USDC".into(),
        };

        let balance = json.to_balance("account-1");
        assert!((balance.available - 9500.50).abs() < 0.01);
        assert!((balance.total - 10000.00).abs() < 0.01);
        assert!((balance.locked - 499.50).abs() < 0.01);
        assert_eq!(balance.currency, "USDC");
    }

    #[test]
    fn polymarket_position_json_parsing() {
        let json = PolymarketPositionJson {
            id: "pos-1".into(),
            condition_id: "cond-1".into(),
            token_id: "token-1".into(),
            size: "100.0".into(),
            avg_price: "0.45".into(),
            current_price: "0.50".into(),
            unrealized_pnl: "5.00".into(),
            realized_pnl: "0.00".into(),
        };

        let pos = json.to_position();
        assert!((pos.quantity - 100.0).abs() < 1e-9);
        assert!((pos.avg_entry_price - 0.45).abs() < 1e-9);
        assert!((pos.mark_price - 0.50).abs() < 1e-9);
        assert!((pos.unrealized_pnl - 5.00).abs() < 1e-9);
    }
}
