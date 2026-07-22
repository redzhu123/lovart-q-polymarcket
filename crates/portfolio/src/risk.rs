//! 风控：[`RiskManager`] + [`RiskPolicy`]。
//!
//! 统一风控闸，提供四类检查（简单版本）：
//! - Maximum Position：开仓数上限（[`RiskManager::check_open_position`]）。
//! - Maximum Open Orders：待处理订单上限（[`RiskManager::check_submit_order`]）。
//! - Cash Check：可用现金是否足以覆盖单笔成本 / 订单金额。
//! - Maximum Daily Loss：当日已实现亏损超过上限则禁止新开仓（[`RiskManager::check_daily_loss`]）。
//!
//! `RiskPolicy` 由 driver 从 `Config`（portfolio / execution / risk 段）组装后注入，
//! 本 crate 不依赖 pm-models，保持低耦合。引擎层（paper / execution）仍保留各自的内部硬限制作为兜底。

/// 风控策略参数。由调用方从 `Config` 组装。
#[derive(Debug, Clone, Copy)]
pub struct RiskPolicy {
    /// 最大同时持仓数（paper 开仓上限）。
    pub max_positions: usize,
    /// 单笔持仓最大成本（USDC，paper 现金检查基准）。
    pub max_position_size: f64,
    /// 最大待处理订单数（execution 提交上限）。
    pub max_open_orders: usize,
    /// 单日最大亏损（USDC，超过则禁止新开仓）。
    pub max_daily_loss: f64,
}

impl Default for RiskPolicy {
    fn default() -> Self {
        Self {
            max_positions: 10,
            max_position_size: 100.0,
            max_open_orders: 20,
            max_daily_loss: 1000.0,
        }
    }
}

/// 风控拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskRejection {
    /// 已达最大持仓数。
    MaxPositions,
    /// 已达最大待处理订单数。
    MaxOpenOrders,
    /// 可用现金不足。
    InsufficientCash,
    /// 当日亏损已达上限。
    MaxDailyLoss,
    /// 价格非法（非正 / 非有限）。
    InvalidPrice,
}

impl RiskRejection {
    /// 控制台展示文案。
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskRejection::MaxPositions => "Max Positions Reached",
            RiskRejection::MaxOpenOrders => "Max Open Orders Reached",
            RiskRejection::InsufficientCash => "Insufficient Cash",
            RiskRejection::MaxDailyLoss => "Max Daily Loss Reached",
            RiskRejection::InvalidPrice => "Invalid Price",
        }
    }
}

/// 风控管理器：持有策略参数与当日已实现盈亏累计，提供开仓 / 提交 / 日亏检查。
#[derive(Debug, Clone)]
pub struct RiskManager {
    policy: RiskPolicy,
    /// 当日已实现盈亏累计（负值表示亏损）。
    daily_realized_pnl: f64,
}

impl RiskManager {
    pub fn new(policy: RiskPolicy) -> Self {
        Self {
            policy,
            daily_realized_pnl: 0.0,
        }
    }

    /// 当前策略参数（只读）。
    pub fn policy(&self) -> RiskPolicy {
        self.policy
    }

    /// 当日已实现盈亏累计。
    pub fn daily_realized_pnl(&self) -> f64 {
        self.daily_realized_pnl
    }

    /// 记录一笔已实现盈亏（平仓后调用）。
    pub fn record_realized_pnl(&mut self, pnl: f64) {
        if pnl.is_finite() {
            self.daily_realized_pnl += pnl;
        }
    }

    /// 重置当日累计（跨日时调用）。
    pub fn reset_day(&mut self) {
        self.daily_realized_pnl = 0.0;
    }

    /// 开仓前检查：价格合法性 -> 持仓数上限 -> 现金充足 -> 当日亏损上限。
    pub fn check_open_position(
        &self,
        entry_price: f64,
        available_cash: f64,
        current_open_positions: usize,
    ) -> Result<(), RiskRejection> {
        if !entry_price.is_finite() || entry_price <= 0.0 {
            return Err(RiskRejection::InvalidPrice);
        }
        if current_open_positions >= self.policy.max_positions {
            return Err(RiskRejection::MaxPositions);
        }
        if available_cash + f64::EPSILON < self.policy.max_position_size {
            return Err(RiskRejection::InsufficientCash);
        }
        self.check_daily_loss()?;
        Ok(())
    }

    /// 提交订单前检查：价格合法性 -> 待处理订单上限 -> 现金充足（按 `notional`）-> 当日亏损上限。
    pub fn check_submit_order(
        &self,
        price: f64,
        notional: f64,
        available_cash: f64,
        current_pending_orders: usize,
    ) -> Result<(), RiskRejection> {
        if !price.is_finite() || price <= 0.0 {
            return Err(RiskRejection::InvalidPrice);
        }
        if current_pending_orders >= self.policy.max_open_orders {
            return Err(RiskRejection::MaxOpenOrders);
        }
        if available_cash + f64::EPSILON < notional {
            return Err(RiskRejection::InsufficientCash);
        }
        self.check_daily_loss()?;
        Ok(())
    }

    /// 当日亏损上限检查：已实现亏损超过 `max_daily_loss` 则拒绝。
    pub fn check_daily_loss(&self) -> Result<(), RiskRejection> {
        if -self.daily_realized_pnl > self.policy.max_daily_loss {
            return Err(RiskRejection::MaxDailyLoss);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RiskPolicy {
        RiskPolicy {
            max_positions: 2,
            max_position_size: 100.0,
            max_open_orders: 3,
            max_daily_loss: 500.0,
        }
    }

    #[test]
    fn open_position_blocks() {
        let rm = RiskManager::new(policy());
        // 非法价格
        assert_eq!(
            rm.check_open_position(0.0, 10000.0, 0),
            Err(RiskRejection::InvalidPrice)
        );
        assert_eq!(
            rm.check_open_position(f64::NAN, 10000.0, 0),
            Err(RiskRejection::InvalidPrice)
        );
        // 持仓数上限
        assert_eq!(
            rm.check_open_position(0.5, 10000.0, 2),
            Err(RiskRejection::MaxPositions)
        );
        // 现金不足
        assert_eq!(
            rm.check_open_position(0.5, 50.0, 0),
            Err(RiskRejection::InsufficientCash)
        );
        // 正常
        assert!(rm.check_open_position(0.5, 10000.0, 0).is_ok());
    }

    #[test]
    fn submit_order_blocks() {
        let rm = RiskManager::new(policy());
        assert_eq!(
            rm.check_submit_order(0.5, 100.0, 10000.0, 3),
            Err(RiskRejection::MaxOpenOrders)
        );
        assert_eq!(
            rm.check_submit_order(0.5, 100.0, 50.0, 0),
            Err(RiskRejection::InsufficientCash)
        );
        assert!(rm.check_submit_order(0.5, 100.0, 10000.0, 0).is_ok());
    }

    #[test]
    fn daily_loss_blocks_after_threshold() {
        let mut rm = RiskManager::new(policy());
        assert!(rm.check_open_position(0.5, 10000.0, 0).is_ok());
        rm.record_realized_pnl(-600.0); // 亏损 600 > max_daily_loss 500
        assert_eq!(
            rm.check_open_position(0.5, 10000.0, 0),
            Err(RiskRejection::MaxDailyLoss)
        );
        // 跨日重置后恢复
        rm.reset_day();
        assert!(rm.check_open_position(0.5, 10000.0, 0).is_ok());
    }

    #[test]
    fn daily_loss_partial_recovery() {
        let mut rm = RiskManager::new(policy());
        rm.record_realized_pnl(-400.0);
        assert!(rm.check_daily_loss().is_ok()); // 400 < 500
        rm.record_realized_pnl(200.0); // 累计 -200
        assert!(rm.check_daily_loss().is_ok());
        rm.record_realized_pnl(-400.0); // 累计 -600
        assert_eq!(rm.check_daily_loss(), Err(RiskRejection::MaxDailyLoss));
    }
}
