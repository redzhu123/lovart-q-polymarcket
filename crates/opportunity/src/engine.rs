//! 机会引擎编排器（V1.04 第一节）。
//!
//! [`OpportunityEngine`] 是 V1.04 的核心编排器：
//! ```text
//! UnifiedMarket + OrderBook → Opportunity（评分/分类/过滤/排序）
//! ```
//!
//! Scanner 调用 `engine.analyze(markets, orderbooks)` 获取机会列表，
//! 然后传给 Strategy。Strategy 不再直接分析 Market。

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use pm_models::{OrderBook, ProviderCapability, UnifiedMarket};

use crate::confidence::ConfidenceEngine;
use crate::model::{Opportunity, OpportunityStatus, OpportunityType};
use crate::queue::OpportunityQueue;
use crate::score::{OpportunityScore, ScoreResult};

/// 引擎配置。
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 机会 SUM 阈值（YES+NO < threshold 才纳入分析）。
    pub opportunity_threshold: f64,
    /// 最小保留评分（低于此值过滤）。
    pub min_score: f64,
    /// 队列最大容量。
    pub max_opportunities: usize,
    /// 机会 TTL（秒），未在 TTL 内再次出现则过期。
    pub ttl_secs: u64,
    /// 高优先级阈值。
    pub high_priority_threshold: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            opportunity_threshold: 0.99,
            min_score: 20.0,
            max_opportunities: 100,
            ttl_secs: 120,
            high_priority_threshold: 80.0,
        }
    }
}

/// 引擎单次处理输出。
#[derive(Debug)]
pub struct EngineOutput {
    /// 本轮发现 / 更新的机会（按 Score 降序）。
    pub opportunities: Vec<Opportunity>,
    /// 本轮新发现的机会数。
    pub new_count: usize,
    /// 本轮更新的机会数。
    pub updated_count: usize,
    /// 本轮过期的机会。
    pub expired: Vec<Opportunity>,
    /// 被过滤的机会数（低于 min_score）。
    pub filtered_count: usize,
}

/// 机会引擎：Market → Opportunity 的核心编排器。
pub struct OpportunityEngine {
    config: EngineConfig,
    scorer: OpportunityScore,
    confidence_engine: ConfidenceEngine,
    /// 活跃机会状态（key = market_id）。
    state: HashMap<String, TrackedState>,
    /// 本轮处理后的机会队列。
    queue: OpportunityQueue,
    /// 累计机会 ID 计数器（用于生成唯一 ID，保留供未来扩展）。
    #[allow(dead_code)]
    id_counter: u64,
}

/// 内部追踪状态：记录每个机会的持续轮数与创建时间。
#[derive(Debug, Clone)]
struct TrackedState {
    /// 该机会首次出现的时间（UTC）。
    first_seen: DateTime<Utc>,
    /// 最后出现的时间。
    last_seen: DateTime<Utc>,
    /// 累计出现的扫描轮数。
    scan_count: u64,
    /// 上一轮的评分（用于稳定性判定）。
    last_score: f64,
    /// 上一轮的置信度。
    last_confidence: f64,
    /// 机会类型（跨轮保持一致）。
    opportunity_type: OpportunityType,
}

impl OpportunityEngine {
    /// 使用默认配置创建引擎。
    pub fn new() -> Self {
        Self::with_config(EngineConfig::default())
    }

    /// 使用自定义配置创建引擎。
    pub fn with_config(config: EngineConfig) -> Self {
        Self {
            config,
            scorer: OpportunityScore::new(),
            confidence_engine: ConfidenceEngine::new(),
            state: HashMap::new(),
            queue: OpportunityQueue::new(EngineConfig::default().max_opportunities),
            id_counter: 0,
        }
    }

    /// 从 `pm_models::Config` 创建引擎。
    pub fn from_pm_config(cfg: &pm_models::Config) -> Self {
        let config = EngineConfig {
            opportunity_threshold: cfg.scanner.opportunity_threshold,
            ..EngineConfig::default()
        };
        Self::with_config(config)
    }

    /// 核心方法：分析市场 + 订单簿，产出机会列表。
    ///
    /// `markets`：来自 DataSourceManager 的统一市场列表。
    /// `orderbooks`：可选订单簿（key = market_id），Provider 不支持时传空 HashMap。
    /// `provider_cap`：Provider 能力声明。
    /// `now`：当前时间（UTC）。
    pub fn analyze(
        &mut self,
        markets: &[UnifiedMarket],
        orderbooks: &HashMap<String, OrderBook>,
        _provider_cap: &ProviderCapability,
        now: DateTime<Utc>,
    ) -> EngineOutput {
        let mut new_count = 0usize;
        let mut updated_count = 0usize;
        let mut filtered_count = 0usize;
        let mut current_market_ids: Vec<String> = Vec::new();

        // 清空本轮队列
        self.queue.clear();

        for market in markets {
            // 1. 基础过滤：非活跃 / 已关闭 / 无价格 → 跳过
            if !market.active() || market.closed() || !market.has_prices() {
                continue;
            }

            let (yes, no) = match market.yes_no_prices() {
                Some(p) => p,
                None => continue,
            };
            let sum = yes + no;

            // 2. SUM 阈值过滤
            if sum >= self.config.opportunity_threshold {
                continue;
            }

            current_market_ids.push(market.market_id.clone());

            // 3. 查找关联订单簿
            let ob = orderbooks.get(&market.market_id);

            // 4. 分类
            let opp_type =
                OpportunityType::classify(sum, ob.and_then(|o| o.spread), market.liquidity);

            // 5. 获取历史追踪状态
            let tracked = self.state.get(&market.market_id);
            let historical_rounds = tracked.map(|t| t.scan_count).unwrap_or(0);
            let is_new = tracked.is_none();

            // 6. 计算置信度
            let has_orderbook = ob.is_some();
            let has_bid_ask = ob
                .map(|o| o.best_bid.is_some() && o.best_ask.is_some())
                .unwrap_or(false);
            let confidence = self.confidence_engine.evaluate(
                market,
                has_orderbook,
                has_bid_ask,
                historical_rounds,
            );

            // 7. 评分
            let score_result: ScoreResult = self.scorer.compute(
                sum,
                yes,
                market.liquidity,
                market.volume,
                ob.and_then(|o| o.bid_depth),
                ob.and_then(|o| o.ask_depth),
                confidence,
            );

            // 8. 过滤低分
            if score_result.total < self.config.min_score {
                filtered_count += 1;
                continue;
            }

            // 9. 计算优先级（score + confidence 综合映射）
            let priority = Self::compute_priority(score_result.total, confidence);

            // 10. 计算预期收益
            let expected_roi = if sum < 1.0 { (1.0 - sum) / sum } else { 0.0 };
            let expected_profit = expected_roi * 100.0; // 假设 100 USDC 名义本金

            // 11. 构建 Opportunity
            let opp = Opportunity::new(
                market.market_id.clone(),
                market.question.clone(),
                market.provider.clone(),
                now,
                opp_type,
                score_result.total,
                confidence,
                priority,
                score_result.spread,
                score_result.liquidity,
                score_result.depth,
                score_result.volume,
                score_result.volatility,
                score_result.risk,
                expected_roi,
                expected_profit,
                yes,
                no,
                sum,
                ob.and_then(|o| o.spread),
                market.volume,
                market.liquidity,
                ob.and_then(|o| o.bid_depth),
                ob.and_then(|o| o.ask_depth),
            );

            // 12. 更新追踪状态
            if is_new {
                self.state.insert(
                    market.market_id.clone(),
                    TrackedState {
                        first_seen: now,
                        last_seen: now,
                        scan_count: 1,
                        last_score: score_result.total,
                        last_confidence: confidence,
                        opportunity_type: opp_type,
                    },
                );
                new_count += 1;
            } else if let Some(t) = self.state.get_mut(&market.market_id) {
                t.last_seen = now;
                t.scan_count += 1;
                t.last_score = score_result.total;
                t.last_confidence = confidence;
                updated_count += 1;
            }

            // 13. 入队
            self.queue.push(opp);
        }

        // 14. 清理过期机会（超过 TTL 未出现）
        let expired = self.reap_expired(&current_market_ids, now);

        EngineOutput {
            opportunities: self.queue.all().to_vec(),
            new_count,
            updated_count,
            expired,
            filtered_count,
        }
    }

    /// 获取 Top-N 机会。
    pub fn top_n(&self, n: usize) -> &[Opportunity] {
        self.queue.top_n(n)
    }

    /// 获取所有活跃机会。
    pub fn all(&self) -> &[Opportunity] {
        self.queue.all()
    }

    /// 活跃状态数。
    pub fn state_count(&self) -> usize {
        self.state.len()
    }

    /// 计算优先级（0~100）。
    ///
    /// 综合 score 与 confidence 映射：
    /// `priority = score * 0.6 + confidence * 100 * 0.4`
    fn compute_priority(score: f64, confidence: f64) -> u8 {
        let p = score * 0.6 + confidence * 100.0 * 0.4;
        (p.clamp(0.0, 100.0)) as u8
    }

    /// 清理超过 TTL 未再出现的机会。
    fn reap_expired(&mut self, current_ids: &[String], now: DateTime<Utc>) -> Vec<Opportunity> {
        let ttl = chrono::Duration::seconds(self.config.ttl_secs as i64);
        let mut expired_ids: Vec<String> = Vec::new();

        // 找到过期或消失的 market_id
        for (id, state) in &self.state {
            let elapsed = now - state.last_seen;
            let is_current = current_ids.contains(id);
            if !is_current && elapsed > ttl {
                expired_ids.push(id.clone());
            }
        }

        let mut expired_opps: Vec<Opportunity> = Vec::new();
        for id in &expired_ids {
            if let Some(state) = self.state.remove(id) {
                // 构建一个标记为 Expired 的 Opportunity 用于通知 Strategy
                let mut opp = Opportunity::new(
                    id.clone(),
                    id.clone(),    // question 已丢失，折中用 ID
                    String::new(), // provider
                    state.first_seen,
                    state.opportunity_type,
                    state.last_score,
                    state.last_confidence,
                    Self::compute_priority(state.last_score, state.last_confidence),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    None,
                    0.0,
                    0.0,
                    None,
                    None,
                );
                opp.status = OpportunityStatus::Expired;
                expired_opps.push(opp);
            }
        }

        expired_opps
    }
}

impl Default for OpportunityEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    fn test_market(
        id: &str,
        question: &str,
        yes: f64,
        no: f64,
        liquidity: f64,
        volume: f64,
    ) -> UnifiedMarket {
        UnifiedMarket {
            market_id: id.into(),
            question: question.into(),
            description: None,
            status: MarketStatus::Active,
            yes_price: Some(yes),
            no_price: Some(no),
            volume,
            liquidity,
            category: None,
            outcome_count: 2,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    fn test_capability() -> ProviderCapability {
        ProviderCapability {
            supports_markets: true,
            supports_orderbook: false,
            supports_trades: false,
            supports_bid_ask: false,
            supports_liquidity: true,
            depth_levels: 0,
            supports_depth: false,
        }
    }

    #[test]
    fn engine_finds_arbitrage_opportunity() {
        let markets = vec![
            test_market("m1", "测试套利", 0.40, 0.50, 10000.0, 50000.0), // SUM=0.90 → Arbitrage
            test_market("m2", "正常市场", 0.48, 0.52, 5000.0, 20000.0),  // SUM=1.00 → skip
        ];
        let mut engine = OpportunityEngine::new();
        let output = engine.analyze(&markets, &HashMap::new(), &test_capability(), Utc::now());
        assert_eq!(output.opportunities.len(), 1);
        assert_eq!(output.new_count, 1);
        let opp = &output.opportunities[0];
        assert_eq!(opp.market_id, "m1");
        assert_eq!(opp.opportunity_type, OpportunityType::Spread); // 0.90 → Spread (not < 0.90 → Spread)
        // 0.90 is not < 0.90, so it's Spread (0.90 ≤ SUM < 0.98)
    }

    #[test]
    fn engine_filters_below_min_score() {
        // SUM=0.90 可通过阈值 (<0.99)，但流动性极低导致评分不达标
        let markets = vec![test_market("m1", "低质量", 0.40, 0.50, 1.0, 1.0)];
        let mut config = EngineConfig::default();
        config.min_score = 90.0; // 苛刻过滤
        let mut engine = OpportunityEngine::with_config(config);
        let output = engine.analyze(&markets, &HashMap::new(), &test_capability(), Utc::now());
        // 评分应低于 90，被过滤
        assert!(output.opportunities.is_empty());
        assert!(output.filtered_count >= 1);
    }

    #[test]
    fn engine_tracks_updates_across_rounds() {
        let markets = vec![test_market("m1", "Q", 0.40, 0.50, 10000.0, 50000.0)];
        let mut engine = OpportunityEngine::new();
        let now = Utc::now();

        // Round 1
        let out1 = engine.analyze(&markets, &HashMap::new(), &test_capability(), now);
        assert_eq!(out1.new_count, 1);
        assert_eq!(out1.updated_count, 0);

        // Round 2: same market
        let out2 = engine.analyze(&markets, &HashMap::new(), &test_capability(), now);
        assert_eq!(out2.new_count, 0);
        assert_eq!(out2.updated_count, 1);
    }

    #[test]
    fn engine_skips_closed_and_inactive() {
        let markets = vec![
            UnifiedMarket {
                market_id: "closed".into(),
                question: "已关闭".into(),
                description: None,
                status: MarketStatus::Closed,
                yes_price: Some(0.40),
                no_price: Some(0.50),
                volume: 1000.0,
                liquidity: 1000.0,
                category: None,
                outcome_count: 2,
                provider: "test".into(),
                updated_at: Utc::now(),
            },
            UnifiedMarket {
                market_id: "inactive".into(),
                question: "不活跃".into(),
                description: None,
                status: MarketStatus::Inactive,
                yes_price: Some(0.40),
                no_price: Some(0.50),
                volume: 1000.0,
                liquidity: 1000.0,
                category: None,
                outcome_count: 2,
                provider: "test".into(),
                updated_at: Utc::now(),
            },
        ];
        let mut engine = OpportunityEngine::new();
        let output = engine.analyze(&markets, &HashMap::new(), &test_capability(), Utc::now());
        assert!(output.opportunities.is_empty());
    }
}
