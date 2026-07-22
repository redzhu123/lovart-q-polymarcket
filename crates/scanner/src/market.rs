//! 市场扫描：从 [`UnifiedMarket`] 列表中识别潜在套利机会。
//!
//! V1.02 数据层重构：HTTP 拉取已迁至 [`crate::datasource::GammaProvider`]，
//! 本模块**只负责机会识别**，输入由 Gamma 专用 `Market` 改为统一 [`UnifiedMarket`]。
//!
//! 套利判定沿用 V0.2-V0.5（**逻辑不变**）：二元市场 SUM = YES + NO < `threshold` 视为潜在机会。
//! 说明：Gamma 的 outcomePrices 是归一化中间价（YES+NO 恒为 1.0），结构上不会出现 SUM<0.99，
//! 常态下 [`find_opportunities`] 返回空。真实套利需 CLOB Provider 提供真实买卖价
//! （届时只需换数据源，本模块与 Tracker/Recorder 无需改动）。
//!
//! V1.0.1 / V1.01 的可观测性（细分统计 / 拒绝明细 / 随机样本）保持不变，仅字段读取来源
//! 从 `Market.*` 改为 `UnifiedMarket.*`。

use rand::seq::SliceRandom;

use pm_models::{OppSnapshot, UnifiedMarket};

use crate::stats::{
    MarketRejection, MarketSample, RoundAnalysis, RejectionReason,
};

/// 从市场中识别潜在套利机会：active && !closed 且为二元市场且 SUM < `threshold`。
/// 返回按 SUM 升序排列的快照列表（SUM 越低，套利空间越大，排在越前）。
///
/// V1.0.1：委托给 [`analyze_markets_inner`]（单一事实源），保证调试路径与生产路径
/// 使用完全相同的过滤逻辑。V1.02：入参由 `&[Market]` 改为 `&[UnifiedMarket]`，逻辑不变。
pub fn find_opportunities(markets: &[UnifiedMarket], threshold: f64) -> Vec<OppSnapshot> {
    analyze_markets_inner(markets, threshold, false).opportunities
}

/// 单轮市场分析的细分统计 + 拒绝明细 + 样本（debug 路径用）。
///
/// 与 [`find_opportunities`] 共用 [`analyze_markets_inner`]，过滤逻辑完全一致，
/// 额外收集各阶段计数、被过滤市场的拒绝原因、随机 3 个市场样本。
pub fn analyze_markets(markets: &[UnifiedMarket], threshold: f64) -> RoundAnalysis {
    analyze_markets_inner(markets, threshold, true)
}

/// 市场分析的内部实现：单遍迭代，按数据流阶段分类计数。
///
/// `detail=true` 时额外收集 `rejections` / `samples`（调试用）；
/// `detail=false` 时跳过明细收集（无额外开销，保持 debug=false 的性能与 V1.0 一致）。
///
/// 过滤优先级（每个市场恰好落入一类）：
/// `Closed` > `Inactive` > `MissingPrice`/`InvalidData` > `SumAboveThreshold` > 机会。
/// 与 `find_opportunities` 的 `active && !closed && yes_no_prices().is_some() && sum < threshold`
/// 完全等价。
fn analyze_markets_inner(markets: &[UnifiedMarket], threshold: f64, detail: bool) -> RoundAnalysis {
    let mut a = RoundAnalysis::default();
    let mut snaps: Vec<OppSnapshot> = Vec::new();

    for m in markets.iter() {
        a.received += 1;
        a.parsed += 1;
        if m.active() {
            a.active += 1;
        }
        if m.closed() {
            a.closed += 1;
        }

        // ---- 价格提取 ----
        let prices = m.yes_no_prices();
        let yes = m.yes_price;
        let no = m.no_price;
        match &prices {
            Some((y, n)) => {
                a.with_prices += 1;
                a.price_available += 1;
                let sum = y + n;
                if !sum.is_finite() || sum <= 0.0 {
                    a.invalid_sum += 1;
                }
            }
            None => {
                a.missing_prices += 1;
                if yes.is_none() {
                    a.yes_missing += 1;
                }
                if no.is_none() {
                    a.no_missing += 1;
                }
            }
        }

        // ---- 过滤分类（优先级：Closed > Inactive > Missing/Invalid > Strategy）----
        let rejected: Option<RejectionReason> = if m.closed() {
            a.filtered_closed += 1;
            Some(RejectionReason::Closed)
        } else if !m.active() {
            a.filtered_inactive += 1;
            Some(RejectionReason::Inactive)
        } else {
            // active && !closed：按是否有二元价格分流
            match prices {
                None => {
                    if m.outcome_count == 0 {
                        a.filtered_missing_price += 1;
                        Some(RejectionReason::MissingPrice)
                    } else {
                        a.filtered_invalid_data += 1;
                        Some(RejectionReason::InvalidData)
                    }
                }
                Some((y, n)) => {
                    // 通过校验 -> 进入策略判定
                    a.passed_validation += 1;
                    let sum = y + n;
                    if sum < threshold {
                        a.passed_strategy += 1;
                        snaps.push(OppSnapshot {
                            question: m.question.clone(),
                            yes_price: y,
                            no_price: n,
                            sum,
                            volume: m.volume,
                            liquidity: m.liquidity,
                        });
                        None
                    } else {
                        a.filtered_strategy += 1;
                        Some(RejectionReason::SumAboveThreshold)
                    }
                }
            }
        };

        if detail {
            if let Some(reason) = rejected {
                a.rejections.push(MarketRejection {
                    question: m.question.clone(),
                    reason,
                });
            }
        }
    }

    // 随机 3 个市场样本（V1.01 第五节）-- detail 路径收集。
    if detail {
        a.samples = pick_random_samples(markets, 3);
    }

    // NaN 视作相等，避免 sort 异常
    snaps.sort_by(|x, y| x.sum.partial_cmp(&y.sum).unwrap_or(std::cmp::Ordering::Equal));
    a.opportunities = snaps;
    a
}

/// 从市场中随机抽取 `k` 个样本（不足则全取；去重）。
///
/// V1.01 第五节要求"随机打印 3 个 Market"，避免每轮总是同样的最高成交额前 3。
/// 使用 `rand::seq::SliceRandom::shuffle` 打乱索引副本后取前 `k`，保证去重且无偏。
fn pick_random_samples(markets: &[UnifiedMarket], k: usize) -> Vec<MarketSample> {
    if markets.is_empty() || k == 0 {
        return Vec::new();
    }
    let mut idx: Vec<usize> = (0..markets.len()).collect();
    idx.shuffle(&mut rand::rng());
    idx.iter().take(k).map(|&i| MarketSample::from(&markets[i])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    fn market(question: &str, yes: f64, no: f64, active: bool, closed: bool) -> UnifiedMarket {
        let status = if closed {
            MarketStatus::Closed
        } else if active {
            MarketStatus::Active
        } else {
            MarketStatus::Inactive
        };
        UnifiedMarket {
            market_id: question.into(),
            question: question.into(),
            description: None,
            status,
            yes_price: Some(yes),
            no_price: Some(no),
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count: 2,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    fn market_no_price(question: &str, active: bool, closed: bool) -> UnifiedMarket {
        let status = if closed {
            MarketStatus::Closed
        } else if active {
            MarketStatus::Active
        } else {
            MarketStatus::Inactive
        };
        UnifiedMarket {
            market_id: question.into(),
            question: question.into(),
            description: None,
            status,
            yes_price: None,
            no_price: None,
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count: 0,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn find_opportunities_filters_by_threshold_and_binary() {
        // SUM<0.99 才算机会；归一化市场 SUM≈1.0 常态下不入选
        let markets = vec![
            market("Low", 0.40, 0.55, true, false),   // SUM=0.95 -> 机会
            market("Normal", 0.43, 0.57, true, false), // SUM=1.00 -> 非
            market("Closed", 0.30, 0.40, true, true),  // closed -> 非
            market("Inactive", 0.30, 0.40, false, false), // inactive -> 非
        ];
        let snaps = find_opportunities(&markets, 0.99);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].question, "Low");
        assert!((snaps[0].sum - 0.95).abs() < 1e-9);
    }

    #[test]
    fn find_opportunities_sorted_by_sum_asc() {
        let markets = vec![
            market("B", 0.45, 0.50, true, false), // SUM=0.95
            market("A", 0.30, 0.40, true, false), // SUM=0.70
        ];
        let snaps = find_opportunities(&markets, 0.99);
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].question, "A"); // SUM 更低排前
        assert_eq!(snaps[1].question, "B");
    }

    #[test]
    fn find_opportunities_empty_when_all_normalized() {
        // 真实 Gamma 归一化市场：SUM=1.0，阈值 0.99 -> 空
        let markets = vec![market("X", 0.43, 0.57, true, false)];
        assert!(find_opportunities(&markets, 0.99).is_empty());
    }

    #[test]
    fn analyze_matches_find_opportunities() {
        // analyze_markets 产出的机会必须与 find_opportunities 完全一致
        let markets = vec![
            market("Low", 0.40, 0.55, true, false),
            market("Normal", 0.43, 0.57, true, false),
            market("Closed", 0.30, 0.40, true, true),
            market("Inactive", 0.30, 0.40, false, false),
            market_no_price("NoPrice", true, false),
        ];
        let a = analyze_markets(&markets, 0.99);
        let f = find_opportunities(&markets, 0.99);
        assert_eq!(a.opportunities.len(), f.len());
        assert_eq!(a.opportunities[0].question, f[0].question);
        assert_eq!(a.opportunity_count(), 1);
        assert_eq!(a.passed_strategy, 1);
    }

    #[test]
    fn analyze_counts_all_stages() {
        let markets = vec![
            market("Opp", 0.40, 0.55, true, false),      // 机会
            market("Strat", 0.43, 0.57, true, false),     // sum>=threshold -> strategy
            market("Closed", 0.30, 0.40, true, true),     // closed
            market("Inactive", 0.30, 0.40, false, false), // inactive
            market_no_price("NoPrice", true, false),      // missing price
        ];
        let a = analyze_markets(&markets, 0.99);
        assert_eq!(a.received, 5);
        assert_eq!(a.parsed, 5);
        assert_eq!(a.active, 4); // Opp,Strat,Closed,NoPrice
        assert_eq!(a.closed, 1);
        assert_eq!(a.with_prices, 4); // Opp,Strat,Closed,Inactive
        assert_eq!(a.missing_prices, 1);
        // passed_validation = active && !closed && has price: Opp, Strat（NoPrice 无价 -> 不计）
        assert_eq!(a.passed_validation, 2);
        assert_eq!(a.passed_strategy, 1);
        assert_eq!(a.filtered_closed, 1);
        assert_eq!(a.filtered_inactive, 1);
        assert_eq!(a.filtered_missing_price, 1);
        assert_eq!(a.filtered_invalid_data, 0);
        assert_eq!(a.filtered_strategy, 1);
        assert_eq!(a.remaining(), 1);
        assert_eq!(a.price_available, 4);
        assert_eq!(a.yes_missing, 1);
        assert_eq!(a.no_missing, 1);
    }

    #[test]
    fn analyze_rejections_carry_question_and_reason() {
        let markets = vec![
            market("Closed", 0.30, 0.40, true, true),
            market_no_price("NoPrice", true, false),
            market("Strat", 0.43, 0.57, true, false),
        ];
        let a = analyze_markets(&markets, 0.99);
        assert_eq!(a.rejections.len(), 3);
        // 拒绝明细必须带 Question 与原因
        for r in &a.rejections {
            assert!(!r.question.is_empty());
            assert!(!r.reason.as_str().is_empty());
        }
        let reasons: Vec<&str> = a.rejections.iter().map(|r| r.reason.as_str()).collect();
        assert!(reasons.contains(&"已关闭"));
        assert!(reasons.contains(&"缺价"));
        assert!(reasons.contains(&"YES+NO >= 阈值"));
    }

    #[test]
    fn analyze_samples_random_three() {
        // V1.01：样本为随机 3 个（顺序无关），均来自输入集且去重。
        let markets = vec![
            market("Q1", 0.4, 0.5, true, false),
            market("Q2", 0.4, 0.5, true, false),
            market("Q3", 0.4, 0.5, true, false),
            market("Q4", 0.4, 0.5, true, false),
        ];
        let a = analyze_markets(&markets, 0.99);
        assert_eq!(a.samples.len(), 3);
        let names: Vec<&str> = a.samples.iter().map(|s| s.question.as_str()).collect();
        let valid = ["Q1", "Q2", "Q3", "Q4"];
        for n in &names {
            assert!(valid.contains(n), "未知样本: {}", n);
        }
        // 去重
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "样本存在重复: {:?}", names);
    }

    #[test]
    fn analyze_samples_fewer_than_k() {
        // 不足 3 个 -> 全取
        let markets = vec![market("Only", 0.4, 0.5, true, false)];
        let a = analyze_markets(&markets, 0.99);
        assert_eq!(a.samples.len(), 1);
        assert_eq!(a.samples[0].question, "Only");
    }

    #[test]
    fn analyze_empty_markets() {
        let a = analyze_markets(&[], 0.99);
        assert_eq!(a.received, 0);
        assert_eq!(a.opportunity_count(), 0);
        assert!(a.rejections.is_empty());
        assert!(a.samples.is_empty());
    }
}
