//! Execution Validator（V1.06 第五节）。
//!
//! 在订单进入 Queue 之前校验所有条件。
//! 任何失败 → 拒绝 Execution。
//!
//! 检查项：资金 / 价格 / 数量 / Risk / 重复订单 / 市场状态 / Provider。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use std::collections::HashSet;

use crate::order::Order;

// ============================================================================
// Validation Context
// ============================================================================

/// 校验上下文：提供 Validator 需要的外部状态。
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// 可用现金（USDC）。
    pub available_cash: f64,
    /// 当前待处理订单数。
    pub pending_orders: usize,
    /// 最大待处理订单数。
    pub max_pending_orders: usize,
    /// 当前开仓数。
    pub open_positions: usize,
    /// 最大开仓数。
    pub max_positions: usize,
    /// 已存在的 client_order_id 集合（用于去重）。
    pub existing_client_ids: HashSet<String>,
    /// 市场是否活跃。
    pub is_market_active: bool,
    /// Provider 是否健康。
    pub provider_healthy: bool,
}

impl Default for ValidationContext {
    fn default() -> Self {
        Self {
            available_cash: 10000.0,
            pending_orders: 0,
            max_pending_orders: 20,
            open_positions: 0,
            max_positions: 10,
            existing_client_ids: HashSet::new(),
            is_market_active: true,
            provider_healthy: true,
        }
    }
}

// ============================================================================
// Validation Result
// ============================================================================

/// 单条规则的校验结果。
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// 通过。
    Pass,
    /// 拒绝，附带中文原因。
    Reject { reason: String },
}

impl ValidationResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, ValidationResult::Pass)
    }

    pub fn is_reject(&self) -> bool {
        matches!(self, ValidationResult::Reject { .. })
    }
}

// ============================================================================
// Validation Rule Trait
// ============================================================================

/// 校验规则 trait。每条规则实现一个维度的校验。
pub trait ValidationRule: Send + Sync {
    /// 规则名称（中文）。
    fn name(&self) -> &str;

    /// 执行校验。
    fn validate(&self, order: &Order, context: &ValidationContext) -> ValidationResult;
}

// ============================================================================
// Built-in Rules
// ============================================================================

/// 价格规则：price > 0 且 finite。
pub struct PriceRule;

impl ValidationRule for PriceRule {
    fn name(&self) -> &str {
        "价格校验"
    }

    fn validate(&self, order: &Order, _ctx: &ValidationContext) -> ValidationResult {
        if !order.price.is_finite() || order.price <= 0.0 {
            return ValidationResult::Reject {
                reason: format!("价格非法: {}", order.price),
            };
        }
        if order.price > 1.0 {
            return ValidationResult::Reject {
                reason: format!("价格超出范围 (0~1): {:.4}", order.price),
            };
        }
        ValidationResult::Pass
    }
}

/// 数量规则：quantity > 0 且 finite。
pub struct QuantityRule;

impl ValidationRule for QuantityRule {
    fn name(&self) -> &str {
        "数量校验"
    }

    fn validate(&self, order: &Order, _ctx: &ValidationContext) -> ValidationResult {
        if !order.quantity.is_finite() || order.quantity <= 0.0 {
            return ValidationResult::Reject {
                reason: format!("数量非法: {}", order.quantity),
            };
        }
        ValidationResult::Pass
    }
}

/// 资金规则：available_cash >= notional。
pub struct CashRule;

impl ValidationRule for CashRule {
    fn name(&self) -> &str {
        "资金校验"
    }

    fn validate(&self, order: &Order, ctx: &ValidationContext) -> ValidationResult {
        let cost = order.notional();
        if ctx.available_cash + 1e-9 < cost {
            return ValidationResult::Reject {
                reason: format!(
                    "资金不足: 需要 {:.2} USDC，可用 {:.2} USDC",
                    cost, ctx.available_cash
                ),
            };
        }
        ValidationResult::Pass
    }
}

/// 待处理订单上限规则。
pub struct PendingLimitRule;

impl ValidationRule for PendingLimitRule {
    fn name(&self) -> &str {
        "待处理上限校验"
    }

    fn validate(&self, _order: &Order, ctx: &ValidationContext) -> ValidationResult {
        if ctx.pending_orders >= ctx.max_pending_orders {
            return ValidationResult::Reject {
                reason: format!(
                    "待处理订单已满: {}/{}",
                    ctx.pending_orders, ctx.max_pending_orders
                ),
            };
        }
        ValidationResult::Pass
    }
}

/// 持仓上限规则（仅 BUY）。
pub struct PositionLimitRule;

impl ValidationRule for PositionLimitRule {
    fn name(&self) -> &str {
        "持仓上限校验"
    }

    fn validate(&self, order: &Order, ctx: &ValidationContext) -> ValidationResult {
        use pm_core::Side;
        if order.side == Side::Buy && ctx.open_positions >= ctx.max_positions {
            return ValidationResult::Reject {
                reason: format!(
                    "持仓已满: {}/{}",
                    ctx.open_positions, ctx.max_positions
                ),
            };
        }
        ValidationResult::Pass
    }
}

/// 重复订单规则：检查 client_order_id 是否已存在。
pub struct DuplicateRule;

impl ValidationRule for DuplicateRule {
    fn name(&self) -> &str {
        "重复订单校验"
    }

    fn validate(&self, order: &Order, ctx: &ValidationContext) -> ValidationResult {
        if ctx.existing_client_ids.contains(&order.client_order_id) {
            return ValidationResult::Reject {
                reason: format!("重复订单: client_order_id={}", order.client_order_id),
            };
        }
        ValidationResult::Pass
    }
}

/// 市场状态规则：检查市场是否活跃。
pub struct MarketStateRule;

impl ValidationRule for MarketStateRule {
    fn name(&self) -> &str {
        "市场状态校验"
    }

    fn validate(&self, _order: &Order, ctx: &ValidationContext) -> ValidationResult {
        if !ctx.is_market_active {
            return ValidationResult::Reject {
                reason: "市场已关闭或不可用".to_string(),
            };
        }
        ValidationResult::Pass
    }
}

/// Provider 健康规则。
pub struct ProviderRule;

impl ValidationRule for ProviderRule {
    fn name(&self) -> &str {
        "Provider 校验"
    }

    fn validate(&self, _order: &Order, ctx: &ValidationContext) -> ValidationResult {
        if !ctx.provider_healthy {
            return ValidationResult::Reject {
                reason: "Provider 不健康，暂停接收订单".to_string(),
            };
        }
        ValidationResult::Pass
    }
}

// ============================================================================
// Execution Validator
// ============================================================================

/// 执行校验器（V1.06 第五节）。
///
/// 聚合所有 ValidationRule，对订单执行完整校验。
/// 任何规则拒绝 → 整体拒绝。
pub struct ExecutionValidator {
    rules: Vec<Box<dyn ValidationRule>>,
}

impl ExecutionValidator {
    /// 创建包含所有内置规则的 Validator。
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
        }
    }

    /// 创建包含所有默认规则的 Validator。
    pub fn with_default_rules() -> Self {
        let mut v = Self::new();
        v.add_rule(Box::new(PriceRule));
        v.add_rule(Box::new(QuantityRule));
        v.add_rule(Box::new(CashRule));
        v.add_rule(Box::new(PendingLimitRule));
        v.add_rule(Box::new(PositionLimitRule));
        v.add_rule(Box::new(DuplicateRule));
        v.add_rule(Box::new(MarketStateRule));
        v.add_rule(Box::new(ProviderRule));
        v
    }

    /// 添加一条校验规则。
    pub fn add_rule(&mut self, rule: Box<dyn ValidationRule>) {
        self.rules.push(rule);
    }

    /// 规则数量。
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 执行所有校验规则。
    ///
    /// 返回 (是否全部通过, 拒绝原因列表)。
    /// 即使某条规则失败，也会继续执行后续规则（收集所有失败原因）。
    pub fn validate(&self, order: &Order, context: &ValidationContext) -> ValidationOutcome {
        let mut rejections: Vec<String> = Vec::new();

        for rule in &self.rules {
            match rule.validate(order, context) {
                ValidationResult::Pass => {
                    tracing::debug!(
                        rule = %rule.name(),
                        order_id = %order.order_id,
                        "校验通过"
                    );
                }
                ValidationResult::Reject { reason } => {
                    tracing::warn!(
                        rule = %rule.name(),
                        order_id = %order.order_id,
                        reason = %reason,
                        "校验拒绝"
                    );
                    rejections.push(format!("[{}] {}", rule.name(), reason));
                }
            }
        }

        if rejections.is_empty() {
            ValidationOutcome::Pass
        } else {
            ValidationOutcome::Reject {
                reasons: rejections,
            }
        }
    }

    /// 快速校验：任一失败立即返回（不收集所有原因）。
    pub fn validate_fast(&self, order: &Order, context: &ValidationContext) -> ValidationOutcome {
        for rule in &self.rules {
            match rule.validate(order, context) {
                ValidationResult::Pass => {}
                ValidationResult::Reject { reason } => {
                    tracing::warn!(
                        rule = %rule.name(),
                        order_id = %order.order_id,
                        reason = %reason,
                        "快速校验拒绝"
                    );
                    return ValidationOutcome::Reject {
                        reasons: vec![format!("[{}] {}", rule.name(), reason)],
                    };
                }
            }
        }
        ValidationOutcome::Pass
    }
}

impl Default for ExecutionValidator {
    fn default() -> Self {
        Self::with_default_rules()
    }
}

/// 校验最终结果。
#[derive(Debug, Clone)]
pub enum ValidationOutcome {
    /// 全部通过。
    Pass,
    /// 拒绝，附带所有失败原因。
    Reject { reasons: Vec<String> },
}

impl ValidationOutcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, ValidationOutcome::Pass)
    }

    pub fn rejection_reasons(&self) -> Vec<String> {
        match self {
            ValidationOutcome::Pass => Vec::new(),
            ValidationOutcome::Reject { reasons } => reasons.clone(),
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::{Direction, Order};
    use chrono::Local;
    use pm_core::Side;

    fn make_order() -> Order {
        let now = Local::now();
        Order::new(
            "EX-001".into(),
            "CLI-001".into(),
            "mkt-1".into(),
            "mock".into(),
            Direction::Yes,
            Side::Buy,
            0.45,
            222.22,
            "S1".into(),
            "R1".into(),
            "O1".into(),
            now,
        )
    }

    #[test]
    fn all_rules_pass_for_normal_order() {
        let v = ExecutionValidator::with_default_rules();
        let o = make_order();
        let ctx = ValidationContext::default();
        let outcome = v.validate(&o, &ctx);
        assert!(outcome.is_pass());
    }

    #[test]
    fn price_rule_rejects_zero() {
        let v = ExecutionValidator::with_default_rules();
        let now = Local::now();
        let o = Order::new(
            "EX-001".into(), "C1".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, Side::Buy,
            0.0, 100.0,
            "S".into(), "R".into(), "O".into(), now,
        );
        let outcome = v.validate(&o, &ValidationContext::default());
        assert!(outcome.rejection_reasons().iter().any(|r| r.contains("价格")));
    }

    #[test]
    fn cash_rule_rejects_insufficient() {
        let v = ExecutionValidator::with_default_rules();
        let now = Local::now();
        let o = Order::new(
            "EX-001".into(), "C1".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, Side::Buy,
            0.5, 100000.0, // 需要 50000 USDC
            "S".into(), "R".into(), "O".into(), now,
        );
        let ctx = ValidationContext {
            available_cash: 100.0, // 只有 100
            ..ValidationContext::default()
        };
        let outcome = v.validate(&o, &ctx);
        assert!(outcome.rejection_reasons().iter().any(|r| r.contains("资金")));
    }

    #[test]
    fn pending_limit_rejects_when_full() {
        let v = ExecutionValidator::with_default_rules();
        let o = make_order();
        let ctx = ValidationContext {
            pending_orders: 20,
            max_pending_orders: 20,
            ..ValidationContext::default()
        };
        let outcome = v.validate(&o, &ctx);
        assert!(outcome.rejection_reasons().iter().any(|r| r.contains("待处理")));
    }

    #[test]
    fn duplicate_rule_rejects() {
        let v = ExecutionValidator::with_default_rules();
        let o = make_order();
        let mut ctx = ValidationContext::default();
        ctx.existing_client_ids.insert("CLI-001".to_string());
        let outcome = v.validate(&o, &ctx);
        assert!(outcome.rejection_reasons().iter().any(|r| r.contains("重复")));
    }

    #[test]
    fn market_state_rule_rejects_inactive() {
        let v = ExecutionValidator::with_default_rules();
        let o = make_order();
        let ctx = ValidationContext {
            is_market_active: false,
            ..ValidationContext::default()
        };
        let outcome = v.validate(&o, &ctx);
        assert!(outcome.rejection_reasons().iter().any(|r| r.contains("市场")));
    }

    #[test]
    fn validate_fast_stops_on_first_error() {
        let v = ExecutionValidator::with_default_rules();
        let now = Local::now();
        let o = Order::new(
            "EX-001".into(), "C1".into(), "mkt-1".into(), "mock".into(),
            Direction::Yes, Side::Buy,
            0.0, 0.0, // 价格和数量都非法
            "S".into(), "R".into(), "O".into(), now,
        );
        let outcome = v.validate_fast(&o, &ValidationContext::default());
        assert!(!outcome.is_pass());
        // fast 模式只返回第一条失败
        assert_eq!(outcome.rejection_reasons().len(), 1);
    }

    #[test]
    fn custom_validator_can_add_remove_rules() {
        let mut v = ExecutionValidator::new();
        assert_eq!(v.rule_count(), 0);
        v.add_rule(Box::new(PriceRule));
        assert_eq!(v.rule_count(), 1);
        let o = make_order();
        assert!(v.validate(&o, &ValidationContext::default()).is_pass());
    }
}
