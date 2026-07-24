//! Gateway 共享类型（V1.08 第一节）。
//!
//! 定义 Exchange Gateway 使用的所有共享数据类型。
//! Execution 禁止直接使用 serde — 全部经 Adapter 转换。

use chrono::{DateTime, Local};
use pm_core::Side;
use pm_execution::order::{Direction, OrderStatus};
use serde::{Deserialize, Serialize};

// ============================================================================
// Gateway Result
// ============================================================================

/// Gateway 操作结果。
#[derive(Debug, Clone)]
pub struct GatewayResult {
    /// 操作是否成功。
    pub success: bool,
    /// 订单 ID（Gateway 侧的 ID）。
    pub gateway_order_id: String,
    /// Gateway 返回的订单状态。
    pub status: OrderStatus,
    /// 已成交数量。
    pub filled: f64,
    /// 剩余未成交数量。
    pub remaining: f64,
    /// 加权平均成交价。
    pub avg_price: Option<f64>,
    /// Gateway 返回的消息（中文）。
    pub message: String,
    /// 操作耗时（毫秒）。
    pub latency_ms: u64,
}

impl GatewayResult {
    /// 成功提交。
    pub fn accepted(order_id: &str, message: &str, latency_ms: u64) -> Self {
        Self {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Accepted,
            filled: 0.0,
            remaining: 0.0,
            avg_price: None,
            message: message.to_string(),
            latency_ms,
        }
    }

    /// 完全成交。
    pub fn filled(order_id: &str, filled: f64, avg_price: f64, latency_ms: u64) -> Self {
        Self {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Filled,
            filled,
            remaining: 0.0,
            avg_price: Some(avg_price),
            message: "完全成交".to_string(),
            latency_ms,
        }
    }

    /// 部分成交。
    pub fn partially_filled(
        order_id: &str,
        filled: f64,
        remaining: f64,
        avg_price: f64,
        latency_ms: u64,
    ) -> Self {
        Self {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::PartiallyFilled,
            filled,
            remaining,
            avg_price: Some(avg_price),
            message: "部分成交".to_string(),
            latency_ms,
        }
    }

    /// 订单已取消。
    pub fn cancelled(order_id: &str, message: &str, latency_ms: u64) -> Self {
        Self {
            success: true,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Cancelled,
            filled: 0.0,
            remaining: 0.0,
            avg_price: None,
            message: message.to_string(),
            latency_ms,
        }
    }

    /// 订单被拒绝。
    pub fn rejected(order_id: &str, reason: &str, latency_ms: u64) -> Self {
        Self {
            success: false,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Rejected,
            filled: 0.0,
            remaining: 0.0,
            avg_price: None,
            message: reason.to_string(),
            latency_ms,
        }
    }

    /// 订单过期。
    pub fn expired(order_id: &str, latency_ms: u64) -> Self {
        Self {
            success: false,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Expired,
            filled: 0.0,
            remaining: 0.0,
            avg_price: None,
            message: "订单已过期".to_string(),
            latency_ms,
        }
    }

    /// 操作失败。
    pub fn failed(order_id: &str, error: &str, latency_ms: u64) -> Self {
        Self {
            success: false,
            gateway_order_id: order_id.to_string(),
            status: OrderStatus::Failed,
            filled: 0.0,
            remaining: 0.0,
            avg_price: None,
            message: error.to_string(),
            latency_ms,
        }
    }
}

// ============================================================================
// Order Request（Strategy → Gateway 的输入）
// ============================================================================

/// 下单请求（Strategy 经 Execution 到 Gateway）。
#[derive(Debug, Clone)]
pub struct OrderRequest {
    /// 客户端订单 ID（调用方指定，用于去重）。
    pub client_order_id: String,
    /// 市场 ID。
    pub market_id: String,
    /// 订单方向（YES / NO）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,
    /// 下单价格。
    pub price: f64,
    /// 下单数量（份额）。
    pub quantity: f64,
    /// 策略 ID。
    pub strategy_id: String,
    /// 风控 ID。
    pub risk_id: String,
    /// 机会 ID。
    pub opportunity_id: String,
    /// 订单类型（Market / Limit，默认 Limit）。
    pub order_type: OrderType,
    /// 订单有效期（GTC / IOC / FOK，默认 GTC）。
    pub time_in_force: TimeInForce,
}

impl OrderRequest {
    /// 创建新的下单请求。
    pub fn new(
        market_id: &str,
        direction: Direction,
        side: Side,
        price: f64,
        quantity: f64,
        strategy_id: &str,
        risk_id: &str,
        opportunity_id: &str,
    ) -> Self {
        Self {
            client_order_id: format!("CLI-{}", uuid_simple()),
            market_id: market_id.to_string(),
            direction,
            side,
            price,
            quantity,
            strategy_id: strategy_id.to_string(),
            risk_id: risk_id.to_string(),
            opportunity_id: opportunity_id.to_string(),
            order_type: OrderType::Limit,
            time_in_force: TimeInForce::Gtc,
        }
    }

    /// 设置订单类型。
    pub fn with_order_type(mut self, ot: OrderType) -> Self {
        self.order_type = ot;
        self
    }

    /// 设置有效期。
    pub fn with_time_in_force(mut self, tif: TimeInForce) -> Self {
        self.time_in_force = tif;
        self
    }

    /// 设置客户端订单 ID。
    pub fn with_client_order_id(mut self, id: &str) -> Self {
        self.client_order_id = id.to_string();
        self
    }
}

/// 简单 UUID（避免额外依赖 uuid crate）。
fn uuid_simple() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let bytes: [u8; 16] = rng.random();
    format!(
        "{:08x}{:04x}{:04x}{:04x}{:08x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u32::from_be_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]),
    )
}

// ============================================================================
// 订单类型与有效期
// ============================================================================

/// 订单类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    /// 市价单。
    Market,
    /// 限价单（默认）。
    Limit,
}

impl OrderType {
    pub fn as_zh(&self) -> &'static str {
        match self {
            OrderType::Market => "市价",
            OrderType::Limit => "限价",
        }
    }
}

/// 订单有效期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
    /// 取消前有效（默认）。
    Gtc,
    /// 立即成交或取消。
    Ioc,
    /// 全部成交或取消。
    Fok,
}

impl TimeInForce {
    pub fn as_zh(&self) -> &'static str {
        match self {
            TimeInForce::Gtc => "取消前有效",
            TimeInForce::Ioc => "立即成交或取消",
            TimeInForce::Fok => "全部成交或取消",
        }
    }
}

// ============================================================================
// Balance（账户余额）
// ============================================================================

/// 账户余额。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Balance {
    /// 账户 ID。
    pub account_id: String,
    /// 可用余额（USDC）。
    pub available: f64,
    /// 总余额（USDC）。
    pub total: f64,
    /// 已占用保证金。
    pub locked: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 货币。
    pub currency: String,
    /// 更新时间。
    pub updated_at: Option<DateTime<Local>>,
}

impl Balance {
    /// 创建空余额（Mock 用）。
    pub fn mock(available: f64) -> Self {
        Self {
            account_id: "mock-account".to_string(),
            available,
            total: available,
            locked: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            currency: "USDC".to_string(),
            updated_at: Some(Local::now()),
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "账户: {} | 可用: {:.2} {} | 总额: {:.2} {} | 占用: {:.2} | 未实现盈亏: {:.2} | 已实现盈亏: {:.2}",
            self.account_id,
            self.available,
            self.currency,
            self.total,
            self.currency,
            self.locked,
            self.unrealized_pnl,
            self.realized_pnl,
        )
    }
}

// ============================================================================
// Position（持仓）
// ============================================================================

/// 持仓信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// 持仓 ID。
    pub position_id: String,
    /// 市场 ID。
    pub market_id: String,
    /// 问题描述。
    pub question: String,
    /// 方向（YES / NO）。
    pub direction: Direction,
    /// 持仓数量。
    pub quantity: f64,
    /// 平均入场价。
    pub avg_entry_price: f64,
    /// 当前标记价格。
    pub mark_price: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 持仓成本。
    pub cost_basis: f64,
    /// 当前市值。
    pub market_value: f64,
    /// 更新时间。
    pub updated_at: Option<DateTime<Local>>,
}

impl Position {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "{} | {} | {} | 数量:{:.2} | 入场:{:.4} | 标记:{:.4} | 未实现盈亏:{:.2}",
            self.position_id,
            self.market_id,
            self.direction.as_zh(),
            self.quantity,
            self.avg_entry_price,
            self.mark_price,
            self.unrealized_pnl,
        )
    }
}

// ============================================================================
// Gateway Info（网关信息）
// ============================================================================

/// Gateway 信息摘要（供 CLI 展示）。
#[derive(Debug, Clone)]
pub struct GatewayInfo {
    /// Gateway 名称。
    pub name: String,
    /// Gateway 类型。
    pub gateway_type: String,
    /// 是否启用真实交易。
    pub live_enabled: bool,
    /// 健康状态。
    pub healthy: bool,
    /// API 延迟（毫秒）。
    pub api_latency_ms: u64,
    /// HTTP 成功率。
    pub http_success_rate: f64,
    /// WebSocket 状态。
    pub ws_connected: bool,
    /// Rate Limit 剩余百分比。
    pub rate_limit_remaining: f64,
    /// 总订单数。
    pub total_orders: u64,
    /// 总成交数。
    pub total_fills: u64,
    /// 连接状态描述（中文）。
    pub connection_status: String,
}

impl GatewayInfo {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let live = if self.live_enabled {
            "⚠️ 真实交易"
        } else {
            "🔒 模拟交易"
        };
        let health = if self.healthy {
            "✅ 正常"
        } else {
            "❌ 异常"
        };
        let ws = if self.ws_connected {
            "✅ 已连接"
        } else {
            "❌ 未连接"
        };
        format!(
            "【{}】{} | {}\n\
             连接状态: {} | {}\n\
             API 延迟: {}ms | HTTP 成功率: {:.1}%\n\
             WebSocket: {} | Rate Limit 剩余: {:.0}%\n\
             订单: {} 提交 / {} 成交",
            self.name,
            self.gateway_type,
            live,
            health,
            self.connection_status,
            self.api_latency_ms,
            self.http_success_rate * 100.0,
            ws,
            self.rate_limit_remaining,
            self.total_orders,
            self.total_fills,
        )
    }
}

// ============================================================================
// Market（市场信息）
// ============================================================================

/// 市场信息（P2-03）。
#[derive(Debug, Clone)]
pub struct Market {
    /// 市场 ID（condition_id）。
    pub market_id: String,
    /// 问题文本。
    pub question: String,
    /// 是否已关闭。
    pub closed: bool,
    /// YES 价格。
    pub yes_price: Option<f64>,
    /// NO 价格。
    pub no_price: Option<f64>,
    /// 成交量。
    pub volume: f64,
    /// 流动性。
    pub liquidity: f64,
    /// 市场状态（中文）。
    pub status: String,
}

impl Market {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let yes = self
            .yes_price
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "-".into());
        let no = self
            .no_price
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "-".into());
        format!(
            "{} | YES: {} | NO: {} | 成交量: {:.0} | 状态: {}",
            self.question, yes, no, self.volume, self.status,
        )
    }
}

// ============================================================================
// OrderBook（订单簿）
// ============================================================================

/// 订单簿信息（P2-03）。
#[derive(Debug, Clone)]
pub struct OrderBook {
    /// 市场 ID。
    pub market_id: String,
    /// 买盘（价格、数量）- 按价格从高到低排序。
    pub bids: Vec<BookLevel>,
    /// 卖盘（价格、数量）- 按价格从低到高排序。
    pub asks: Vec<BookLevel>,
    /// 最小报价单位。
    pub tick_size: f64,
    /// 更新时间。
    pub updated_at: Option<chrono::DateTime<chrono::Local>>,
}

/// 订单簿档位。
#[derive(Debug, Clone)]
pub struct BookLevel {
    /// 价格（0.0 ~ 1.0）。
    pub price: f64,
    /// 数量（份额）。
    pub size: f64,
}

impl OrderBook {
    /// 最佳买价。
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|b| b.price)
    }

    /// 最佳卖价。
    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|a| a.price)
    }

    /// 价差。
    pub fn spread(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let bid = self
            .best_bid()
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "-".into());
        let ask = self
            .best_ask()
            .map(|p| format!("{:.4}", p))
            .unwrap_or_else(|| "-".into());
        let spread = self
            .spread()
            .map(|s| format!("{:.4}", s))
            .unwrap_or_else(|| "-".into());
        format!(
            "市场: {} | Best Bid: {} | Best Ask: {} | 价差: {} | 买盘: {}档 | 卖盘: {}档",
            self.market_id,
            bid,
            ask,
            spread,
            self.bids.len(),
            self.asks.len(),
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_result_factories() {
        let r = GatewayResult::accepted("EX-001", "已接受", 10);
        assert!(r.success);
        assert_eq!(r.status, OrderStatus::Accepted);

        let r = GatewayResult::rejected("EX-002", "资金不足", 5);
        assert!(!r.success);
        assert_eq!(r.status, OrderStatus::Rejected);

        let r = GatewayResult::filled("EX-003", 100.0, 0.452, 15);
        assert!(r.success);
        assert_eq!(r.status, OrderStatus::Filled);
        assert!((r.avg_price.unwrap() - 0.452).abs() < 1e-9);

        let r = GatewayResult::cancelled("EX-004", "用户取消", 8);
        assert!(r.success);
        assert_eq!(r.status, OrderStatus::Cancelled);

        let r = GatewayResult::partially_filled("EX-005", 50.0, 50.0, 0.45, 12);
        assert!(r.success);
        assert_eq!(r.status, OrderStatus::PartiallyFilled);
        assert!((r.remaining - 50.0).abs() < 1e-9);
    }

    #[test]
    fn order_request_builder() {
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
        assert_eq!(req.market_id, "mkt-1");
        assert_eq!(req.order_type, OrderType::Limit);
        assert_eq!(req.time_in_force, TimeInForce::Gtc);

        let req = req
            .with_order_type(OrderType::Market)
            .with_time_in_force(TimeInForce::Ioc);
        assert_eq!(req.order_type, OrderType::Market);
        assert_eq!(req.time_in_force, TimeInForce::Ioc);
    }

    #[test]
    fn balance_summary_zh() {
        let b = Balance::mock(10000.0);
        let summary = b.summary_zh();
        assert!(summary.contains("10000.00"));
        assert!(summary.contains("USDC"));
    }

    #[test]
    fn position_summary_zh() {
        let p = Position {
            position_id: "POS-001".into(),
            market_id: "mkt-1".into(),
            question: "测试".into(),
            direction: Direction::Yes,
            quantity: 100.0,
            avg_entry_price: 0.45,
            mark_price: 0.50,
            unrealized_pnl: 5.0,
            realized_pnl: 0.0,
            cost_basis: 45.0,
            market_value: 50.0,
            updated_at: None,
        };
        let summary = p.summary_zh();
        assert!(summary.contains("POS-001"));
        assert!(summary.contains("YES"));
    }
}
