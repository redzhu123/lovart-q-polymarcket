//! 统一订单模型（V1.06 第三节）。
//!
//! 定义 Execution Engine 的唯一订单类型，包含完整的生命周期字段。
//! 所有订单必须经过 Execution Pipeline，禁止 Strategy / Risk 直接下单。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不下单。

use chrono::{DateTime, Local};
use pm_core::Side;
use serde::{Deserialize, Serialize};

/// 订单方向（Polymarket 特有：YES / NO）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// YES 方向（赌"是"）。
    Yes,
    /// NO 方向（赌"否"）。
    No,
}

impl Direction {
    pub fn as_zh(&self) -> &'static str {
        match self {
            Direction::Yes => "YES",
            Direction::No => "NO",
        }
    }
}

/// 订单生命周期状态（V1.06 第四节）。
///
/// 完整生命周期：
/// ```text
/// Created → Validated → Queued → Submitted → Accepted
///                                              ↓
///                            PartiallyFilled ← ┘
///                               ↓         ↘
///                            Filled     Cancelled
///
///   任意非终态可进入：Rejected / Failed / Expired
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OrderStatus {
    /// 已创建（初始状态）。
    Created,
    /// 已校验（通过 Validator）。
    Validated,
    /// 已入队（在 Queue 中等待）。
    Queued,
    /// 已提交（已发送到 Gateway）。
    Submitted,
    /// 已接受（Gateway 确认接收）。
    Accepted,
    /// 部分成交。
    PartiallyFilled,
    /// 完全成交（终态）。
    Filled,
    /// 已取消（终态）。
    Cancelled,
    /// 已过期（终态）。
    Expired,
    /// 已拒绝（终态）。
    Rejected,
    /// 失败（终态）。
    Failed,
}

impl OrderStatus {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            OrderStatus::Created => "已创建",
            OrderStatus::Validated => "已校验",
            OrderStatus::Queued => "已入队",
            OrderStatus::Submitted => "已提交",
            OrderStatus::Accepted => "已接受",
            OrderStatus::PartiallyFilled => "部分成交",
            OrderStatus::Filled => "完全成交",
            OrderStatus::Cancelled => "已取消",
            OrderStatus::Expired => "已过期",
            OrderStatus::Rejected => "已拒绝",
            OrderStatus::Failed => "失败",
        }
    }

    /// 英文标识符（用于 CSV / 日志 key）。
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Created => "Created",
            OrderStatus::Validated => "Validated",
            OrderStatus::Queued => "Queued",
            OrderStatus::Submitted => "Submitted",
            OrderStatus::Accepted => "Accepted",
            OrderStatus::PartiallyFilled => "PartiallyFilled",
            OrderStatus::Filled => "Filled",
            OrderStatus::Cancelled => "Cancelled",
            OrderStatus::Expired => "Expired",
            OrderStatus::Rejected => "Rejected",
            OrderStatus::Failed => "Failed",
        }
    }

    /// 是否为终态（不再变化）。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled
                | OrderStatus::Cancelled
                | OrderStatus::Expired
                | OrderStatus::Rejected
                | OrderStatus::Failed
        )
    }

    /// 是否为活跃状态（非终态）。
    pub fn is_active(&self) -> bool {
        !self.is_terminal()
    }
}

/// 状态变化记录（V1.06 第四节：记录所有状态变化，支持 Replay）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusChange {
    /// 原状态。
    pub from: OrderStatus,
    /// 新状态。
    pub to: OrderStatus,
    /// 变化时间。
    pub timestamp: DateTime<Local>,
    /// 变化原因（中文）。
    pub reason: String,
}

impl StatusChange {
    pub fn new(
        from: OrderStatus,
        to: OrderStatus,
        reason: &str,
        timestamp: DateTime<Local>,
    ) -> Self {
        Self {
            from,
            to,
            timestamp,
            reason: reason.to_string(),
        }
    }

    /// 人类可读的中文描述。
    pub fn description(&self) -> String {
        format!(
            "[{}] {} → {}：{}",
            self.timestamp.format("%H:%M:%S"),
            self.from.as_zh(),
            self.to.as_zh(),
            self.reason
        )
    }
}

/// 统一订单模型（V1.06 第三节）。
///
/// 这是 Execution Engine 的唯一订单类型。所有字段均为订单全生命周期数据。
/// Strategy / Risk 禁止持有或修改 Order — 只有 Execution Pipeline 可以。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// 系统订单 ID（Execution 分配，如 "EX-000001"）。
    pub order_id: String,
    /// 客户端订单 ID（调用方指定，用于去重和关联）。
    pub client_order_id: String,
    /// 市场 ID。
    pub market_id: String,
    /// 数据源 Provider（"gamma" | "clob" | "mock"）。
    pub provider: String,
    /// 订单方向（YES / NO）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,
    /// 下单价格。
    pub price: f64,
    /// 下单数量（份额）。
    pub quantity: f64,
    /// 已成交数量。
    pub filled: f64,
    /// 剩余未成交数量。
    pub remaining: f64,
    /// 订单状态。
    pub status: OrderStatus,
    /// 创建时间。
    pub create_time: DateTime<Local>,
    /// 最后更新时间。
    pub update_time: DateTime<Local>,
    /// 策略 ID（谁发起的）。
    pub strategy_id: String,
    /// 风控 ID（哪个 Risk 决策批准的）。
    pub risk_id: String,
    /// 机会 ID（关联的套利机会）。
    pub opportunity_id: String,
    /// Execution 运行 ID（哪次 Pipeline 运行）。
    pub execution_id: String,
    /// 版本号（用于乐观锁 / 并发控制）。
    pub version: u32,
    /// 重试次数。
    pub retry_count: u32,
    /// 优先级（越大越优先出队）。
    pub priority: u32,
    /// 加权平均成交价。
    pub avg_fill_price: f64,
    /// 综合滑点（小数形式）。
    pub slippage: f64,
    /// 状态变化历史（支持 Replay）。
    pub status_history: Vec<StatusChange>,
    /// 永远为 true：标记本订单仅为模拟。
    pub simulation_only: bool,
}

impl Order {
    /// 创建新订单（初始状态 Created）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        order_id: String,
        client_order_id: String,
        market_id: String,
        provider: String,
        direction: Direction,
        side: Side,
        price: f64,
        quantity: f64,
        strategy_id: String,
        risk_id: String,
        opportunity_id: String,
        now: DateTime<Local>,
    ) -> Self {
        let mut order = Self {
            order_id,
            client_order_id,
            market_id,
            provider,
            direction,
            side,
            price,
            quantity,
            filled: 0.0,
            remaining: quantity,
            status: OrderStatus::Created,
            create_time: now,
            update_time: now,
            strategy_id,
            risk_id,
            opportunity_id,
            execution_id: String::new(),
            version: 1,
            retry_count: 0,
            priority: 0,
            avg_fill_price: 0.0,
            slippage: 0.0,
            status_history: Vec::new(),
            simulation_only: true,
        };
        order.record_status_change(OrderStatus::Created, "订单创建");
        order
    }

    /// 转换状态并记录历史。
    pub fn transition(&mut self, new_status: OrderStatus, reason: &str, now: DateTime<Local>) {
        let old = self.status;
        self.status = new_status;
        self.update_time = now;
        self.version += 1;
        self.record_status_change(new_status, reason);
        tracing::info!(
            order_id = %self.order_id,
            from = %old.as_zh(),
            to = %new_status.as_zh(),
            reason = %reason,
            "订单状态变化"
        );
    }

    /// 记录一次状态变化。
    fn record_status_change(&mut self, to: OrderStatus, reason: &str) {
        self.status_history
            .push(StatusChange::new(self.status, to, reason, self.update_time));
    }

    /// 更新成交信息。
    pub fn update_fill(&mut self, filled: f64, avg_price: f64, slippage: f64) {
        self.filled = filled;
        self.remaining = (self.quantity - filled).max(0.0);
        self.avg_fill_price = avg_price;
        self.slippage = slippage;
    }

    /// 成交率 = filled / quantity。
    pub fn fill_rate(&self) -> f64 {
        if self.quantity > f64::EPSILON {
            self.filled / self.quantity
        } else {
            0.0
        }
    }

    /// 订单名义金额 = price * quantity。
    pub fn notional(&self) -> f64 {
        self.price * self.quantity
    }

    /// 已成交金额 = avg_fill_price * filled（若无成交价则用 price）。
    pub fn filled_notional(&self) -> f64 {
        let price = if self.avg_fill_price > 0.0 {
            self.avg_fill_price
        } else {
            self.price
        };
        price * self.filled
    }

    /// 打印订单生命周期时间线（中文）。
    pub fn print_timeline(&self) {
        println!("订单 {} 生命周期：", self.order_id);
        println!(
            "  市场: {}  方向: {}  {}  @ {:.4} × {:.2}",
            self.market_id,
            self.direction.as_zh(),
            self.side.as_str(),
            self.price,
            self.quantity
        );
        println!(
            "  状态: {}  成交: {:.2}/{:.2}  滑点: {:.2}%",
            self.status.as_zh(),
            self.filled,
            self.quantity,
            self.slippage * 100.0
        );
        println!(
            "  策略: {}  风控: {}  机会: {}",
            self.strategy_id, self.risk_id, self.opportunity_id
        );
        println!();
        println!("  状态变化历史：");
        for change in &self.status_history {
            println!("    {}", change.description());
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn test_order() -> Order {
        let now = Local::now();
        Order::new(
            "EX-000001".into(),
            "CLI-001".into(),
            "mkt-abc".into(),
            "mock".into(),
            Direction::Yes,
            Side::Buy,
            0.45,
            222.22,
            "DefaultStrategy".into(),
            "RISK-001".into(),
            "OPP-001".into(),
            now,
        )
    }

    #[test]
    fn order_creation() {
        let o = test_order();
        assert_eq!(o.status, OrderStatus::Created);
        assert_eq!(o.filled, 0.0);
        assert!((o.remaining - o.quantity).abs() < 1e-9);
        assert!(o.simulation_only);
        assert_eq!(o.status_history.len(), 1);
        assert_eq!(o.version, 1);
    }

    #[test]
    fn order_transitions() {
        let now = Local::now();
        let mut o = test_order();

        o.transition(OrderStatus::Validated, "校验通过", now);
        assert_eq!(o.status, OrderStatus::Validated);
        assert_eq!(o.status_history.len(), 2);
        assert_eq!(o.version, 2);

        o.transition(OrderStatus::Queued, "入队等待", now);
        assert_eq!(o.status, OrderStatus::Queued);
        assert_eq!(o.status_history.len(), 3);

        o.transition(OrderStatus::Rejected, "资金不足", now);
        assert!(o.status.is_terminal());
    }

    #[test]
    fn status_terminal_detection() {
        assert!(!OrderStatus::Created.is_terminal());
        assert!(!OrderStatus::Validated.is_terminal());
        assert!(!OrderStatus::Queued.is_terminal());
        assert!(!OrderStatus::Submitted.is_terminal());
        assert!(!OrderStatus::Accepted.is_terminal());
        assert!(!OrderStatus::PartiallyFilled.is_terminal());
        assert!(OrderStatus::Filled.is_terminal());
        assert!(OrderStatus::Cancelled.is_terminal());
        assert!(OrderStatus::Expired.is_terminal());
        assert!(OrderStatus::Rejected.is_terminal());
        assert!(OrderStatus::Failed.is_terminal());
    }

    #[test]
    fn order_fill_math() {
        let mut o = test_order();
        let now = Local::now();

        o.transition(OrderStatus::Validated, "校验通过", now);
        o.transition(OrderStatus::Queued, "入队", now);
        o.transition(OrderStatus::Submitted, "提交到 Gateway", now);
        o.transition(OrderStatus::Accepted, "Gateway 接受", now);
        o.transition(OrderStatus::PartiallyFilled, "部分成交", now);
        o.update_fill(100.0, 0.452, 0.005);
        assert!((o.filled - 100.0).abs() < 1e-9);
        assert!((o.remaining - 122.22).abs() < 0.01);
        assert!((o.fill_rate() - 0.45).abs() < 0.01);

        o.transition(OrderStatus::Filled, "完全成交", now);
        assert!(o.status.is_terminal());
    }

    #[test]
    fn direction_as_zh() {
        assert_eq!(Direction::Yes.as_zh(), "YES");
        assert_eq!(Direction::No.as_zh(), "NO");
    }

    #[test]
    fn status_as_zh_all_variants() {
        // 确保每个变体有中文名
        let statuses = [
            OrderStatus::Created,
            OrderStatus::Validated,
            OrderStatus::Queued,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::PartiallyFilled,
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Expired,
            OrderStatus::Rejected,
            OrderStatus::Failed,
        ];
        for s in &statuses {
            let zh = s.as_zh();
            assert!(!zh.is_empty(), "Status {:?} missing Chinese name", s);
        }
    }

    #[test]
    fn notional_calculation() {
        let o = test_order();
        assert!((o.notional() - 100.0).abs() < 0.01); // 0.45 * 222.22 ≈ 100
    }
}
