//! 订单簿数据合法性校验（V1.03 第十一节）。
//!
//! 检查订单簿数据的基本合法性：
//! - BestBid ≤ BestAsk（买价不能高于卖价）。
//! - Spread ≥ 0（价差不能为负）。
//! - Depth ≥ 0（深度不能为负）。
//! - Price ∈ [0, 1]（价格必须在此范围内）。
//!
//! 任何异常情况打印中文警告日志，但不丢弃数据（由上层决定如何处理）。

use pm_models::OrderBook;

/// 单条校验结果。
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// 市场标识。
    pub market_id: String,
    /// 校验项名称（中文）。
    pub check: String,
    /// 异常详情（中文）。
    pub detail: String,
}

/// 校验报告。
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    /// 校验的订单簿总数。
    pub total: usize,
    /// 通过校验的数量。
    pub passed: usize,
    /// 发现的问题列表。
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// 是否全部通过校验。
    pub fn all_passed(&self) -> bool {
        self.issues.is_empty()
    }

    /// 问题数量。
    pub fn issue_count(&self) -> usize {
        self.issues.len()
    }

    /// 打印中文校验摘要。
    pub fn print_summary(&self) {
        if self.all_passed() {
            tracing::info!(
                total = self.total,
                "订单簿校验：全部 {} 个通过 ✅",
                self.total
            );
        } else {
            tracing::warn!(
                total = self.total,
                passed = self.passed,
                issues = self.issue_count(),
                "订单簿校验：{} 个通过，{} 个存在问题 ⚠️",
                self.passed,
                self.issue_count()
            );
            for issue in &self.issues {
                tracing::warn!(
                    market_id = %issue.market_id,
                    check = %issue.check,
                    detail = %issue.detail,
                    "订单簿校验异常"
                );
            }
        }
    }
}

/// 订单簿校验器（V1.03 第十一节）。
pub struct OrderBookValidator;

impl OrderBookValidator {
    /// 校验单个订单簿，返回发现的问题列表。
    pub fn validate_one(orderbook: &OrderBook) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // 1. 检查 BestBid ≤ BestAsk
        if let (Some(bid), Some(ask)) = (orderbook.best_bid, orderbook.best_ask) {
            if bid > ask {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "买价 ≤ 卖价".into(),
                    detail: format!("BestBid={} 高于 BestAsk={}，价差异常（交叉盘口）", bid, ask),
                });
            }
        }

        // 2. 检查 Spread ≥ 0
        if let Some(spread) = orderbook.spread {
            if spread < 0.0 {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "价差 ≥ 0".into(),
                    detail: format!("Spread={} 为负数", spread),
                });
            }
        }

        // 3. 检查 Depth ≥ 0
        if let Some(bid_depth) = orderbook.bid_depth {
            if bid_depth < 0.0 {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "买盘深度 ≥ 0".into(),
                    detail: format!("BidDepth={} 为负数", bid_depth),
                });
            }
        }
        if let Some(ask_depth) = orderbook.ask_depth {
            if ask_depth < 0.0 {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "卖盘深度 ≥ 0".into(),
                    detail: format!("AskDepth={} 为负数", ask_depth),
                });
            }
        }

        // 4. 检查 Price ∈ [0, 1]（bid_levels）
        for level in &orderbook.bid_levels {
            if !(0.0..=1.0).contains(&level.price) {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "价格 ∈ [0,1]".into(),
                    detail: format!(
                        "买盘 L{} price={} 超出 [0,1] 范围",
                        level.level, level.price
                    ),
                });
            }
        }
        for level in &orderbook.ask_levels {
            if !(0.0..=1.0).contains(&level.price) {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "价格 ∈ [0,1]".into(),
                    detail: format!(
                        "卖盘 L{} price={} 超出 [0,1] 范围",
                        level.level, level.price
                    ),
                });
            }
        }

        // 5. 检查 bid_levels 价格降序排列（最佳在前）
        for w in orderbook.bid_levels.windows(2) {
            if w[0].price < w[1].price {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "买盘价格降序".into(),
                    detail: format!(
                        "买盘 L{} price={} < L{} price={}（应为降序）",
                        w[0].level, w[0].price, w[1].level, w[1].price
                    ),
                });
            }
        }

        // 6. 检查 ask_levels 价格升序排列（最佳在前）
        for w in orderbook.ask_levels.windows(2) {
            if w[0].price > w[1].price {
                issues.push(ValidationIssue {
                    market_id: orderbook.market_id.clone(),
                    check: "卖盘价格升序".into(),
                    detail: format!(
                        "卖盘 L{} price={} > L{} price={}（应为升序）",
                        w[0].level, w[0].price, w[1].level, w[1].price
                    ),
                });
            }
        }

        issues
    }

    /// 批量校验订单簿，返回完整报告。
    pub fn validate_all(orderbooks: &[OrderBook]) -> ValidationReport {
        let total = orderbooks.len();
        let mut all_issues: Vec<ValidationIssue> = Vec::new();

        for ob in orderbooks {
            let issues = Self::validate_one(ob);
            if issues.is_empty() {
                // 通过
            } else {
                all_issues.extend(issues);
            }
        }

        let passed = total - all_issues.len();

        ValidationReport {
            total,
            passed,
            issues: all_issues,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::PriceLevel;

    /// 构造一个合法的订单簿用于测试。
    fn valid_ob() -> OrderBook {
        OrderBook {
            market_id: "test-valid".into(),
            best_bid: Some(0.45),
            best_ask: Some(0.47),
            spread: Some(0.02),
            bid_depth: Some(350.0),
            ask_depth: Some(200.0),
            bid_levels: vec![
                PriceLevel {
                    price: 0.45,
                    size: 100.0,
                    level: 1,
                },
                PriceLevel {
                    price: 0.44,
                    size: 200.0,
                    level: 2,
                },
            ],
            ask_levels: vec![
                PriceLevel {
                    price: 0.47,
                    size: 80.0,
                    level: 1,
                },
                PriceLevel {
                    price: 0.48,
                    size: 120.0,
                    level: 2,
                },
            ],
            bid_volume: 300.0,
            ask_volume: 200.0,
            timestamp: Utc::now(),
            provider: "test".into(),
        }
    }

    #[test]
    fn valid_orderbook_passes_all_checks() {
        let issues = OrderBookValidator::validate_one(&valid_ob());
        assert!(issues.is_empty());
    }

    #[test]
    fn cross_spread_detected() {
        let mut ob = valid_ob();
        ob.best_bid = Some(0.50);
        ob.best_ask = Some(0.45); // bid > ask -> 交叉盘口
        ob.spread = OrderBook::compute_spread(ob.best_bid, ob.best_ask);
        let issues = OrderBookValidator::validate_one(&ob);
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.check.contains("买价")));
    }

    #[test]
    fn negative_spread_detected() {
        let mut ob = valid_ob();
        ob.spread = Some(-0.01);
        let issues = OrderBookValidator::validate_one(&ob);
        assert!(issues.iter().any(|i| i.check.contains("价差")));
    }

    #[test]
    fn negative_depth_detected() {
        let mut ob = valid_ob();
        ob.bid_depth = Some(-100.0);
        let issues = OrderBookValidator::validate_one(&ob);
        assert!(issues.iter().any(|i| i.check.contains("买盘深度")));
    }

    #[test]
    fn price_out_of_range_detected() {
        let mut ob = valid_ob();
        // 添加一个超出 [0,1] 的买盘价格
        ob.bid_levels.push(PriceLevel {
            price: 1.5,
            size: 10.0,
            level: 3,
        });
        let issues = OrderBookValidator::validate_one(&ob);
        assert!(issues.iter().any(|i| i.check.contains("价格")));
    }

    #[test]
    fn bid_order_descending_check() {
        let mut ob = valid_ob();
        // 乱序：0.44 在 0.45 之后（应为降序）
        ob.bid_levels = vec![
            PriceLevel {
                price: 0.44,
                size: 100.0,
                level: 1,
            },
            PriceLevel {
                price: 0.45,
                size: 200.0,
                level: 2,
            },
        ];
        // 更新 best_bid 为第一档
        ob.best_bid = Some(0.44);
        ob.spread = OrderBook::compute_spread(ob.best_bid, ob.best_ask);
        let issues = OrderBookValidator::validate_one(&ob);
        assert!(
            issues.iter().any(|i| i.check.contains("买盘价格降序")),
            "应检测到买盘价格非降序排列"
        );
    }

    #[test]
    fn ask_order_ascending_check() {
        let mut ob = valid_ob();
        // 乱序：0.48 在 0.47 之后（应为升序）
        ob.ask_levels = vec![
            PriceLevel {
                price: 0.48,
                size: 100.0,
                level: 1,
            },
            PriceLevel {
                price: 0.47,
                size: 200.0,
                level: 2,
            },
        ];
        ob.best_ask = Some(0.48);
        ob.spread = OrderBook::compute_spread(ob.best_bid, ob.best_ask);
        let issues = OrderBookValidator::validate_one(&ob);
        assert!(
            issues.iter().any(|i| i.check.contains("卖盘价格升序")),
            "应检测到卖盘价格非升序排列"
        );
    }

    #[test]
    fn empty_orderbook_passes() {
        let ob = OrderBook::empty("empty", "test");
        let issues = OrderBookValidator::validate_one(&ob);
        assert!(issues.is_empty());
    }

    #[test]
    fn validate_all_produces_report() {
        let mut orders = vec![valid_ob(), valid_ob()];
        // 第三个订单簿有异常
        let mut bad = valid_ob();
        bad.market_id = "bad-one".into();
        bad.spread = Some(-0.05);
        orders.push(bad);

        let report = OrderBookValidator::validate_all(&orders);
        assert_eq!(report.total, 3);
        // 只有 bad-one 有 1 个 issue
        assert_eq!(report.issue_count(), 1);
    }
}
