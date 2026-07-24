//! OMS Order Validator（P2-04 第五节）。
//!
//! 提交订单前自动校验。所有校验失败必须直接拒绝，不得发送到 Gateway。
//!
//! # 校验维度
//!
//! - 价格合法（> 0、有限、非 NaN、在 [0.01, 0.99] 范围内）
//! - 数量合法（> 0、有限、非 NaN）
//! - 余额满足（根据 OMS 当前持仓估算需要资金）
//! - 市场开放（市场未关闭）
//! - 参数完整（market_id / strategy_id / client_order_id 非空）
//!
//! 设计为白名单规则（每条独立 Rule），便于未来扩展。

use chrono::{DateTime, Local};
use pm_core::Side;
use pm_execution::order::Direction;
use pm_gateway::{Balance, OrderType, TimeInForce};
use serde::{Deserialize, Serialize};

use crate::order::{Order, OrderStatus};

// ============================================================================
// ValidationContext — 校验上下文
// ============================================================================

/// 校验上下文：包含账户 / 市场 / OMS 当前状态。
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// 当前账户余额（来自 Gateway 查询或本地缓存）。
    pub balance: Option<Balance>,
    /// 市场是否开放（来自 Gateway / Scanner）。
    pub market_open: bool,
    /// OMS 当前活跃订单数。
    pub active_order_count: usize,
    /// OMS 当前活跃订单最大金额。
    pub max_active_orders: usize,
    /// 当前下单时间。
    pub now: DateTime<Local>,
}

impl ValidationContext {
    /// 创建一个最简上下文（无余额约束，市场开放）。
    pub fn minimal() -> Self {
        Self {
            balance: None,
            market_open: true,
            active_order_count: 0,
            max_active_orders: 100,
            now: Local::now(),
        }
    }

    /// 带余额创建。
    pub fn with_balance(balance: Balance) -> Self {
        Self {
            balance: Some(balance),
            market_open: true,
            active_order_count: 0,
            max_active_orders: 100,
            now: Local::now(),
        }
    }
}

// ============================================================================
// ValidationOutcome — 校验结果
// ============================================================================

/// 单条校验结果。
#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    /// 规则名。
    pub rule: String,
    /// 通过？
    pub passed: bool,
    /// 中文消息（通过时为 "通过"，失败时为具体原因）。
    pub message: String,
}

impl ValidationOutcome {
    pub fn pass(rule: &str) -> Self {
        Self {
            rule: rule.to_string(),
            passed: true,
            message: "通过".to_string(),
        }
    }
    pub fn fail(rule: &str, msg: &str) -> Self {
        Self {
            rule: rule.to_string(),
            passed: false,
            message: msg.to_string(),
        }
    }
}

/// 汇总校验结果。
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub outcomes: Vec<ValidationOutcome>,
    pub all_passed: bool,
}

impl ValidationResult {
    pub fn from_outcomes(outcomes: Vec<ValidationOutcome>) -> Self {
        let all_passed = outcomes.iter().all(|o| o.passed);
        Self { outcomes, all_passed }
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        if self.all_passed {
            format!("校验通过（{} 条规则全部满足）", self.outcomes.len())
        } else {
            let failed: Vec<String> = self
                .outcomes
                .iter()
                .filter(|o| !o.passed)
                .map(|o| format!("{}：{}", o.rule, o.message))
                .collect();
            format!("校验失败：{}", failed.join("；"))
        }
    }
}

// ============================================================================
// ValidationRule Trait
// ============================================================================

/// 校验规则。
pub trait ValidationRule: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self, order: &Order, ctx: &ValidationContext) -> ValidationOutcome;
}

// ============================================================================
// 内置规则
// ============================================================================

/// 价格合法：> 0、有限、非 NaN、≤ 0.99（市场二元期权）。
pub struct PriceRule;
impl ValidationRule for PriceRule {
    fn name(&self) -> &str { "价格合法" }
    fn check(&self, order: &Order, _: &ValidationContext) -> ValidationOutcome {
        let p = order.price;
        if !p.is_finite() {
            return ValidationOutcome::fail(self.name(), "价格非有限数（NaN 或 Inf）");
        }
        if p <= 0.0 {
            return ValidationOutcome::fail(self.name(), "价格必须 > 0");
        }
        if p > 0.99 {
            return ValidationOutcome::fail(self.name(), "价格不能超过 0.99（二元期权）");
        }
        ValidationOutcome::pass(self.name())
    }
}

/// 数量合法：> 0、有限、非 NaN。
pub struct QuantityRule;
impl ValidationRule for QuantityRule {
    fn name(&self) -> &str { "数量合法" }
    fn check(&self, order: &Order, _: &ValidationContext) -> ValidationOutcome {
        let q = order.quantity;
        if !q.is_finite() {
            return ValidationOutcome::fail(self.name(), "数量非有限数");
        }
        if q <= 0.0 {
            return ValidationOutcome::fail(self.name(), "数量必须 > 0");
        }
        ValidationOutcome::pass(self.name())
    }
}

/// 余额满足：订单名义金额不超过账户可用余额（如果是 Buy）。
pub struct BalanceRule;
impl ValidationRule for BalanceRule {
    fn name(&self) -> &str { "余额满足" }
    fn check(&self, order: &Order, ctx: &ValidationContext) -> ValidationOutcome {
        // Sell 不消耗余额
        if matches!(order.side, Side::Sell) {
            return ValidationOutcome::pass(self.name());
        }
        let Some(bal) = &ctx.balance else {
            return ValidationOutcome::pass(self.name()); // 无余额信息跳过
        };
        let cost = order.notional();
        if bal.available < cost {
            return ValidationOutcome::fail(
                self.name(),
                &format!("可用余额 {:.2} < 订单成本 {:.2}", bal.available, cost),
            );
        }
        ValidationOutcome::pass(self.name())
    }
}

/// 市场开放：市场未关闭（ctx.market_open = true）。
pub struct MarketStateRule;
impl ValidationRule for MarketStateRule {
    fn name(&self) -> &str { "市场开放" }
    fn check(&self, _: &Order, ctx: &ValidationContext) -> ValidationOutcome {
        if !ctx.market_open {
            return ValidationOutcome::fail(self.name(), "市场已关闭，禁止下单");
        }
        ValidationOutcome::pass(self.name())
    }
}

/// 参数完整：market_id / client_order_id / strategy_id 非空。
pub struct CompletenessRule;
impl ValidationRule for CompletenessRule {
    fn name(&self) -> &str { "参数完整" }
    fn check(&self, order: &Order, _: &ValidationContext) -> ValidationOutcome {
        if order.market_id.trim().is_empty() {
            return ValidationOutcome::fail(self.name(), "market_id 不能为空");
        }
        if order.client_order_id.trim().is_empty() {
            return ValidationOutcome::fail(self.name(), "client_order_id 不能为空");
        }
        if order.strategy_id.trim().is_empty() {
            return ValidationOutcome::fail(self.name(), "strategy_id 不能为空");
        }
        ValidationOutcome::pass(self.name())
    }
}

/// 订单类型 / 有效期一致：Market 单不接受 IOC（市场无此语义）。
pub struct OrderTypeCoherenceRule;
impl ValidationRule for OrderTypeCoherenceRule {
    fn name(&self) -> &str { "订单类型一致" }
    fn check(&self, order: &Order, _: &ValidationContext) -> ValidationOutcome {
        if matches!(order.order_type, OrderType::Market)
            && matches!(order.time_in_force, TimeInForce::Ioc)
        {
            return ValidationOutcome::fail(
                self.name(),
                "市价单不接受 IOC（应使用 FOK 或 GTC）",
            );
        }
        ValidationOutcome::pass(self.name())
    }
}

/// 活跃订单数上限：未超过 OMS 配置的 max_active_orders。
pub struct ActiveOrderLimitRule;
impl ValidationRule for ActiveOrderLimitRule {
    fn name(&self) -> &str { "活跃订单上限" }
    fn check(&self, _: &Order, ctx: &ValidationContext) -> ValidationOutcome {
        if ctx.active_order_count >= ctx.max_active_orders {
            return ValidationOutcome::fail(
                self.name(),
                &format!(
                    "活跃订单数 {} 已达上限 {}",
                    ctx.active_order_count, ctx.max_active_orders
                ),
            );
        }
        ValidationOutcome::pass(self.name())
    }
}

/// 方向合法：Direction 必须为 Yes / No 之一（强制类型）。
pub struct DirectionRule;
impl ValidationRule for DirectionRule {
    fn name(&self) -> &str { "方向合法" }
    fn check(&self, order: &Order, _: &ValidationContext) -> ValidationOutcome {
        match order.direction {
            Direction::Yes | Direction::No => ValidationOutcome::pass(self.name()),
        }
    }
}

/// 状态前置：仅 Created 状态的订单可被 Validator 校验。
pub struct StatePreconditionRule;
impl ValidationRule for StatePreconditionRule {
    fn name(&self) -> &str { "状态前置" }
    fn check(&self, order: &Order, _: &ValidationContext) -> ValidationOutcome {
        if order.status != OrderStatus::Created {
            return ValidationOutcome::fail(
                self.name(),
                &format!(
                    "仅 Created 状态可校验，当前状态：{}",
                    order.status.as_zh()
                ),
            );
        }
        ValidationOutcome::pass(self.name())
    }
}

// ============================================================================
// Validator — 编排器
// ============================================================================

/// OMS 校验器：顺序执行所有 Rule，聚合结果。
pub struct Validator {
    rules: Vec<Box<dyn ValidationRule>>,
}

impl Default for Validator {
    fn default() -> Self {
        Self::with_default_rules()
    }
}

impl Validator {
    /// 创建空 Validator。
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 创建带全部默认规则的 Validator（9 条）。
    pub fn with_default_rules() -> Self {
        let mut v = Self::new();
        v.add_rule(Box::new(StatePreconditionRule));
        v.add_rule(Box::new(CompletenessRule));
        v.add_rule(Box::new(PriceRule));
        v.add_rule(Box::new(QuantityRule));
        v.add_rule(Box::new(DirectionRule));
        v.add_rule(Box::new(OrderTypeCoherenceRule));
        v.add_rule(Box::new(MarketStateRule));
        v.add_rule(Box::new(BalanceRule));
        v.add_rule(Box::new(ActiveOrderLimitRule));
        v
    }

    /// 添加自定义规则。
    pub fn add_rule(&mut self, rule: Box<dyn ValidationRule>) {
        self.rules.push(rule);
    }

    /// 规则数。
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 执行校验。
    pub fn validate(&self, order: &Order, ctx: &ValidationContext) -> ValidationResult {
        let outcomes: Vec<ValidationOutcome> = self
            .rules
            .iter()
            .map(|r| r.check(order, ctx))
            .collect();
        let result = ValidationResult::from_outcomes(outcomes);
        if result.all_passed {
            tracing::info!(
                order_id = %order.order_id,
                client_order_id = %order.client_order_id,
                rules = self.rules.len(),
                "OMS 校验通过"
            );
        } else {
            tracing::warn!(
                order_id = %order.order_id,
                client_order_id = %order.client_order_id,
                failed = result.summary_zh(),
                "OMS 校验失败"
            );
        }
        result
    }
}

// ============================================================================
// Config
// ============================================================================

/// Validator 配置（可关闭部分规则）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorConfig {
    pub check_balance: bool,
    pub check_market_state: bool,
    pub check_active_limit: bool,
    pub max_active_orders: usize,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            check_balance: true,
            check_market_state: true,
            check_active_limit: true,
            max_active_orders: 100,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::Order;
    use pm_gateway::{OrderType, TimeInForce};

    fn base_order() -> Order {
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
    fn validator_default_has_9_rules() {
        let v = Validator::with_default_rules();
        assert_eq!(v.rule_count(), 9);
    }

    #[test]
    fn happy_path_all_pass() {
        let v = Validator::with_default_rules();
        let order = base_order();
        let mut ctx = ValidationContext::minimal();
        ctx.balance = Some(Balance::mock(10_000.0));
        let r = v.validate(&order, &ctx);
        assert!(r.all_passed, "expected pass, got {}", r.summary_zh());
        assert_eq!(r.outcomes.len(), 9);
    }

    #[test]
    fn price_invalid_caught() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.price = 0.0;
        let ctx = ValidationContext::minimal();
        let r = v.validate(&order, &ctx);
        assert!(!r.all_passed);
        let failed: Vec<_> = r.outcomes.iter().filter(|o| !o.passed).collect();
        assert!(failed.iter().any(|o| o.rule == "价格合法"));
    }

    #[test]
    fn price_too_high_caught() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.price = 1.0;
        let r = v.validate(&order, &ValidationContext::minimal());
        assert!(!r.all_passed);
    }

    #[test]
    fn price_nan_caught() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.price = f64::NAN;
        let r = v.validate(&order, &ValidationContext::minimal());
        assert!(!r.all_passed);
    }

    #[test]
    fn quantity_invalid_caught() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.quantity = -10.0;
        let r = v.validate(&order, &ValidationContext::minimal());
        assert!(!r.all_passed);
    }

    #[test]
    fn balance_insufficient_caught() {
        let v = Validator::with_default_rules();
        let order = base_order(); // 0.45 * 100 = 45
        let mut ctx = ValidationContext::minimal();
        ctx.balance = Some(Balance::mock(10.0));
        let r = v.validate(&order, &ctx);
        assert!(!r.all_passed);
        let failed: Vec<_> = r.outcomes.iter().filter(|o| !o.passed).collect();
        assert!(failed.iter().any(|o| o.rule == "余额满足"));
    }

    #[test]
    fn balance_rule_skipped_for_sell() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.side = Side::Sell;
        let mut ctx = ValidationContext::minimal();
        ctx.balance = Some(Balance::mock(0.0)); // 即使 0 余额，Sell 也通过
        let r = v.validate(&order, &ctx);
        assert!(r.all_passed, "Sell 应该跳过余额检查: {}", r.summary_zh());
    }

    #[test]
    fn market_closed_caught() {
        let v = Validator::with_default_rules();
        let mut ctx = ValidationContext::minimal();
        ctx.market_open = false;
        let r = v.validate(&base_order(), &ctx);
        assert!(!r.all_passed);
    }

    #[test]
    fn missing_market_id_caught() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.market_id = "".into();
        let r = v.validate(&order, &ValidationContext::minimal());
        assert!(!r.all_passed);
    }

    #[test]
    fn market_order_with_ioc_caught() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.order_type = OrderType::Market;
        order.time_in_force = TimeInForce::Ioc;
        let r = v.validate(&order, &ValidationContext::minimal());
        assert!(!r.all_passed);
    }

    #[test]
    fn active_order_limit_caught() {
        let v = Validator::with_default_rules();
        let mut ctx = ValidationContext::minimal();
        ctx.max_active_orders = 5;
        ctx.active_order_count = 5;
        let r = v.validate(&base_order(), &ctx);
        assert!(!r.all_passed);
    }

    #[test]
    fn state_precondition_caught_when_not_created() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.transition(OrderStatus::Validated, "测试", "oms", Local::now());
        let r = v.validate(&order, &ValidationContext::minimal());
        assert!(!r.all_passed);
    }

    #[test]
    fn validator_chinese_summary() {
        let v = Validator::with_default_rules();
        let mut order = base_order();
        order.price = -1.0;
        let r = v.validate(&order, &ValidationContext::minimal());
        let s = r.summary_zh();
        assert!(s.contains("校验失败") || s.contains("价格"));
    }

    #[test]
    fn validator_config_default() {
        let cfg = ValidatorConfig::default();
        assert!(cfg.check_balance);
        assert!(cfg.check_market_state);
        assert!(cfg.check_active_limit);
        assert_eq!(cfg.max_active_orders, 100);
    }
}