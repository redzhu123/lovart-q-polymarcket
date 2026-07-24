//! Fee Engine（手续费引擎 — P2-06 第三节）。
//!
//! 统一手续费模型，支持：
//! - Maker Fee（挂单手续费）
//! - Taker Fee（吃单手续费）
//! - Trading Fee（交易手续费）
//! - Settlement Fee（结算手续费，接口预留）
//!
//! 所有手续费统一计算，禁止各模块自行计算手续费。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use crate::types::{FeeBreakdown, FeeRule, TradeFillEvent};

// ============================================================================
// FeeEngine — 统一手续费引擎
// ============================================================================

/// 手续费引擎。
///
/// 持有当前生效的手续费规则，为每笔成交统一计算手续费。
#[derive(Debug, Clone)]
pub struct FeeEngine {
    /// Maker 手续费规则。
    pub maker_rule: FeeRule,
    /// Taker 手续费规则。
    pub taker_rule: FeeRule,
    /// 当前活跃规则引用。
    pub active_rule: FeeRule,
}

impl Default for FeeEngine {
    fn default() -> Self {
        let rule = FeeRule::default();
        Self {
            maker_rule: FeeRule {
                name: "Maker".to_string(),
                maker_rate: rule.maker_rate,
                taker_rate: 0.0,
                trading_rate: 0.0,
                settlement_rate: 0.0,
                min_fee: rule.min_fee,
                max_fee: rule.max_fee,
            },
            taker_rule: FeeRule {
                name: "Taker".to_string(),
                maker_rate: 0.0,
                taker_rate: rule.taker_rate,
                trading_rate: 0.0,
                settlement_rate: 0.0,
                min_fee: rule.min_fee,
                max_fee: rule.max_fee,
            },
            active_rule: rule,
        }
    }
}

impl FeeEngine {
    /// 创建新手续费引擎。
    pub fn new(rule: FeeRule) -> Self {
        Self {
            maker_rule: FeeRule {
                name: "Maker".to_string(),
                maker_rate: rule.maker_rate,
                taker_rate: 0.0,
                trading_rate: 0.0,
                settlement_rate: 0.0,
                min_fee: rule.min_fee,
                max_fee: rule.max_fee,
            },
            taker_rule: FeeRule {
                name: "Taker".to_string(),
                maker_rate: 0.0,
                taker_rate: rule.taker_rate,
                trading_rate: 0.0,
                settlement_rate: 0.0,
                min_fee: rule.min_fee,
                max_fee: rule.max_fee,
            },
            active_rule: rule,
        }
    }

    /// 创建零手续费引擎（模拟环境）。
    pub fn zero_fee() -> Self {
        Self::new(FeeRule::zero_fee())
    }

    /// 计算成交手续费。
    ///
    /// # 参数
    ///
    /// - `event`：成交事件。
    ///
    /// # 返回
    ///
    /// 手续费明细（FeeBreakdown）。
    pub fn calculate(&self, event: &TradeFillEvent) -> FeeBreakdown {
        let notional = event.fill_notional();
        let rule = &self.active_rule;

        // 根据 Taker/Maker 属性分配费率
        let (maker_rate, taker_rate) = if event.is_taker {
            (0.0, rule.taker_rate)
        } else {
            (rule.maker_rate, 0.0)
        };

        let maker_fee = notional * maker_rate;
        let taker_fee = notional * taker_rate;
        let trading_fee = notional * rule.trading_rate;
        let settlement_fee = notional * rule.settlement_rate;

        let mut total_fee = maker_fee + taker_fee + trading_fee + settlement_fee;

        // 应用最低/最高手续费限制
        if rule.min_fee > 0.0 && total_fee < rule.min_fee {
            total_fee = rule.min_fee;
        }
        if rule.max_fee > 0.0 && total_fee > rule.max_fee {
            total_fee = rule.max_fee;
        }

        tracing::info!(
            trade_id = %event.trade_id,
            order_id = %event.order_id,
            notional = %notional,
            maker_fee = %maker_fee,
            taker_fee = %taker_fee,
            trading_fee = %trading_fee,
            settlement_fee = %settlement_fee,
            total_fee = %total_fee,
            is_taker = %event.is_taker,
            rule = %rule.name,
            "手续费计算完成"
        );

        FeeBreakdown {
            maker_fee,
            taker_fee,
            trading_fee,
            settlement_fee,
            total_fee,
            fee_rule: rule.name.clone(),
            fee_rate: if event.is_taker {
                rule.taker_rate
            } else {
                rule.maker_rate
            },
        }
    }

    /// 计算仅 Maker 手续费。
    pub fn maker_fee(&self, notional: f64) -> f64 {
        let fee = notional * self.active_rule.maker_rate;
        self.clamp_fee(fee)
    }

    /// 计算仅 Taker 手续费。
    pub fn taker_fee(&self, notional: f64) -> f64 {
        let fee = notional * self.active_rule.taker_rate;
        self.clamp_fee(fee)
    }

    /// 计算总有效费率（Maker + Taker + Trading）。
    pub fn effective_rate(&self, is_taker: bool) -> f64 {
        let rule = &self.active_rule;
        let base = if is_taker {
            rule.taker_rate
        } else {
            rule.maker_rate
        };
        base + rule.trading_rate + rule.settlement_rate
    }

    /// 手续费钳位（应用 min/max）。
    fn clamp_fee(&self, fee: f64) -> f64 {
        let mut f = fee;
        if self.active_rule.min_fee > 0.0 && f < self.active_rule.min_fee {
            f = self.active_rule.min_fee;
        }
        if self.active_rule.max_fee > 0.0 && f > self.active_rule.max_fee {
            f = self.active_rule.max_fee;
        }
        f
    }

    /// 设置新规则。
    pub fn set_rule(&mut self, rule: FeeRule) {
        tracing::info!(
            old_rule = %self.active_rule.name,
            new_rule = %rule.name,
            "手续费规则已切换"
        );
        self.maker_rule = FeeRule {
            name: "Maker".to_string(),
            maker_rate: rule.maker_rate,
            taker_rate: 0.0,
            trading_rate: 0.0,
            settlement_rate: 0.0,
            min_fee: rule.min_fee,
            max_fee: rule.max_fee,
        };
        self.taker_rule = FeeRule {
            name: "Taker".to_string(),
            maker_rate: 0.0,
            taker_rate: rule.taker_rate,
            trading_rate: 0.0,
            settlement_rate: 0.0,
            min_fee: rule.min_fee,
            max_fee: rule.max_fee,
        };
        self.active_rule = rule;
    }

    /// 生成手续费报告（中文）。
    pub fn report_zh(&self) -> String {
        let r = &self.active_rule;
        format!(
            "【手续费规则】\n\
             规则名称: {}\n\
             Maker 费率: {:.4}%\n\
             Taker 费率: {:.4}%\n\
             交易费率: {:.4}%\n\
             结算费率: {:.4}%（预留）\n\
             最低手续费: {:.4} USDC\n\
             最高手续费: {} USDC",
            r.name,
            r.maker_rate * 100.0,
            r.taker_rate * 100.0,
            r.trading_rate * 100.0,
            r.settlement_rate * 100.0,
            r.min_fee,
            if r.max_fee > 0.0 {
                format!("{:.4}", r.max_fee)
            } else {
                "无上限".to_string()
            },
        )
    }

    /// 打印手续费报告（中文 CLI 输出）。
    pub fn print_zh(&self) {
        println!();
        println!("{}", self.report_zh());
        println!();
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Direction;
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn sample_fill(is_taker: bool) -> TradeFillEvent {
        TradeFillEvent {
            trade_id: "T-001".into(),
            order_id: "OMS-001".into(),
            client_order_id: "CLI-001".into(),
            exchange_order_id: None,
            market_id: "mkt-btc".into(),
            account_id: "ACCT-MAIN".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.50,
            fill_quantity: 200.0,
            filled_at: Local::now(),
            is_taker,
            gateway_name: "Mock".into(),
        }
    }

    #[test]
    fn default_engine_has_standard_rates() {
        let engine = FeeEngine::default();
        assert!(engine.active_rule.maker_rate > 0.0);
        assert!(engine.active_rule.taker_rate > engine.active_rule.maker_rate);
    }

    #[test]
    fn zero_fee_engine_charges_nothing() {
        let engine = FeeEngine::zero_fee();
        let fill = sample_fill(true);
        let breakdown = engine.calculate(&fill);
        assert!(approx(breakdown.total_fee, 0.0));
        assert!(approx(breakdown.maker_fee, 0.0));
        assert!(approx(breakdown.taker_fee, 0.0));
    }

    #[test]
    fn taker_fee_higher_than_maker() {
        let engine = FeeEngine::default();
        let taker_fill = sample_fill(true);
        let maker_fill = sample_fill(false);

        let taker_bd = engine.calculate(&taker_fill);
        let maker_bd = engine.calculate(&maker_fill);

        // Taker 应该比 Maker 手续费高
        assert!(taker_bd.taker_fee > 0.0);
        assert!(maker_bd.maker_fee > 0.0);
        assert!(taker_bd.taker_fee > maker_bd.maker_fee);
    }

    #[test]
    fn fee_calculation_proportional_to_notional() {
        let engine = FeeEngine::default();
        let mut small = sample_fill(true);
        small.fill_quantity = 100.0; // notional = 50
        let mut large = sample_fill(true);
        large.fill_quantity = 400.0; // notional = 200

        let small_bd = engine.calculate(&small);
        let large_bd = engine.calculate(&large);

        // 大额成交的手续费应该是小额成交的 4 倍（按比例）
        assert!(large_bd.taker_fee > small_bd.taker_fee);
        let ratio = large_bd.taker_fee / small_bd.taker_fee;
        assert!((ratio - 4.0).abs() < 0.01);
    }

    #[test]
    fn min_fee_applied() {
        let mut rule = FeeRule::zero_fee();
        rule.min_fee = 1.0;
        let engine = FeeEngine::new(rule);
        let fill = sample_fill(true);
        let bd = engine.calculate(&fill);
        assert!(approx(bd.total_fee, 1.0));
    }

    #[test]
    fn max_fee_capped() {
        let mut engine = FeeEngine::default();
        // 设一个很低的 max_fee 验证上限生效
        engine.active_rule.max_fee = 0.001;
        let fill = sample_fill(true);
        let bd = engine.calculate(&fill);
        assert!(bd.total_fee <= 0.001 + 1e-9);
    }

    #[test]
    fn effective_rate_differs_by_role() {
        let engine = FeeEngine::default();
        let taker_rate = engine.effective_rate(true);
        let maker_rate = engine.effective_rate(false);
        assert!(taker_rate > maker_rate);
    }

    #[test]
    fn set_rule_updates_active() {
        let mut engine = FeeEngine::default();
        let old_name = engine.active_rule.name.clone();
        let new_rule = FeeRule::zero_fee();
        engine.set_rule(new_rule);
        assert_ne!(engine.active_rule.name, old_name);
        assert_eq!(engine.active_rule.name, "ZeroFee");
    }

    #[test]
    fn report_zh_contains_key_info() {
        let engine = FeeEngine::default();
        let report = engine.report_zh();
        assert!(report.contains("手续费规则"));
        assert!(report.contains("Maker 费率"));
        assert!(report.contains("Taker 费率"));
    }

    #[test]
    fn print_zh_does_not_panic() {
        let engine = FeeEngine::default();
        engine.print_zh();
    }
}
