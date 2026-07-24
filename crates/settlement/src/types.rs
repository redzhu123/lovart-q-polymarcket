//! Settlement Engine 统一领域类型（P2-06）。
//!
//! 所有成交结算相关类型统一定义于此。
//!
//! # 类型清单
//!
//! - [`TradeFillEvent`]：成交事件（输入）
//! - [`SettlementResult`]：结算结果（输出）
//! - [`LedgerEntry`]：资金流水（追加不可修改）
//! - [`FeeBreakdown`]：手续费明细
//! - [`FeeRule`]：手续费规则
//! - [`PositionState`]：持仓状态（Settlement 内部）
//! - [`BalanceState`]：余额状态（Settlement 内部）
//! - [`SettlementStatus`]：结算状态
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use pm_core::Side;
pub use pm_execution::order::Direction;
use serde::{Deserialize, Serialize};

// ============================================================================
// TradeFillEvent — 成交事件（输入）
// ============================================================================

/// 成交事件。
///
/// Gateway 或 Exchange 回报的成交信息。
/// Settlement Engine 接收此事件作为唯一输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeFillEvent {
    /// 成交 ID（交易所分配）。
    pub trade_id: String,
    /// 关联订单 ID。
    pub order_id: String,
    /// 客户端订单 ID。
    pub client_order_id: String,
    /// 交易所订单 ID。
    pub exchange_order_id: Option<String>,
    /// 市场 ID。
    pub market_id: String,
    /// 账户 ID。
    pub account_id: String,
    /// 方向（YES/NO）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,
    /// 成交价格。
    pub fill_price: f64,
    /// 成交数量。
    pub fill_quantity: f64,
    /// 成交时间。
    pub filled_at: DateTime<Local>,
    /// 是否为 Taker（主动成交）。
    pub is_taker: bool,
    /// Gateway 名称。
    pub gateway_name: String,
}

impl TradeFillEvent {
    /// 成交金额 = fill_price * fill_quantity。
    pub fn fill_notional(&self) -> f64 {
        self.fill_price * self.fill_quantity
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "成交 {} | 订单 {} | {} {} @ {:.4} × {:.2} | 金额 {:.2} USDC | {}",
            self.trade_id,
            self.order_id,
            self.direction.as_zh(),
            self.side.as_str(),
            self.fill_price,
            self.fill_quantity,
            self.fill_notional(),
            if self.is_taker { "Taker" } else { "Maker" },
        )
    }
}

// ============================================================================
// FeeBreakdown — 手续费明细
// ============================================================================

/// 手续费明细。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeBreakdown {
    /// Maker 手续费。
    pub maker_fee: f64,
    /// Taker 手续费。
    pub taker_fee: f64,
    /// 交易手续费。
    pub trading_fee: f64,
    /// 结算手续费（接口预留，当前为 0）。
    pub settlement_fee: f64,
    /// 总手续费。
    pub total_fee: f64,
    /// 适用的手续费规则名称。
    pub fee_rule: String,
    /// 手续费率（小数形式，如 0.001 = 0.1%）。
    pub fee_rate: f64,
}

impl FeeBreakdown {
    /// 创建零手续费明细。
    pub fn zero() -> Self {
        Self {
            maker_fee: 0.0,
            taker_fee: 0.0,
            trading_fee: 0.0,
            settlement_fee: 0.0,
            total_fee: 0.0,
            fee_rule: "Default".to_string(),
            fee_rate: 0.0,
        }
    }

    /// 中文展示。
    pub fn display_zh(&self) -> String {
        format!(
            "手续费: Maker={:.4} Taker={:.4} Trading={:.4} Settlement={:.4} | 合计={:.4} | 规则={} (费率={:.4}%)",
            self.maker_fee,
            self.taker_fee,
            self.trading_fee,
            self.settlement_fee,
            self.total_fee,
            self.fee_rule,
            self.fee_rate * 100.0,
        )
    }
}

// ============================================================================
// FeeRule — 手续费规则
// ============================================================================

/// 手续费规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeRule {
    /// 规则名称。
    pub name: String,
    /// Maker 费率（小数形式）。
    pub maker_rate: f64,
    /// Taker 费率（小数形式）。
    pub taker_rate: f64,
    /// 交易费率（小数形式）。
    pub trading_rate: f64,
    /// 结算费率（接口预留，当前为 0）。
    pub settlement_rate: f64,
    /// 最低手续费（USDC）。
    pub min_fee: f64,
    /// 最高手续费（USDC），0 表示无上限。
    pub max_fee: f64,
}

impl Default for FeeRule {
    fn default() -> Self {
        Self {
            name: "Standard".to_string(),
            maker_rate: 0.0002,   // 0.02%
            taker_rate: 0.0005,   // 0.05%
            trading_rate: 0.0001, // 0.01%
            settlement_rate: 0.0, // 预留
            min_fee: 0.0,
            max_fee: 0.0, // 无上限
        }
    }
}

impl FeeRule {
    /// 零手续费规则（模拟环境）。
    pub fn zero_fee() -> Self {
        Self {
            name: "ZeroFee".to_string(),
            maker_rate: 0.0,
            taker_rate: 0.0,
            trading_rate: 0.0,
            settlement_rate: 0.0,
            min_fee: 0.0,
            max_fee: 0.0,
        }
    }

    /// 中文展示。
    pub fn display_zh(&self) -> String {
        format!(
            "手续费规则[{}]: Maker={:.2}% Taker={:.2}% Trading={:.2}% Settlement={:.2}% | 最低={:.4} 最高={}",
            self.name,
            self.maker_rate * 100.0,
            self.taker_rate * 100.0,
            self.trading_rate * 100.0,
            self.settlement_rate * 100.0,
            self.min_fee,
            if self.max_fee > 0.0 {
                format!("{:.4}", self.max_fee)
            } else {
                "无上限".to_string()
            },
        )
    }
}

// ============================================================================
// PositionState — 持仓状态（Settlement 内部）
// ============================================================================

/// Settlement 内部持仓状态。
///
/// 与 PMS Position 独立，Settlement Engine 是持仓的唯一更新中心。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionState {
    /// 持仓 ID（格式 `SPOS-YYYYMMDD-NNNNNN`）。
    pub position_id: String,
    /// 市场 ID。
    pub market_id: String,
    /// 方向（YES/NO）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,
    /// 持仓数量。
    pub quantity: f64,
    /// 开仓均价。
    pub average_price: f64,
    /// 开仓成本 = quantity * average_price。
    pub cost_basis: f64,
    /// 当前标记价。
    pub mark_price: f64,
    /// 持仓市值 = quantity * mark_price。
    pub market_value: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 关联订单 ID 列表。
    pub order_ids: Vec<String>,
    /// 关联成交 ID 列表。
    pub trade_ids: Vec<String>,
    /// 是否已平仓。
    pub is_closed: bool,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 最后更新时间。
    pub updated_at: DateTime<Local>,
}

impl PositionState {
    /// 开仓。
    pub fn open(
        position_id: String,
        market_id: String,
        direction: Direction,
        side: Side,
        quantity: f64,
        price: f64,
        order_id: String,
        trade_id: String,
        now: DateTime<Local>,
    ) -> Self {
        let cost = quantity * price;
        Self {
            position_id,
            market_id,
            direction,
            side,
            quantity,
            average_price: price,
            cost_basis: cost,
            mark_price: price,
            market_value: cost,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            order_ids: vec![order_id],
            trade_ids: vec![trade_id],
            is_closed: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// 加仓（均价调整）。
    pub fn add_fill(
        &mut self,
        qty: f64,
        price: f64,
        order_id: &str,
        trade_id: &str,
        now: DateTime<Local>,
    ) {
        let old_cost = self.cost_basis;
        let new_cost = qty * price;
        self.quantity += qty;
        self.cost_basis = old_cost + new_cost;
        self.average_price = self.cost_basis / self.quantity;
        self.market_value = self.quantity * self.mark_price;
        self.unrealized_pnl = self.quantity * (self.mark_price - self.average_price);
        if !self.order_ids.contains(&order_id.to_string()) {
            self.order_ids.push(order_id.to_string());
        }
        if !self.trade_ids.contains(&trade_id.to_string()) {
            self.trade_ids.push(trade_id.to_string());
        }
        self.updated_at = now;
    }

    /// 减仓（部分/全部平仓）。
    /// 返回已实现盈亏。
    pub fn reduce(&mut self, close_qty: f64, exit_price: f64, now: DateTime<Local>) -> f64 {
        let close_qty = close_qty.min(self.quantity);
        let realized = close_qty * (exit_price - self.average_price);
        self.realized_pnl += realized;
        self.quantity -= close_qty;
        self.cost_basis = self.quantity * self.average_price;
        self.market_value = self.quantity * self.mark_price;
        self.unrealized_pnl = self.quantity * (self.mark_price - self.average_price);
        self.updated_at = now;
        if self.quantity <= f64::EPSILON {
            self.is_closed = true;
        }
        realized
    }

    /// 重新标记价格。
    pub fn mark(&mut self, price: f64, now: DateTime<Local>) {
        self.mark_price = price;
        self.market_value = self.quantity * price;
        self.unrealized_pnl = self.quantity * (price - self.average_price);
        self.updated_at = now;
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "持仓 {} | {} {} | 数量={:.2} 均价={:.4} 成本={:.2} 市值={:.2} | 未实现={:.2} 已实现={:.2} | {}",
            self.position_id,
            self.market_id,
            self.direction.as_zh(),
            self.quantity,
            self.average_price,
            self.cost_basis,
            self.market_value,
            self.unrealized_pnl,
            self.realized_pnl,
            if self.is_closed {
                "已平仓"
            } else {
                "持仓中"
            },
        )
    }
}

// ============================================================================
// BalanceState — 余额状态（Settlement 内部）
// ============================================================================

/// Settlement 内部余额状态。
///
/// 所有余额变化必须通过 Settlement Engine。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceState {
    /// 账户 ID。
    pub account_id: String,
    /// 资产（默认 USDC）。
    pub asset: String,
    /// 可用余额。
    pub available: f64,
    /// 冻结余额。
    pub frozen: f64,
    /// 预留余额（接口预留）。
    pub reserved: f64,
    /// 账户权益。
    pub equity: f64,
    /// 钱包余额（交易所侧）。
    pub wallet_balance: f64,
    /// 净资产价值（NAV）。
    pub nav: f64,
    /// 最后更新时间。
    pub updated_at: DateTime<Local>,
}

impl BalanceState {
    /// 创建新余额状态。
    pub fn new(account_id: String, initial_balance: f64, now: DateTime<Local>) -> Self {
        Self {
            account_id,
            asset: "USDC".to_string(),
            available: initial_balance,
            frozen: 0.0,
            reserved: 0.0,
            equity: initial_balance,
            wallet_balance: initial_balance,
            nav: initial_balance,
            updated_at: now,
        }
    }

    /// 冻结资金。
    pub fn freeze(&mut self, amount: f64) {
        let amount = amount.min(self.available);
        self.available -= amount;
        self.frozen += amount;
    }

    /// 释放冻结资金。
    pub fn unfreeze(&mut self, amount: f64) {
        let amount = amount.min(self.frozen);
        self.frozen -= amount;
        self.available += amount;
    }

    /// 成交扣款（从冻结资金中扣除）。
    pub fn debit(&mut self, amount: f64) {
        // 优先从冻结中扣除
        let from_frozen = amount.min(self.frozen);
        self.frozen -= from_frozen;
        let remaining = amount - from_frozen;
        // 剩余从可用中扣除
        let from_available = remaining.min(self.available);
        self.available -= from_available;
        self.recalc_derived();
    }

    /// 平仓入账。
    pub fn credit(&mut self, amount: f64) {
        self.available += amount;
        self.recalc_derived();
    }

    /// 扣除手续费。
    pub fn charge_fee(&mut self, fee: f64) {
        self.available = (self.available - fee).max(0.0);
        self.recalc_derived();
    }

    /// 重算派生字段。
    fn recalc_derived(&mut self) {
        self.equity = self.available + self.frozen + self.reserved;
        self.wallet_balance = self.available + self.frozen;
        self.nav = self.equity;
    }

    /// 更新钱包余额（交易所侧同步）。
    pub fn sync_wallet(&mut self, balance: f64) {
        self.wallet_balance = balance;
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "余额 {} | 可用={:.2} 冻结={:.2} 预留={:.2} | 权益={:.2} NAV={:.2} | 钱包={:.2}",
            self.account_id,
            self.available,
            self.frozen,
            self.reserved,
            self.equity,
            self.nav,
            self.wallet_balance,
        )
    }
}

// ============================================================================
// LedgerEntry — 资金流水（追加不可修改）
// ============================================================================

/// 资金流水条目。
///
/// 所有资金变化必须生成 Ledger。
/// Ledger 禁止修改，只能追加（Append Only）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// 流水 ID（格式 `LEDGER-YYYYMMDD-NNNNNN`）。
    pub ledger_id: String,
    /// 关联成交 ID。
    pub trade_id: String,
    /// 关联订单 ID。
    pub order_id: String,
    /// 账户 ID。
    pub account_id: String,
    /// 资产（默认 USDC）。
    pub asset: String,
    /// 变动金额（正=入账，负=出账）。
    pub amount: f64,
    /// 手续费。
    pub fee: f64,
    /// 资金方向（Debit=出账 / Credit=入账）。
    pub direction: LedgerDirection,
    /// 变动前余额。
    pub before_balance: f64,
    /// 变动后余额。
    pub after_balance: f64,
    /// 摘要说明（中文）。
    pub description: String,
    /// 时间戳。
    pub timestamp: DateTime<Local>,
}

/// 资金方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerDirection {
    /// 出账（扣款）。
    Debit,
    /// 入账（收款）。
    Credit,
}

impl LedgerDirection {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            LedgerDirection::Debit => "出账",
            LedgerDirection::Credit => "入账",
        }
    }
}

impl LedgerEntry {
    /// 创建出账流水（成交扣款）。
    pub fn debit(
        ledger_id: String,
        trade_id: String,
        order_id: String,
        account_id: String,
        amount: f64,
        fee: f64,
        before_balance: f64,
        after_balance: f64,
        description: String,
        timestamp: DateTime<Local>,
    ) -> Self {
        Self {
            ledger_id,
            trade_id,
            order_id,
            account_id,
            asset: "USDC".to_string(),
            amount: -amount, // 出账为负数
            fee,
            direction: LedgerDirection::Debit,
            before_balance,
            after_balance,
            description,
            timestamp,
        }
    }

    /// 创建入账流水（平仓收款 / 退款）。
    pub fn credit(
        ledger_id: String,
        trade_id: String,
        order_id: String,
        account_id: String,
        amount: f64,
        fee: f64,
        before_balance: f64,
        after_balance: f64,
        description: String,
        timestamp: DateTime<Local>,
    ) -> Self {
        Self {
            ledger_id,
            trade_id,
            order_id,
            account_id,
            asset: "USDC".to_string(),
            amount, // 入账为正数
            fee,
            direction: LedgerDirection::Credit,
            before_balance,
            after_balance,
            description,
            timestamp,
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "{} | {} {} {} | 金额={:.4} 手续费={:.4} | 余额 {:.2} -> {:.2} | {}",
            self.ledger_id,
            self.trade_id,
            self.order_id,
            self.direction.as_zh(),
            self.amount.abs(),
            self.fee,
            self.before_balance,
            self.after_balance,
            self.description,
        )
    }

    /// CSV 行（用于持久化）。
    pub fn to_csv_row(&self) -> [String; 12] {
        [
            self.ledger_id.clone(),
            self.trade_id.clone(),
            self.order_id.clone(),
            self.account_id.clone(),
            self.asset.clone(),
            format!("{:.6}", self.amount),
            format!("{:.6}", self.fee),
            self.direction.as_zh().to_string(),
            format!("{:.6}", self.before_balance),
            format!("{:.6}", self.after_balance),
            self.description.clone(),
            self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        ]
    }

    /// CSV 表头。
    pub fn csv_header() -> [String; 12] {
        [
            "ledger_id".to_string(),
            "trade_id".to_string(),
            "order_id".to_string(),
            "account_id".to_string(),
            "asset".to_string(),
            "amount".to_string(),
            "fee".to_string(),
            "direction".to_string(),
            "before_balance".to_string(),
            "after_balance".to_string(),
            "description".to_string(),
            "timestamp".to_string(),
        ]
    }
}

// ============================================================================
// SettlementStatus — 结算状态
// ============================================================================

/// 结算状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettlementStatus {
    /// 结算成功。
    Success,
    /// 校验失败（终止结算）。
    ValidationFailed,
    /// 手续费计算失败。
    FeeFailed,
    /// 持仓更新失败。
    PositionFailed,
    /// 余额更新失败。
    BalanceFailed,
    /// 流水记录失败。
    LedgerFailed,
}

impl SettlementStatus {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            SettlementStatus::Success => "结算成功",
            SettlementStatus::ValidationFailed => "校验失败",
            SettlementStatus::FeeFailed => "手续费失败",
            SettlementStatus::PositionFailed => "持仓更新失败",
            SettlementStatus::BalanceFailed => "余额更新失败",
            SettlementStatus::LedgerFailed => "流水记录失败",
        }
    }

    /// 是否成功。
    pub fn is_success(&self) -> bool {
        matches!(self, SettlementStatus::Success)
    }
}

// ============================================================================
// SettlementResult — 结算结果（输出）
// ============================================================================

/// 结算结果。
///
/// Settlement Engine 处理完成后返回此结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResult {
    /// 结算 ID（格式 `SETTLE-YYYYMMDD-NNNNNN`）。
    pub settlement_id: String,
    /// 关联成交 ID。
    pub trade_id: String,
    /// 关联订单 ID。
    pub order_id: String,
    /// 结算状态。
    pub status: SettlementStatus,
    /// 手续费明细。
    pub fee_breakdown: FeeBreakdown,
    /// 持仓变化摘要。
    pub position_summary: Option<String>,
    /// 余额变化：结算前可用余额。
    pub balance_before: f64,
    /// 余额变化：结算后可用余额。
    pub balance_after: f64,
    /// 盈亏变化：已实现盈亏。
    pub realized_pnl: f64,
    /// 盈亏变化：未实现盈亏。
    pub unrealized_pnl: f64,
    /// 生成的流水条目数。
    pub ledger_count: usize,
    /// 结算耗时（毫秒）。
    pub elapsed_ms: u64,
    /// 结算时间。
    pub settled_at: DateTime<Local>,
    /// 错误信息（仅失败时）。
    pub error: Option<String>,
}

impl SettlementResult {
    /// 创建成功结算结果。
    pub fn success(
        settlement_id: String,
        trade_id: String,
        order_id: String,
        fee_breakdown: FeeBreakdown,
        position_summary: Option<String>,
        balance_before: f64,
        balance_after: f64,
        realized_pnl: f64,
        unrealized_pnl: f64,
        ledger_count: usize,
        elapsed_ms: u64,
        settled_at: DateTime<Local>,
    ) -> Self {
        Self {
            settlement_id,
            trade_id,
            order_id,
            status: SettlementStatus::Success,
            fee_breakdown,
            position_summary,
            balance_before,
            balance_after,
            realized_pnl,
            unrealized_pnl,
            ledger_count,
            elapsed_ms,
            settled_at,
            error: None,
        }
    }

    /// 创建失败结算结果。
    pub fn failed(
        settlement_id: String,
        trade_id: String,
        order_id: String,
        status: SettlementStatus,
        error: String,
        elapsed_ms: u64,
        settled_at: DateTime<Local>,
    ) -> Self {
        Self {
            settlement_id,
            trade_id,
            order_id,
            status,
            fee_breakdown: FeeBreakdown::zero(),
            position_summary: None,
            balance_before: 0.0,
            balance_after: 0.0,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            ledger_count: 0,
            elapsed_ms,
            settled_at,
            error: Some(error),
        }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let mut s = format!(
            "结算 {} | 成交 {} | 订单 {} | 状态={} | 耗时={}ms",
            self.settlement_id,
            self.trade_id,
            self.order_id,
            self.status.as_zh(),
            self.elapsed_ms,
        );
        if self.status.is_success() {
            s.push_str(&format!(
                "\n  手续费: total={:.4} (maker={:.4} taker={:.4} trading={:.4})",
                self.fee_breakdown.total_fee,
                self.fee_breakdown.maker_fee,
                self.fee_breakdown.taker_fee,
                self.fee_breakdown.trading_fee,
            ));
            if let Some(ref ps) = self.position_summary {
                s.push_str(&format!("\n  持仓: {}", ps));
            }
            s.push_str(&format!(
                "\n  余额: {:.2} -> {:.2} (Δ={:.2})",
                self.balance_before,
                self.balance_after,
                self.balance_after - self.balance_before,
            ));
            s.push_str(&format!(
                "\n  盈亏: 已实现={:.2} 未实现={:.2}",
                self.realized_pnl, self.unrealized_pnl,
            ));
            s.push_str(&format!("\n  流水: {} 条", self.ledger_count));
        } else {
            s.push_str(&format!(
                "\n  错误: {}",
                self.error.as_deref().unwrap_or("未知"),
            ));
        }
        s
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn trade_fill_event_notional() {
        let ev = TradeFillEvent {
            trade_id: "T-001".into(),
            order_id: "OMS-001".into(),
            client_order_id: "CLI-001".into(),
            exchange_order_id: None,
            market_id: "mkt-btc".into(),
            account_id: "ACCT-MAIN".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.55,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "MockGateway".into(),
        };
        assert!(approx(ev.fill_notional(), 55.0));
        assert!(ev.summary_zh().contains("T-001"));
    }

    #[test]
    fn fee_breakdown_zero() {
        let fb = FeeBreakdown::zero();
        assert!(approx(fb.total_fee, 0.0));
        assert_eq!(fb.fee_rule, "Default");
    }

    #[test]
    fn fee_rule_defaults() {
        let rule = FeeRule::default();
        assert!(!rule.name.is_empty());
        assert!(rule.maker_rate > 0.0);
        assert!(rule.taker_rate > rule.maker_rate); // Taker 费率更高
    }

    #[test]
    fn fee_rule_zero() {
        let rule = FeeRule::zero_fee();
        assert!(approx(rule.maker_rate, 0.0));
        assert!(approx(rule.taker_rate, 0.0));
    }

    #[test]
    fn position_state_open_add_reduce() {
        let now = Local::now();
        let mut pos = PositionState::open(
            "SPOS-001".into(),
            "mkt-btc".into(),
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            "T-001".into(),
            now,
        );
        assert!(approx(pos.quantity, 100.0));
        assert!(approx(pos.average_price, 0.50));
        assert!(approx(pos.cost_basis, 50.0));
        assert!(!pos.is_closed);

        // 加仓
        pos.add_fill(50.0, 0.60, "OMS-002", "T-002", now);
        assert!(approx(pos.quantity, 150.0));
        assert!(approx(pos.average_price, 0.533333333)); // (50+30)/150
        assert_eq!(pos.order_ids.len(), 2);
        assert_eq!(pos.trade_ids.len(), 2);

        // 减仓
        let realized = pos.reduce(75.0, 0.55, now);
        assert!(approx(realized, 1.25)); // 75 * (0.55 - 0.5333)
        assert!(!pos.is_closed);

        // 全部平仓
        let _realized2 = pos.reduce(75.0, 0.55, now);
        assert!(pos.is_closed);
    }

    #[test]
    fn balance_state_freeze_debit_credit() {
        let now = Local::now();
        let mut bal = BalanceState::new("ACCT-001".into(), 10000.0, now);
        assert!(approx(bal.available, 10000.0));

        bal.freeze(500.0);
        assert!(approx(bal.available, 9500.0));
        assert!(approx(bal.frozen, 500.0));

        bal.debit(500.0);
        assert!(approx(bal.frozen, 0.0));
        assert!(approx(bal.available, 9500.0));
        assert!(approx(bal.equity, 9500.0));

        bal.credit(300.0);
        assert!(approx(bal.available, 9800.0));
        assert!(approx(bal.equity, 9800.0));
    }

    #[test]
    fn balance_state_charge_fee() {
        let now = Local::now();
        let mut bal = BalanceState::new("ACCT-001".into(), 10000.0, now);
        bal.charge_fee(10.0);
        assert!(approx(bal.available, 9990.0));
    }

    #[test]
    fn ledger_entry_debit_credit() {
        let now = Local::now();
        let debit = LedgerEntry::debit(
            "L-001".into(),
            "T-001".into(),
            "OMS-001".into(),
            "ACCT-001".into(),
            55.0,
            0.02,
            10000.0,
            9944.98,
            "成交扣款".into(),
            now,
        );
        assert!(debit.amount < 0.0);
        assert_eq!(debit.direction, LedgerDirection::Debit);
        assert_eq!(debit.direction.as_zh(), "出账");

        let credit = LedgerEntry::credit(
            "L-002".into(),
            "T-002".into(),
            "OMS-002".into(),
            "ACCT-001".into(),
            60.0,
            0.02,
            9944.98,
            10004.96,
            "平仓入账".into(),
            now,
        );
        assert!(credit.amount > 0.0);
        assert_eq!(credit.direction, LedgerDirection::Credit);
        assert_eq!(credit.direction.as_zh(), "入账");

        // CSV 行格式
        let row = debit.to_csv_row();
        assert_eq!(row.len(), 12);
        assert_eq!(row[0], "L-001");
    }

    #[test]
    fn settlement_result_success_failed() {
        let now = Local::now();
        let ok = SettlementResult::success(
            "S-001".into(),
            "T-001".into(),
            "OMS-001".into(),
            FeeBreakdown::zero(),
            Some("mkt-btc YES ×100".into()),
            10000.0,
            9945.0,
            0.0,
            0.0,
            1,
            5,
            now,
        );
        assert!(ok.status.is_success());
        assert!(ok.summary_zh().contains("结算成功"));

        let fail = SettlementResult::failed(
            "S-002".into(),
            "T-002".into(),
            "OMS-002".into(),
            SettlementStatus::ValidationFailed,
            "余额不足".into(),
            1,
            now,
        );
        assert!(!fail.status.is_success());
        assert!(fail.summary_zh().contains("校验失败"));
    }

    #[test]
    fn settlement_status_chinese_names() {
        let all = [
            SettlementStatus::Success,
            SettlementStatus::ValidationFailed,
            SettlementStatus::FeeFailed,
            SettlementStatus::PositionFailed,
            SettlementStatus::BalanceFailed,
            SettlementStatus::LedgerFailed,
        ];
        for s in &all {
            assert!(!s.as_zh().is_empty());
        }
    }
}
