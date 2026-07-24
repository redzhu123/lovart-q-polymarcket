//! OMS Order Matcher（P2-04 — 内部撮合辅助）。
//!
//! 在 OMS 内部为新订单寻找匹配（基于本地缓存的市场数据）。
//! 注意：真正的交易所撮合由 Gateway 完成；此处仅供 OMS 做：
//! - 内部风控预检（价格偏离检测）
//! - 延迟对账（本地状态 vs Gateway）
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use crate::order::Order;

/// 撮合决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchDecision {
    /// 允许通过：价格合理、数量合法。
    Allow,
    /// 警告：价格偏离最优价过大，建议 OMS 内部审查。
    Warn,
    /// 拒绝：价格严重偏离或数量异常。
    Reject,
}

impl MatchDecision {
    pub fn as_zh(&self) -> &'static str {
        match self {
            MatchDecision::Allow => "允许",
            MatchDecision::Warn => "警告",
            MatchDecision::Reject => "拒绝",
        }
    }
}

/// 撮合结果（含中文原因）。
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub decision: MatchDecision,
    pub reason: String,
    /// 价格偏离度（小数形式，0.05 = 5%）。
    pub price_deviation: f64,
}

impl MatchResult {
    pub fn allow() -> Self {
        Self {
            decision: MatchDecision::Allow,
            reason: "通过".into(),
            price_deviation: 0.0,
        }
    }
    pub fn warn(reason: &str, dev: f64) -> Self {
        Self {
            decision: MatchDecision::Warn,
            reason: reason.into(),
            price_deviation: dev,
        }
    }
    pub fn reject(reason: &str, dev: f64) -> Self {
        Self {
            decision: MatchDecision::Reject,
            reason: reason.into(),
            price_deviation: dev,
        }
    }

    pub fn summary_zh(&self) -> String {
        format!(
            "{}（偏离 {:.2}%）：{}",
            self.decision.as_zh(),
            self.price_deviation * 100.0,
            self.reason
        )
    }
}

/// 价格偏离阈值。
pub const WARN_DEVIATION: f64 = 0.02; // 2%
pub const REJECT_DEVIATION: f64 = 0.10; // 10%

/// 撮合器：基于外部市场快照判断订单价格是否合理。
pub struct Matcher;

impl Matcher {
    /// 评估订单价格相对最佳买/卖价的偏离度。
    ///
    /// # 参数
    ///
    /// - `order`：待评估订单。
    /// - `best_bid`：最优买价（可选）。
    /// - `best_ask`：最优卖价（可选）。
    ///
    /// # 决策
    ///
    /// - Buy 订单：price > best_ask → 偏离为 (price - best_ask) / best_ask
    /// - Sell 订单：price < best_bid → 偏离为 (best_bid - price) / best_bid
    /// - 无 best_bid/ask → Allow（无市场参照）
    pub fn evaluate(
        order: &Order,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
    ) -> MatchResult {
        use pm_core::Side;

        match order.side {
            Side::Buy => {
                let Some(best_ask) = best_ask else {
                    return MatchResult::allow();
                };
                if best_ask <= 0.0 {
                    return MatchResult::allow();
                }
                let dev = if order.price > best_ask {
                    (order.price - best_ask) / best_ask
                } else {
                    0.0
                };
                if dev >= REJECT_DEVIATION {
                    MatchResult::reject(
                        &format!(
                            "买价 {:.4} 严重高于最优卖价 {:.4}",
                            order.price, best_ask
                        ),
                        dev,
                    )
                } else if dev >= WARN_DEVIATION {
                    MatchResult::warn(
                        &format!(
                            "买价 {:.4} 偏离最优卖价 {:.4}",
                            order.price, best_ask
                        ),
                        dev,
                    )
                } else {
                    MatchResult::allow()
                }
            }
            Side::Sell => {
                let Some(best_bid) = best_bid else {
                    return MatchResult::allow();
                };
                if best_bid <= 0.0 {
                    return MatchResult::allow();
                }
                let dev = if order.price < best_bid {
                    (best_bid - order.price) / best_bid
                } else {
                    0.0
                };
                if dev >= REJECT_DEVIATION {
                    MatchResult::reject(
                        &format!(
                            "卖价 {:.4} 严重低于最优买价 {:.4}",
                            order.price, best_bid
                        ),
                        dev,
                    )
                } else if dev >= WARN_DEVIATION {
                    MatchResult::warn(
                        &format!(
                            "卖价 {:.4} 偏离最优买价 {:.4}",
                            order.price, best_bid
                        ),
                        dev,
                    )
                } else {
                    MatchResult::allow()
                }
            }
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
    use pm_gateway::{OrderType, TimeInForce};

    fn build_order(side: Side, price: f64) -> Order {
        let now = Local::now();
        Order::new(
            "CLI-1".into(),
            "mkt-1".into(),
            "mock".into(),
            "MockGateway".into(),
            Direction::Yes,
            side,
            price,
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
    fn buy_within_ask_allowed() {
        let o = build_order(Side::Buy, 0.45);
        let r = Matcher::evaluate(&o, Some(0.43), Some(0.46));
        assert_eq!(r.decision, MatchDecision::Allow);
    }

    #[test]
    fn buy_warn_when_slightly_above_ask() {
        let o = build_order(Side::Buy, 0.48); // 比 0.46 高 4.3%
        let r = Matcher::evaluate(&o, Some(0.43), Some(0.46));
        assert_eq!(r.decision, MatchDecision::Warn);
    }

    #[test]
    fn buy_reject_when_far_above_ask() {
        let o = build_order(Side::Buy, 0.60); // 比 0.46 高 30%
        let r = Matcher::evaluate(&o, Some(0.43), Some(0.46));
        assert_eq!(r.decision, MatchDecision::Reject);
    }

    #[test]
    fn sell_within_bid_allowed() {
        let o = build_order(Side::Sell, 0.43);
        let r = Matcher::evaluate(&o, Some(0.43), Some(0.46));
        assert_eq!(r.decision, MatchDecision::Allow);
    }

    #[test]
    fn sell_warn_when_slightly_below_bid() {
        let o = build_order(Side::Sell, 0.41); // 比 0.43 低 4.7%
        let r = Matcher::evaluate(&o, Some(0.43), Some(0.46));
        assert_eq!(r.decision, MatchDecision::Warn);
    }

    #[test]
    fn sell_reject_when_far_below_bid() {
        let o = build_order(Side::Sell, 0.35); // 比 0.43 低 18.6%
        let r = Matcher::evaluate(&o, Some(0.43), Some(0.46));
        assert_eq!(r.decision, MatchDecision::Reject);
    }

    #[test]
    fn no_market_data_allow() {
        let o = build_order(Side::Buy, 0.45);
        let r = Matcher::evaluate(&o, None, None);
        assert_eq!(r.decision, MatchDecision::Allow);
    }

    #[test]
    fn chinese_decision_names() {
        assert_eq!(MatchDecision::Allow.as_zh(), "允许");
        assert_eq!(MatchDecision::Warn.as_zh(), "警告");
        assert_eq!(MatchDecision::Reject.as_zh(), "拒绝");
    }

    #[test]
    fn match_result_summary_chinese() {
        let r = MatchResult::warn("偏离过大", 0.05);
        let s = r.summary_zh();
        assert!(s.contains("警告"));
        assert!(s.contains("5.00%"));
    }
}