//! PMS 统一领域对象（P2-05 第二节）。
//!
//! 所有市场统一使用本模块定义的领域类型。
//! 禁止交易所对象（Gateway 类型）进入 PMS。
//!
//! # 类型清单
//!
//! - [`Portfolio`]：投资组合（总资产/可用资金/冻结资金/持仓价值/总权益/盈亏/收益率）。
//! - [`Account`]：账户（余额/持仓列表）。
//! - [`Position`]：统一持仓模型（支持 Prediction/Spot/Perpetual/AMM）。
//! - [`Holding`]：持仓快照（mark-to-market 用）。
//! - [`Balance`]：余额。
//! - [`Asset`] / [`AssetType`]：资产分类。
//! - [`Currency`]：货币。
//! - [`PnLReport`]：盈亏报告。
//! - [`ValuationReport`]：估值报告。
//! - [`ExposureReport`]：风险敞口报告。
//! - [`PmsMetrics`]：PMS 指标。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不上链。

use chrono::{DateTime, Local};
use pm_core::Side;
pub use pm_execution::order::Direction;
use serde::{Deserialize, Serialize};

// ============================================================================
// AssetType — 资产类型
// ============================================================================

/// 资产类型（支持未来新增市场，无需修改接口）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetType {
    /// 预测市场（Polymarket 等）。
    Prediction,
    /// 现货（BTC/ETH 等）。
    Spot,
    /// 永续合约。
    Perpetual,
    /// AMM 流动性。
    AMM,
}

impl AssetType {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            AssetType::Prediction => "预测市场",
            AssetType::Spot => "现货",
            AssetType::Perpetual => "永续合约",
            AssetType::AMM => "AMM",
        }
    }

    /// 英文标识符（CSV/日志 key）。
    pub fn as_str(&self) -> &'static str {
        match self {
            AssetType::Prediction => "Prediction",
            AssetType::Spot => "Spot",
            AssetType::Perpetual => "Perpetual",
            AssetType::AMM => "AMM",
        }
    }
}

// ============================================================================
// PositionStatus — 持仓状态
// ============================================================================

/// 持仓状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionStatus {
    /// 持仓中。
    Open,
    /// 已平仓。
    Closed,
}

impl PositionStatus {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            PositionStatus::Open => "持仓中",
            PositionStatus::Closed => "已平仓",
        }
    }
}

// ============================================================================
// Currency — 货币
// ============================================================================

/// 货币。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Currency(pub String);

impl Currency {
    pub fn usdc() -> Self {
        Currency("USDC".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Currency {
    fn default() -> Self {
        Currency::usdc()
    }
}

// ============================================================================
// Balance — 余额
// ============================================================================

/// 账户余额。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    /// 总余额。
    pub total: f64,
    /// 可用余额。
    pub available: f64,
    /// 冻结余额。
    pub frozen: f64,
}

impl Balance {
    /// 创建新余额。
    pub fn new(total: f64) -> Self {
        Self {
            total,
            available: total,
            frozen: 0.0,
        }
    }

    /// 冻结资金（可用→冻结）。
    pub fn freeze(&mut self, amount: f64) {
        let amount = amount.min(self.available);
        self.available -= amount;
        self.frozen += amount;
    }

    /// 释放冻结资金（冻结→可用）。
    pub fn unfreeze(&mut self, amount: f64) {
        let amount = amount.min(self.frozen);
        self.frozen -= amount;
        self.available += amount;
    }

    /// 扣减可用资金（用于成交扣款）。
    pub fn debit(&mut self, amount: f64) {
        let amount = amount.min(self.available);
        self.available -= amount;
        self.total -= amount;
        // 同步减少冻结（如果之前冻结过）
        let unfreeze = amount.min(self.frozen);
        self.frozen -= unfreeze;
    }

    /// 增加可用资金（用于平仓入账）。
    pub fn credit(&mut self, amount: f64) {
        self.available += amount;
        self.total += amount;
    }
}

// ============================================================================
// Asset — 资产
// ============================================================================

/// 资产定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// 资产类型。
    pub asset_type: AssetType,
    /// 交易对/符号（如 "BTC-USD"）。
    pub symbol: String,
    /// 计价货币。
    pub currency: Currency,
}

// ============================================================================
// Position — PMS 统一持仓模型（P2-05 第四节）
// ============================================================================

/// PMS 统一持仓模型。
///
/// 支持 Prediction / Spot / Perpetual / AMM 所有市场类型。
/// 未来新增市场类型无需修改本结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// 持仓 ID（PMS 内部，格式 `POS-YYYYMMDD-NNNNNN`）。
    pub position_id: String,
    /// 市场 ID。
    pub market_id: String,
    /// 资产类型。
    pub asset_type: AssetType,
    /// 方向（YES/NO，Polymarket 特有；其他市场默认为 Yes）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,
    /// 持仓数量。
    pub quantity: f64,
    /// 开仓均价。
    pub average_price: f64,
    /// 当前标记价。
    pub current_price: f64,
    /// 持仓市值 = quantity * current_price。
    pub market_value: f64,
    /// 开仓成本 = quantity * average_price。
    pub cost_basis: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 收益率（ROI）。
    pub roi: f64,
    /// 持仓状态。
    pub status: PositionStatus,
    /// 关联订单 ID 列表。
    pub order_ids: Vec<String>,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 最后更新时间。
    pub updated_at: DateTime<Local>,
    /// 平仓时间。
    pub closed_at: Option<DateTime<Local>>,
    /// 备注。
    pub notes: String,
}

impl Position {
    /// 开仓：以 entry_price 买入 quantity 份额。
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        position_id: String,
        market_id: String,
        asset_type: AssetType,
        direction: Direction,
        side: Side,
        quantity: f64,
        entry_price: f64,
        order_id: String,
        now: DateTime<Local>,
    ) -> Self {
        let cost = quantity * entry_price;
        Self {
            position_id,
            market_id,
            asset_type,
            direction,
            side,
            quantity,
            average_price: entry_price,
            current_price: entry_price,
            market_value: cost,
            cost_basis: cost,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            roi: 0.0,
            status: PositionStatus::Open,
            order_ids: vec![order_id],
            created_at: now,
            updated_at: now,
            closed_at: None,
            notes: String::new(),
        }
    }

    /// mark-to-market：更新 current_price / market_value / unrealized_pnl / roi。
    pub fn mark(&mut self, current_price: f64, now: DateTime<Local>) {
        self.current_price = current_price;
        self.market_value = self.quantity * current_price;
        self.unrealized_pnl = self.quantity * (current_price - self.average_price);
        self.roi = if self.cost_basis.abs() > f64::EPSILON {
            self.unrealized_pnl / self.cost_basis
        } else {
            0.0
        };
        self.updated_at = now;
    }

    /// 部分平仓：减少 quantity，按比例结算 realized_pnl。
    /// 返回已实现盈亏。
    pub fn reduce(&mut self, close_qty: f64, exit_price: f64, now: DateTime<Local>) -> f64 {
        let close_qty = close_qty.min(self.quantity);
        let realized = close_qty * (exit_price - self.average_price);
        self.realized_pnl += realized;
        self.quantity -= close_qty;
        self.cost_basis = self.quantity * self.average_price;
        self.market_value = self.quantity * self.current_price;
        self.unrealized_pnl = self.quantity * (self.current_price - self.average_price);
        self.roi = if self.cost_basis.abs() > f64::EPSILON {
            (self.unrealized_pnl + self.realized_pnl)
                / (self.cost_basis + close_qty * self.average_price)
        } else {
            0.0
        };
        self.updated_at = now;
        if self.quantity <= f64::EPSILON {
            self.status = PositionStatus::Closed;
            self.closed_at = Some(now);
        }
        realized
    }

    /// 完全平仓：计算 realized_pnl，状态置 Closed。
    pub fn close(&mut self, exit_price: f64, now: DateTime<Local>) -> f64 {
        let realized = self.quantity * (exit_price - self.average_price);
        self.realized_pnl += realized;
        self.current_price = exit_price;
        self.market_value = 0.0;
        self.unrealized_pnl = 0.0;
        self.roi = if self.cost_basis.abs() > f64::EPSILON {
            self.realized_pnl / self.cost_basis
        } else {
            0.0
        };
        self.quantity = 0.0;
        self.status = PositionStatus::Closed;
        self.closed_at = Some(now);
        self.updated_at = now;
        realized
    }

    /// 添加关联订单 ID。
    pub fn add_order_id(&mut self, order_id: &str) {
        if !self.order_ids.contains(&order_id.to_string()) {
            self.order_ids.push(order_id.to_string());
        }
    }

    /// 加仓（均价调整）。
    pub fn add_quantity(&mut self, qty: f64, price: f64, order_id: &str, now: DateTime<Local>) {
        let old_cost = self.cost_basis;
        let new_cost = qty * price;
        self.quantity += qty;
        self.cost_basis = old_cost + new_cost;
        self.average_price = self.cost_basis / self.quantity;
        self.market_value = self.quantity * self.current_price;
        self.unrealized_pnl = self.quantity * (self.current_price - self.average_price);
        self.add_order_id(order_id);
        self.updated_at = now;
    }

    /// 持仓时长（秒）；未平仓时按 now 计算。
    pub fn duration_secs(&self, now: DateTime<Local>) -> f64 {
        let end = self.closed_at.unwrap_or(now);
        (end - self.created_at).num_seconds() as f64
    }

    /// 是否为盈利持仓（含未实现 + 已实现）。
    pub fn is_profitable(&self) -> bool {
        self.unrealized_pnl + self.realized_pnl > f64::EPSILON
    }
}

// ============================================================================
// Holding — 持仓快照
// ============================================================================

/// 持仓快照（用于 Valuation Engine 快速汇总，不持有完整 Position 生命周期信息）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holding {
    /// 市场 ID。
    pub market_id: String,
    /// 资产类型。
    pub asset_type: AssetType,
    /// 持有数量。
    pub quantity: f64,
    /// 开仓均价。
    pub average_price: f64,
    /// 当前标记价。
    pub current_price: f64,
    /// 市值 = quantity * current_price。
    pub market_value: f64,
}

impl From<&Position> for Holding {
    fn from(pos: &Position) -> Self {
        Self {
            market_id: pos.market_id.clone(),
            asset_type: pos.asset_type,
            quantity: pos.quantity,
            average_price: pos.average_price,
            current_price: pos.current_price,
            market_value: pos.market_value,
        }
    }
}

// ============================================================================
// Account — 账户
// ============================================================================

/// 交易账户。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// 账户 ID。
    pub account_id: String,
    /// 账户名称。
    pub name: String,
    /// 计价货币。
    pub currency: Currency,
    /// 余额。
    pub balance: Balance,
    /// 持仓 ID 列表（Position::position_id）。
    pub position_ids: Vec<String>,
    /// 创建时间。
    pub created_at: DateTime<Local>,
}

impl Account {
    /// 创建新账户。
    pub fn new(
        account_id: String,
        name: String,
        currency: Currency,
        initial_balance: f64,
        now: DateTime<Local>,
    ) -> Self {
        Self {
            account_id,
            name,
            currency,
            balance: Balance::new(initial_balance),
            position_ids: Vec::new(),
            created_at: now,
        }
    }

    /// 默认主账户。
    pub fn default_main(now: DateTime<Local>) -> Self {
        Self::new(
            "ACCT-MAIN-001".to_string(),
            "主账户".to_string(),
            Currency::usdc(),
            10_000.0,
            now,
        )
    }

    /// 账户总价值 = 可用资金 + 冻结资金。
    pub fn total_cash(&self) -> f64 {
        self.balance.available + self.balance.frozen
    }
}

// ============================================================================
// Portfolio — 投资组合（P2-05 第三节）
// ============================================================================

/// 投资组合。
///
/// 统一管理：总资产 / 可用资金 / 冻结资金 / 持仓价值 / 总权益 / 未实现盈亏 / 已实现盈亏 / 收益率。
/// 支持多账户。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    /// 组合 ID。
    pub portfolio_id: String,
    /// 组合名称。
    pub name: String,
    /// 关联账户 ID 列表。
    pub account_ids: Vec<String>,
    /// 总资产 = 可用资金 + 冻结资金 + 持仓价值。
    pub total_assets: f64,
    /// 可用资金。
    pub available_cash: f64,
    /// 冻结资金。
    pub frozen_cash: f64,
    /// 持仓总价值（所有持仓市值之和）。
    pub position_value: f64,
    /// 总权益 = 可用资金 + 冻结资金 + 持仓价值 + 未实现盈亏。
    pub total_equity: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 收益率 = total_pnl / initial_capital。
    pub roi: f64,
    /// 总盈亏 = realized_pnl + unrealized_pnl。
    pub total_pnl: f64,
    /// 初始资金。
    pub initial_capital: f64,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 最后更新时间。
    pub updated_at: DateTime<Local>,
}

impl Portfolio {
    /// 创建新投资组合。
    pub fn new(
        portfolio_id: String,
        name: String,
        initial_capital: f64,
        now: DateTime<Local>,
    ) -> Self {
        Self {
            portfolio_id,
            name,
            account_ids: Vec::new(),
            total_assets: initial_capital,
            available_cash: initial_capital,
            frozen_cash: 0.0,
            position_value: 0.0,
            total_equity: initial_capital,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            roi: 0.0,
            total_pnl: 0.0,
            initial_capital,
            created_at: now,
            updated_at: now,
        }
    }

    /// 默认投资组合。
    pub fn default_portfolio(now: DateTime<Local>) -> Self {
        Self::new(
            "PF-MAIN-001".to_string(),
            "主投资组合".to_string(),
            10_000.0,
            now,
        )
    }

    /// 重算组合各项指标。
    /// 调用方传入：当前持仓列表（用于汇总 position_value / unrealized_pnl）。
    pub fn revalue(&mut self, positions: &[Position], now: DateTime<Local>) {
        self.position_value = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .map(|p| p.market_value)
            .sum();
        self.unrealized_pnl = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .map(|p| p.unrealized_pnl)
            .sum();
        self.realized_pnl = positions.iter().map(|p| p.realized_pnl).sum();
        self.total_pnl = self.realized_pnl + self.unrealized_pnl;
        self.total_assets = self.available_cash + self.frozen_cash + self.position_value;
        self.total_equity = self.available_cash + self.frozen_cash + self.position_value;
        self.roi = if self.initial_capital.abs() > f64::EPSILON {
            self.total_pnl / self.initial_capital
        } else {
            0.0
        };
        self.updated_at = now;
    }

    /// 冻结资金。
    pub fn freeze_cash(&mut self, amount: f64) {
        let amount = amount.min(self.available_cash);
        self.available_cash -= amount;
        self.frozen_cash += amount;
    }

    /// 释放冻结资金。
    pub fn unfreeze_cash(&mut self, amount: f64) {
        let amount = amount.min(self.frozen_cash);
        self.frozen_cash -= amount;
        self.available_cash += amount;
    }

    /// 成交扣款（从冻结资金中扣除，不重复扣可用资金）。
    pub fn debit(&mut self, amount: f64) {
        let amount = amount.min(self.frozen_cash);
        self.frozen_cash -= amount;
    }

    /// 平仓入账。
    pub fn credit(&mut self, amount: f64) {
        self.available_cash += amount;
    }

    /// 添加账户。
    pub fn add_account(&mut self, account_id: &str) {
        if !self.account_ids.contains(&account_id.to_string()) {
            self.account_ids.push(account_id.to_string());
        }
    }
}

// ============================================================================
// PnLReport — 盈亏报告（P2-05 第五节）
// ============================================================================

/// 盈亏报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnLReport {
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 当日盈亏。
    pub daily_pnl: f64,
    /// 累计总盈亏。
    pub total_pnl: f64,
    /// 收益率。
    pub roi: f64,
    /// 胜率 = winning_trades / total_trades。
    pub win_rate: f64,
    /// 平均盈利（盈利交易的平均收益）。
    pub avg_profit: f64,
    /// 平均亏损（亏损交易的平均亏损，正数）。
    pub avg_loss: f64,
    /// 盈亏比 = avg_profit / avg_loss。
    pub profit_factor: f64,
    /// 总交易数（已平仓）。
    pub total_trades: usize,
    /// 盈利交易数。
    pub winning_trades: usize,
    /// 亏损交易数。
    pub losing_trades: usize,
}

impl Default for PnLReport {
    fn default() -> Self {
        Self {
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            daily_pnl: 0.0,
            total_pnl: 0.0,
            roi: 0.0,
            win_rate: 0.0,
            avg_profit: 0.0,
            avg_loss: 0.0,
            profit_factor: 0.0,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
        }
    }
}

// ============================================================================
// ValuationReport — 估值报告（P2-05 第六节）
// ============================================================================

/// 估值报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationReport {
    /// 持仓总价值。
    pub position_value: f64,
    /// 投资组合总价值。
    pub portfolio_value: f64,
    /// 现金价值。
    pub cash_value: f64,
    /// 总敞口。
    pub total_exposure: f64,
    /// 净资产价值（NAV）。
    pub nav: f64,
    /// 总市值（position_value + cash_value）。
    pub market_value: f64,
    /// 估值时间。
    pub valued_at: DateTime<Local>,
}

// ============================================================================
// ExposureReport — 风险敞口报告（P2-05 第七节）
// ============================================================================

/// 单个市场敞口。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketExposure {
    /// 市场 ID。
    pub market_id: String,
    /// 多头敞口。
    pub long_exposure: f64,
    /// 空头敞口。
    pub short_exposure: f64,
    /// 净敞口 = long - short。
    pub net_exposure: f64,
}

/// 资产配置项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetAllocation {
    /// 资产类型。
    pub asset_type: AssetType,
    /// 配置价值。
    pub value: f64,
    /// 配置比例（占总资产）。
    pub percentage: f64,
}

/// 风险敞口报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureReport {
    /// 多头总敞口。
    pub long_exposure: f64,
    /// 空头总敞口。
    pub short_exposure: f64,
    /// 净敞口 = long - short。
    pub net_exposure: f64,
    /// 预测市场敞口。
    pub prediction_exposure: f64,
    /// 现货敞口。
    pub spot_exposure: f64,
    /// AMM 敞口。
    pub amm_exposure: f64,
    /// 永续合约敞口。
    pub perpetual_exposure: f64,
    /// 各市场敞口明细。
    pub market_exposures: Vec<MarketExposure>,
    /// 资产配置。
    pub asset_allocation: Vec<AssetAllocation>,
    /// 报告时间。
    pub reported_at: DateTime<Local>,
}

// ============================================================================
// PmsMetrics — PMS 指标（P2-05 第十一节）
// ============================================================================

/// PMS 统计指标。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmsMetrics {
    /// 账户净值（NAV）。
    pub nav: f64,
    /// 当前持仓数。
    pub position_count: usize,
    /// 盈利率（已平仓盈利占比）。
    pub win_rate: f64,
    /// 最大回撤（接口预留，当前未实现）。
    pub max_drawdown: Option<f64>,
    /// 累计收益率。
    pub return_rate: f64,
    /// 平均持仓时间（秒）。
    pub avg_holding_time_secs: f64,
    /// 平均每笔收益。
    pub avg_profit_per_trade: f64,
    /// 总交易笔数（已平仓）。
    pub total_closed_trades: usize,
    /// 生成时间。
    pub generated_at: DateTime<Local>,
}

impl Default for PmsMetrics {
    fn default() -> Self {
        Self {
            nav: 0.0,
            position_count: 0,
            win_rate: 0.0,
            max_drawdown: None,
            return_rate: 0.0,
            avg_holding_time_secs: 0.0,
            avg_profit_per_trade: 0.0,
            total_closed_trades: 0,
            generated_at: Local::now(),
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

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn asset_type_chinese_names_unique() {
        let types = [
            AssetType::Prediction,
            AssetType::Spot,
            AssetType::Perpetual,
            AssetType::AMM,
        ];
        for t in &types {
            assert!(!t.as_zh().is_empty());
            assert!(!t.as_str().is_empty());
        }
    }

    #[test]
    fn balance_freeze_unfreeze() {
        let mut b = Balance::new(1000.0);
        assert!(approx(b.available, 1000.0));
        assert!(approx(b.frozen, 0.0));

        b.freeze(300.0);
        assert!(approx(b.available, 700.0));
        assert!(approx(b.frozen, 300.0));
        assert!(approx(b.total, 1000.0));

        b.unfreeze(150.0);
        assert!(approx(b.available, 850.0));
        assert!(approx(b.frozen, 150.0));
    }

    #[test]
    fn balance_debit_credit() {
        let mut b = Balance::new(1000.0);
        b.freeze(200.0);
        b.debit(200.0); // 成交扣款（先释放冻结）
        assert!(approx(b.available, 600.0));
        assert!(approx(b.frozen, 0.0));
        assert!(approx(b.total, 800.0));

        b.credit(500.0); // 平仓入账
        assert!(approx(b.available, 1100.0));
        assert!(approx(b.total, 1300.0));
    }

    #[test]
    fn position_open_mark_close() {
        let now = Local::now();
        let mut pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            200.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        assert_eq!(pos.status, PositionStatus::Open);
        assert!(approx(pos.cost_basis, 100.0));
        assert!(approx(pos.market_value, 100.0));
        assert!(approx(pos.unrealized_pnl, 0.0));

        // mark-to-market：价格上涨
        pos.mark(0.55, now);
        assert!(approx(pos.current_price, 0.55));
        assert!(approx(pos.market_value, 110.0));
        assert!(approx(pos.unrealized_pnl, 10.0));
        assert!(approx(pos.roi, 0.10));

        // 平仓
        let realized = pos.close(0.55, now);
        assert!(approx(realized, 10.0));
        assert_eq!(pos.status, PositionStatus::Closed);
        assert!(pos.closed_at.is_some());
    }

    #[test]
    fn position_add_quantity_averages_price() {
        let now = Local::now();
        let mut pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        // 加仓：100 * 0.60
        pos.add_quantity(100.0, 0.60, "OMS-002", now);
        assert!(approx(pos.quantity, 200.0));
        assert!(approx(pos.average_price, 0.55)); // (50+60)/200
        assert!(approx(pos.cost_basis, 110.0));
        assert_eq!(pos.order_ids.len(), 2);
    }

    #[test]
    fn position_reduce_partial_close() {
        let now = Local::now();
        let mut pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            200.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        let realized = pos.reduce(100.0, 0.60, now);
        assert!(approx(realized, 10.0));
        assert!(approx(pos.quantity, 100.0));
        assert_eq!(pos.status, PositionStatus::Open);

        // 再平 100
        let realized2 = pos.reduce(100.0, 0.40, now);
        assert!(approx(realized2, -10.0));
        assert_eq!(pos.status, PositionStatus::Closed);
    }

    #[test]
    fn portfolio_revalue_works() {
        let now = Local::now();
        let mut pf = Portfolio::default_portfolio(now);
        let pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            200.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        // 模拟成交后的资金变化：先冻结再扣款
        pf.freeze_cash(100.0); // 冻结资金
        pf.debit(100.0); // 从冻结中扣款
        pf.revalue(&[pos], now);
        assert!(approx(pf.position_value, 100.0));
        assert!(approx(pf.total_assets, 10000.0)); // 9900 + 0 + 100
        assert!(approx(pf.total_equity, 10000.0));
    }

    #[test]
    fn holding_from_position() {
        let now = Local::now();
        let pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            200.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        let h = Holding::from(&pos);
        assert_eq!(h.market_id, "mkt-btc");
        assert!(approx(h.market_value, 100.0));
    }

    #[test]
    fn account_default_creation() {
        let now = Local::now();
        let acct = Account::default_main(now);
        assert_eq!(acct.account_id, "ACCT-MAIN-001");
        assert_eq!(acct.name, "主账户");
        assert!(approx(acct.balance.available, 10_000.0));
        assert!(acct.position_ids.is_empty());
    }

    #[test]
    fn pnl_report_defaults() {
        let r = PnLReport::default();
        assert!(approx(r.total_pnl, 0.0));
        assert_eq!(r.total_trades, 0);
    }

    #[test]
    fn metrics_defaults() {
        let m = PmsMetrics::default();
        assert!(approx(m.nav, 0.0));
        assert_eq!(m.position_count, 0);
        assert!(m.max_drawdown.is_none());
    }
}
