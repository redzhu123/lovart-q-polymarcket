//! 统一 Order 领域模型（P2-04 第二节）。
//!
//! OMS 内部唯一订单类型。所有市场、所有交易所都使用本类型。
//!
//! # 职责边界
//!
//! - **业务层禁止直接使用交易所订单结构**（如 `pm_gateway::OrderRequest`）。
//! - 所有 OMS API 入参/出参均为 `Domain::Order`。
//! - Gateway 仅作为底层投递通道，OMS 负责所有业务状态管理。
//!
//! # 字段分层
//!
//! - 基础字段：`order_id` / `client_order_id` / `market_id` / `side` / `direction` /
//!   `order_type` / `quantity` / `price` / `time_in_force`
//! - 状态字段：`status` / `previous_status` / `filled` / `remaining` / `avg_fill_price`
//! - 时间字段：`created_at` / `updated_at`
//! - 来源字段：`strategy_id` / `risk_id` / `opportunity_id`
//! - 网关字段：`exchange_order_id` / `gateway_name`
//! - 历史字段：`status_history` / `events`
//! - 元字段：`version` / `retry_count` / `simulation_only` / `notes`
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名。

use chrono::{DateTime, Local};
use pm_core::Side;
pub use pm_execution::order::Direction;
use pm_gateway::{OrderType, TimeInForce};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ============================================================================
// OMS Order Status — 12 态完整生命周期
// ============================================================================

/// OMS 订单状态（P2-04 第三节）。
///
/// 完整生命周期：
/// ```text
/// Created → Validated → PendingSubmit → Submitted → Accepted
///                                                  ↓
///                                PartiallyFilled ← ┘
///                                   ↓       ↘
///                                Filled    Cancelled
///
/// 任意非终态可进入：Rejected / Expired
/// Completed：Filled/Cancelled/Rejected/Expired 任意终态聚合（用于统计）。
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    /// 已创建（初始）。
    Created,
    /// 已校验（Validator 通过）。
    Validated,
    /// 待提交（OMS 内部已决策，尚未调用 Gateway）。
    PendingSubmit,
    /// 已提交（已发往 Gateway）。
    Submitted,
    /// 已接受（Gateway 接受）。
    Accepted,
    /// 部分成交。
    PartiallyFilled,
    /// 完全成交（终态）。
    Filled,
    /// 已取消（终态）。
    Cancelled,
    /// 已拒绝（终态）。
    Rejected,
    /// 已过期（终态）。
    Expired,
    /// 完成（聚合终态：Filled / Cancelled / Rejected / Expired 之一，统计用）。
    Completed,
}

impl OrderStatus {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            OrderStatus::Created => "已创建",
            OrderStatus::Validated => "已校验",
            OrderStatus::PendingSubmit => "待提交",
            OrderStatus::Submitted => "已提交",
            OrderStatus::Accepted => "已接受",
            OrderStatus::PartiallyFilled => "部分成交",
            OrderStatus::Filled => "完全成交",
            OrderStatus::Cancelled => "已取消",
            OrderStatus::Rejected => "已拒绝",
            OrderStatus::Expired => "已过期",
            OrderStatus::Completed => "已完成",
        }
    }

    /// 英文标识符（CSV / 日志 key）。
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Created => "Created",
            OrderStatus::Validated => "Validated",
            OrderStatus::PendingSubmit => "PendingSubmit",
            OrderStatus::Submitted => "Submitted",
            OrderStatus::Accepted => "Accepted",
            OrderStatus::PartiallyFilled => "PartiallyFilled",
            OrderStatus::Filled => "Filled",
            OrderStatus::Cancelled => "Cancelled",
            OrderStatus::Rejected => "Rejected",
            OrderStatus::Expired => "Expired",
            OrderStatus::Completed => "Completed",
        }
    }

    /// 是否为真实终态（不再变化，且不会进入 Completed）。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled
                | OrderStatus::Cancelled
                | OrderStatus::Rejected
                | OrderStatus::Expired
        )
    }

    /// 是否活跃（非终态 + 非聚合）。
    pub fn is_active(&self) -> bool {
        !self.is_terminal() && *self != OrderStatus::Completed
    }

    /// 是否为成功终态（Filled / Completed）。
    pub fn is_success(&self) -> bool {
        matches!(self, OrderStatus::Filled | OrderStatus::Completed)
    }

    /// 从 pm_execution::order::OrderStatus 转换（Gateway 适配用）。
    pub fn from_execution(s: pm_execution::order::OrderStatus) -> Self {
        use pm_execution::order::OrderStatus as E;
        match s {
            E::Created => OrderStatus::Created,
            E::Validated => OrderStatus::Validated,
            E::Queued => OrderStatus::PendingSubmit,
            E::Submitted => OrderStatus::Submitted,
            E::Accepted => OrderStatus::Accepted,
            E::PartiallyFilled => OrderStatus::PartiallyFilled,
            E::Filled => OrderStatus::Filled,
            E::Cancelled => OrderStatus::Cancelled,
            E::Rejected => OrderStatus::Rejected,
            E::Expired => OrderStatus::Expired,
            E::Failed => OrderStatus::Rejected,
        }
    }
}

// ============================================================================
// StatusChange — 状态变化记录（用于审计 + Replay）
// ============================================================================

/// 单次状态变化记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusChange {
    /// 原状态。
    pub from: OrderStatus,
    /// 新状态。
    pub to: OrderStatus,
    /// 时间戳。
    pub at: DateTime<Local>,
    /// 中文原因。
    pub reason: String,
    /// 触发者（"validator" | "oms" | "gateway" | "recovery"）。
    pub actor: String,
}

impl StatusChange {
    pub fn new(
        from: OrderStatus,
        to: OrderStatus,
        reason: &str,
        actor: &str,
        at: DateTime<Local>,
    ) -> Self {
        Self {
            from,
            to,
            at,
            reason: reason.to_string(),
            actor: actor.to_string(),
        }
    }

    /// 中文描述（用于日志 / 时间线）。
    pub fn description(&self) -> String {
        format!(
            "[{}] {} → {}（{}）：{}",
            self.at.format("%H:%M:%S"),
            self.from.as_zh(),
            self.to.as_zh(),
            self.actor,
            self.reason,
        )
    }
}

// ============================================================================
// Domain::Order — OMS 统一订单
// ============================================================================

// ============================================================================
// OrderType / TimeInForce serde 适配（pm_gateway 不提供 serde 实现）
// ============================================================================

mod order_type_serde {
    use super::OrderType;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &OrderType, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(v.as_zh())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<OrderType, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "市价" => OrderType::Market,
            _ => OrderType::Limit,
        })
    }
}

mod tif_serde {
    use super::TimeInForce;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &TimeInForce, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(v.as_zh())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<TimeInForce, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "立即成交或取消" => TimeInForce::Ioc,
            "全部成交或取消" => TimeInForce::Fok,
            _ => TimeInForce::Gtc,
        })
    }
}

/// OMS 统一订单模型（P2-04 第二节）。
///
/// 业务层唯一订单类型。所有 OMS API（create_order / submit_order / cancel_order 等）
/// 的入参和返回都使用本类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    // ---- 基础标识 ----
    /// OMS 订单 ID（系统分配，格式 `OMS-YYYYMMDD-NNNNNN`）。
    pub order_id: String,
    /// 客户端订单 ID（调用方指定，用于幂等 / 去重）。
    pub client_order_id: String,
    /// 交易所订单 ID（Gateway 分配，可选 — 未提交前为空）。
    pub exchange_order_id: Option<String>,
    /// Gateway 名称（如 "MockGateway" / "PolymarketGateway"）。
    pub gateway_name: String,

    // ---- 市场与方向 ----
    /// 市场 ID。
    pub market_id: String,
    /// Provider 类型（"gamma" | "clob" | "mock"）。
    pub provider: String,
    /// 方向（YES / NO，Polymarket 特有）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,

    // ---- 订单参数 ----
    /// 下单价格。
    pub price: f64,
    /// 下单数量（份额）。
    pub quantity: f64,
    /// 订单类型（Market / Limit）。
    #[serde(with = "order_type_serde")]
    pub order_type: OrderType,
    /// 订单有效期（GTC / IOC / FOK）。
    #[serde(with = "tif_serde")]
    pub time_in_force: TimeInForce,

    // ---- 状态字段 ----
    /// 当前状态。
    pub status: OrderStatus,
    /// 已成交数量。
    pub filled: f64,
    /// 剩余未成交数量。
    pub remaining: f64,
    /// 加权平均成交价。
    pub avg_fill_price: f64,
    /// 滑点（小数形式，0.01 = 1%）。
    pub slippage: f64,

    // ---- 时间字段 ----
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 最后更新时间。
    pub updated_at: DateTime<Local>,

    // ---- 来源字段（业务可追溯）----
    /// 策略 ID（谁发起的）。
    pub strategy_id: String,
    /// 风控 ID（哪个 Risk 决策批准的）。
    pub risk_id: String,
    /// 机会 ID（关联的套利机会）。
    pub opportunity_id: String,

    // ---- 历史 / 事件 ----
    /// 状态变化历史（用于 Replay）。
    pub status_history: Vec<StatusChange>,

    // ---- 元字段 ----
    /// 版本号（乐观锁）。
    pub version: u32,
    /// 重试次数。
    pub retry_count: u32,
    /// 优先级（越大越优先提交）。
    pub priority: u32,
    /// 备注 / 标签。
    pub notes: String,
    /// 是否模拟（永远 true：OMS 不连接真实交易）。
    pub simulation_only: bool,
}

impl Order {
    /// 创建新订单（初始状态 Created）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client_order_id: String,
        market_id: String,
        provider: String,
        gateway_name: String,
        direction: Direction,
        side: Side,
        price: f64,
        quantity: f64,
        order_type: OrderType,
        time_in_force: TimeInForce,
        strategy_id: String,
        risk_id: String,
        opportunity_id: String,
        now: DateTime<Local>,
    ) -> Self {
        let mut order = Self {
            order_id: format!("OMS-{}-{}", now.format("%Y%m%d"), next_seq_id()),
            client_order_id,
            exchange_order_id: None,
            gateway_name,
            market_id,
            provider,
            direction,
            side,
            price,
            quantity,
            order_type,
            time_in_force,
            status: OrderStatus::Created,
            filled: 0.0,
            remaining: quantity,
            avg_fill_price: 0.0,
            slippage: 0.0,
            created_at: now,
            updated_at: now,
            strategy_id,
            risk_id,
            opportunity_id,
            status_history: Vec::new(),
            version: 1,
            retry_count: 0,
            priority: 0,
            notes: String::new(),
            simulation_only: true,
        };
        order.record_change(OrderStatus::Created, "订单创建", "oms", now);
        order
    }

    /// 转换状态并记录历史。
    pub fn transition(
        &mut self,
        new_status: OrderStatus,
        reason: &str,
        actor: &str,
        now: DateTime<Local>,
    ) {
        let old = self.status;
        self.status = new_status;
        self.updated_at = now;
        self.version += 1;
        self.record_change(new_status, reason, actor, now);
        tracing::info!(
            order_id = %self.order_id,
            client_order_id = %self.client_order_id,
            market_id = %self.market_id,
            from = %old.as_zh(),
            to = %new_status.as_zh(),
            reason = %reason,
            actor = %actor,
            "OMS 订单状态变化"
        );
    }

    /// 记录一次状态变化（不进 tracing.info，仅写历史）。
    fn record_change(&mut self, to: OrderStatus, reason: &str, actor: &str, at: DateTime<Local>) {
        self.status_history
            .push(StatusChange::new(self.status, to, reason, actor, at));
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

    /// 已成交金额。
    pub fn filled_notional(&self) -> f64 {
        let p = if self.avg_fill_price > 0.0 {
            self.avg_fill_price
        } else {
            self.price
        };
        p * self.filled
    }

    /// 设置优先级。
    pub fn with_priority(mut self, p: u32) -> Self {
        self.priority = p;
        self
    }

    /// 设置备注。
    pub fn with_notes(mut self, notes: &str) -> Self {
        self.notes = notes.to_string();
        self
    }

    /// 设置 ExchangeOrderId（Gateway 回填时使用）。
    pub fn set_exchange_order_id(&mut self, id: &str) {
        self.exchange_order_id = Some(id.to_string());
    }

    /// 重试次数 +1。
    pub fn bump_retry(&mut self) {
        self.retry_count += 1;
    }

    /// 打印订单生命周期时间线（中文，用于 CLI `order <id>`）。
    pub fn print_timeline(&self) {
        println!("【订单 {}】", self.order_id);
        println!("  客户端订单 ID : {}", self.client_order_id);
        if let Some(ref eid) = self.exchange_order_id {
            println!("  交易所订单 ID : {}", eid);
        } else {
            println!("  交易所订单 ID : （尚未分配）");
        }
        println!("  Gateway       : {}", self.gateway_name);
        println!("  市场          : {} ({})", self.market_id, self.provider);
        println!(
            "  方向          : {} {}",
            self.direction.as_zh(),
            self.side.as_str()
        );
        println!(
            "  订单类型      : {} ({})",
            self.order_type.as_zh(),
            self.time_in_force.as_zh()
        );
        println!(
            "  报价 / 数量   : {:.4} × {:.2}  名义金额 {:.2}",
            self.price,
            self.quantity,
            self.notional()
        );
        println!(
            "  状态          : {}（{}/{}，成交率 {:.1}%）",
            self.status.as_zh(),
            self.filled,
            self.quantity,
            self.fill_rate() * 100.0
        );
        if self.avg_fill_price > 0.0 {
            println!(
                "  加权均价 / 滑点: {:.4} / {:.2}%",
                self.avg_fill_price,
                self.slippage * 100.0
            );
        }
        println!(
            "  来源          : 策略 {} | 风控 {} | 机会 {}",
            self.strategy_id, self.risk_id, self.opportunity_id
        );
        println!(
            "  创建 / 更新   : {} / {}",
            self.created_at.format("%Y-%m-%d %H:%M:%S"),
            self.updated_at.format("%Y-%m-%d %H:%M:%S")
        );
        println!("  版本 / 重试   : {} / {}", self.version, self.retry_count);
        if !self.notes.is_empty() {
            println!("  备注          : {}", self.notes);
        }
        if !self.simulation_only {
            println!("  ⚠️ 警告：simulation_only=false，疑似真实订单");
        }
        println!();
        println!("  状态变化历史：");
        if self.status_history.is_empty() {
            println!("    （无）");
        } else {
            for c in &self.status_history {
                println!("    {}", c.description());
            }
        }
        println!();
    }
}

// ============================================================================
// ID 生成（避免引入 uuid crate）
// ============================================================================

use std::sync::atomic::{AtomicU64, Ordering};
static OMS_SEQ: AtomicU64 = AtomicU64::new(0);

/// 自增序列号（用于 OMS 订单 ID）。全局单调递增。
fn next_seq_id() -> String {
    let n = OMS_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    format!("{:06}", n)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use pm_gateway::{OrderType, TimeInForce};

    fn test_order() -> Order {
        let now = Local::now();
        Order::new(
            "CLI-001".into(),
            "mkt-abc".into(),
            "mock".into(),
            "MockGateway".into(),
            Direction::Yes,
            Side::Buy,
            0.45,
            100.0,
            OrderType::Limit,
            TimeInForce::Gtc,
            "S1".into(),
            "R1".into(),
            "O1".into(),
            now,
        )
    }

    #[test]
    fn order_creation_defaults() {
        let o = test_order();
        assert_eq!(o.status, OrderStatus::Created);
        assert_eq!(o.filled, 0.0);
        assert!((o.remaining - 100.0).abs() < 1e-9);
        assert!(o.simulation_only);
        assert_eq!(o.status_history.len(), 1);
        assert!(o.order_id.starts_with("OMS-"));
        assert_eq!(o.version, 1);
    }

    #[test]
    fn transition_records_history() {
        let now = Local::now();
        let mut o = test_order();
        o.transition(OrderStatus::Validated, "校验通过", "validator", now);
        o.transition(OrderStatus::PendingSubmit, "等待提交", "oms", now);
        o.transition(OrderStatus::Submitted, "已提交", "oms", now);
        assert_eq!(o.status, OrderStatus::Submitted);
        assert_eq!(o.status_history.len(), 4);
        assert_eq!(o.version, 4);
    }

    #[test]
    fn status_chinese_names_unique() {
        let all = [
            OrderStatus::Created,
            OrderStatus::Validated,
            OrderStatus::PendingSubmit,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::PartiallyFilled,
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
            OrderStatus::Expired,
            OrderStatus::Completed,
        ];
        for s in &all {
            assert!(!s.as_zh().is_empty(), "{:?} 缺少中文名", s);
            assert!(!s.as_str().is_empty(), "{:?} 缺少英文标识", s);
        }
    }

    #[test]
    fn status_terminal_active_classification() {
        assert!(!OrderStatus::Created.is_terminal());
        assert!(!OrderStatus::Validated.is_terminal());
        assert!(OrderStatus::Filled.is_terminal());
        assert!(OrderStatus::Cancelled.is_terminal());
        assert!(OrderStatus::Rejected.is_terminal());
        assert!(OrderStatus::Expired.is_terminal());
        assert!(!OrderStatus::Completed.is_terminal()); // 聚合态
        assert!(OrderStatus::Created.is_active());
        assert!(!OrderStatus::Filled.is_active());
    }

    #[test]
    fn fill_math() {
        let mut o = test_order();
        o.update_fill(40.0, 0.452, 0.005);
        assert!((o.filled - 40.0).abs() < 1e-9);
        assert!((o.remaining - 60.0).abs() < 1e-9);
        assert!((o.avg_fill_price - 0.452).abs() < 1e-9);
        assert!((o.fill_rate() - 0.4).abs() < 1e-9);
    }

    #[test]
    fn notional_and_filled_notional() {
        let mut o = test_order();
        assert!((o.notional() - 45.0).abs() < 1e-9); // 0.45 * 100
        o.update_fill(50.0, 0.46, 0.0);
        assert!((o.filled_notional() - 23.0).abs() < 1e-9); // 0.46 * 50
    }

    #[test]
    fn exchange_order_id_setter() {
        let mut o = test_order();
        o.set_exchange_order_id("GW-12345");
        assert_eq!(o.exchange_order_id.as_deref(), Some("GW-12345"));
    }

    #[test]
    fn sequence_id_monotonic() {
        let _a = next_seq_id();
        let b = next_seq_id();
        let c = next_seq_id();
        // 不保证严格连续（并发 fetch_add），但保证递增
        assert!(b.parse::<u64>().unwrap() >= 1);
        assert!(c.parse::<u64>().unwrap() >= b.parse::<u64>().unwrap());
    }

    #[test]
    fn builder_helpers() {
        let o = test_order().with_priority(5).with_notes("测试备注");
        assert_eq!(o.priority, 5);
        assert_eq!(o.notes, "测试备注");
    }

    #[test]
    fn print_timeline_smoke_test() {
        // 仅验证不 panic；println 由 #[ignore] 友好排除
        let o = test_order();
        o.print_timeline();
    }
}
