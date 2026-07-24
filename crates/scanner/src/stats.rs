//! pm-scanner::stats：扫描可观测性（Observability）数据结构。
//!
//! V1.0.1 新增。**只增强可观测性，不改变任何交易/策略/Shadow/Execution 逻辑。**
//!
//! 三层统计：
//! - [`PageStats`] / [`FetchStats`] / [`FetchResult`]：HTTP 拉取的请求级统计
//!   （URL / 状态码 / 字节数 / 耗时 / 错误），用于定位 "API 请求失败" 与 "JSON 解析失败"。
//! - [`RoundAnalysis`]：单轮市场分析的细分统计（接收 / 解析 / 活跃 / 价格 / 策略各阶段计数
//!   + 拒绝明细 + 前 3 个样本），用于定位 "Market 为 0 / Active 为 0 / Price 缺失 / 策略过滤"。
//! - [`ScannerStats`]：跨轮累计统计（请求数 / 成功数 / 失败数 / 市场数 / 机会数），
//!   由 driver 持有，每轮更新，统一打印。
//!
//! Simulation Only -- 仅用于诊断"为何 Active Opportunities = 0"，不参与交易决策。

use pm_models::{OppSnapshot, UnifiedMarket};

// ============================================================================
// HTTP 拉取统计
// ============================================================================

/// 单次分页请求的统计（一页一个）。
#[derive(Debug, Clone, Default)]
pub struct PageStats {
    /// 请求 URL（含 limit/offset）。
    pub url: String,
    /// HTTP 状态码（请求未到达服务端时为 0）。
    pub status: u16,
    /// 响应体字节数。
    pub bytes: usize,
    /// 本次请求耗时（毫秒）。
    pub elapsed_ms: u128,
    /// 是否成功（2xx 或 422 末尾）。
    pub ok: bool,
    /// 失败时的完整错误信息。
    pub error: Option<String>,
}

/// 一次 `fetch_active_markets` 的聚合统计（跨所有分页）。
#[derive(Debug, Clone, Default)]
pub struct FetchStats {
    /// 总请求数（分页数）。
    pub request_count: u64,
    /// 成功请求数。
    pub success_count: u64,
    /// 失败请求数。
    pub failed_count: u64,
    /// 累计响应字节数。
    pub total_bytes: u64,
    /// 累计 HTTP 耗时（毫秒，含网络 + 读取响应体，不含反序列化）。
    pub total_ms: u128,
    /// 累计 JSON 反序列化耗时（毫秒）-- V1.01 第六节 API Diagnostics。
    pub deserialize_ms: u128,
    /// 最后一次请求的状态码。
    pub last_status: u16,
    /// 最后一次错误信息（若有）。
    pub last_error: Option<String>,
    /// 首页 URL（调试打印用，展示请求的目标地址）。
    pub first_url: Option<String>,
    /// Rate-Limit 头（若服务端返回，如 `x-ratelimit-remaining`）-- V1.01 第六节。
    pub rate_limit: Option<String>,
    /// 逐页明细（调试打印用）。
    pub pages: Vec<PageStats>,
}

/// `fetch_active_markets` 的返回：市场列表 + 拉取统计。
///
/// V1.02：`markets` 类型由 Gamma 专用 `Market` 改为统一 `UnifiedMarket`。
pub struct FetchResult {
    pub markets: Vec<UnifiedMarket>,
    pub stats: FetchStats,
}

// ============================================================================
// 市场分析统计（单轮）
// ============================================================================

/// 单个市场被过滤的拒绝原因。
///
/// 与 `find_opportunities` 的过滤条件一一对应：
/// - `active && !closed` 不成立 -> [`RejectionReason::Closed`] / [`RejectionReason::Inactive`]
/// - `yes_no_prices()` 为 None -> [`RejectionReason::MissingPrice`] / [`RejectionReason::InvalidData`]
/// - `sum >= threshold` -> [`RejectionReason::SumAboveThreshold`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    /// `closed == true`（被 `!closed` 过滤）。
    Closed,
    /// `active == false`（被 `active` 过滤）。
    Inactive,
    /// `outcome_prices` 缺失（None）。
    MissingPrice,
    /// `outcome_prices` 存在但无法解析 / 非二元（len != 2）。
    InvalidData,
    /// 通过校验但 `YES+NO >= threshold`（无套利空间）。
    SumAboveThreshold,
}

impl RejectionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            RejectionReason::Closed => "已关闭",
            RejectionReason::Inactive => "不活跃",
            RejectionReason::MissingPrice => "缺价",
            RejectionReason::InvalidData => "数据无效",
            RejectionReason::SumAboveThreshold => "YES+NO >= 阈值",
        }
    }
}

/// 单个被过滤市场的明细（Question + 原因），用于调试打印定位。
#[derive(Debug, Clone)]
pub struct MarketRejection {
    pub question: String,
    pub reason: RejectionReason,
}

/// 调试用的市场样本（随机 3 个），用于确认字段是否正确解析。
#[derive(Debug, Clone)]
pub struct MarketSample {
    pub question: String,
    pub active: bool,
    pub closed: bool,
    pub volume: f64,
    pub liquidity: f64,
    /// outcome 数量（outcome_prices 数组长度；缺失为 0）-- V1.01 第五节 Outcome Count。
    pub outcome_count: usize,
    /// 二元 (YES, NO) 价格；缺失或非二元为 None。
    pub price: Option<(f64, f64)>,
}

impl From<&UnifiedMarket> for MarketSample {
    fn from(m: &UnifiedMarket) -> Self {
        MarketSample {
            question: m.question.clone(),
            active: m.active(),
            closed: m.closed(),
            volume: m.volume,
            liquidity: m.liquidity,
            outcome_count: m.outcome_count,
            price: m.yes_no_prices(),
        }
    }
}

/// 单轮市场分析的细分统计。
///
/// 字段对齐 V1.0.1 调试输出：接收 -> 解析 -> 活跃/关闭 -> 价格 -> 校验 -> 策略 -> 机会
/// 的完整数据流，每一步都有计数，任一项为 0 都能被一眼看到。
#[derive(Debug, Clone, Default)]
pub struct RoundAnalysis {
    /// Markets Received：本轮拉取到的市场总数。
    pub received: usize,
    /// Markets Parsed：serde 解析成功后的市场数（与 received 相等，除非中途解析失败已 return）。
    pub parsed: usize,
    /// Active Markets：`active == true` 的市场数。
    pub active: usize,
    /// Closed Markets：`closed == true` 的市场数。
    pub closed: usize,
    /// Markets With Prices：`yes_no_prices()` 成功的市场数。
    pub with_prices: usize,
    /// Markets Missing Prices：`yes_no_prices()` 失败的市场数。
    pub missing_prices: usize,
    /// Markets Passed Validation：`active && !closed && has price` 的市场数。
    pub passed_validation: usize,
    /// Markets Passed Strategy：通过 `sum < threshold` 的市场数（= opportunities.len()）。
    pub passed_strategy: usize,
    /// Possible Opportunities：本轮发现的机会快照（按 SUM 升序）。
    pub opportunities: Vec<OppSnapshot>,

    // ---- 过滤原因细分 ----
    /// Filtered By Closed。
    pub filtered_closed: usize,
    /// Filtered By Inactive。
    pub filtered_inactive: usize,
    /// Filtered By Missing Price。
    pub filtered_missing_price: usize,
    /// Filtered By Invalid Data（解析失败 / 非二元）。
    pub filtered_invalid_data: usize,
    /// Filtered By Strategy（sum >= threshold）。
    pub filtered_strategy: usize,

    // ---- 价格统计 ----
    /// Price Available：有可用 (YES, NO) 价格的市场数（= with_prices）。
    pub price_available: usize,
    /// YES Price Missing：无法取到 YES 价的市场数。
    pub yes_missing: usize,
    /// NO Price Missing：无法取到 NO 价的市场数。
    pub no_missing: usize,
    /// Invalid Sum：YES+NO 非有限或 <= 0 的市场数。
    pub invalid_sum: usize,

    // ---- 调试明细 ----
    /// 被过滤市场的拒绝明细（按优先级排序：Missing/Invalid > Inactive > Closed > Strategy）。
    pub rejections: Vec<MarketRejection>,
    /// 随机 3 个市场样本（V1.01 第五节；不足 3 个则全取）。
    pub samples: Vec<MarketSample>,
}

impl RoundAnalysis {
    /// Remaining = Possible Opportunities。
    pub fn remaining(&self) -> usize {
        self.opportunities.len()
    }

    /// Possible Opportunities 计数。
    pub fn opportunity_count(&self) -> usize {
        self.opportunities.len()
    }
}

// ============================================================================
// 跨轮累计统计（会话级）
// ============================================================================

/// 跨轮累计统计，由 driver 持有，每轮更新，统一打印。
///
/// 字段对齐 V1.0.1 第八节要求。
#[derive(Debug, Clone, Default)]
pub struct ScannerStats {
    /// 累计 HTTP 请求数。
    pub request_count: u64,
    /// 累计 HTTP 成功数。
    pub success_count: u64,
    /// 累计 HTTP 失败数。
    pub failed_count: u64,
    /// 累计 Markets Received。
    pub market_count: u64,
    /// 累计 Markets Parsed。
    pub parsed_count: u64,
    /// 累计 Active Markets。
    pub active_count: u64,
    /// 累计 Markets Missing Prices。
    pub missing_price_count: u64,
    /// 累计 Filtered By Strategy。
    pub strategy_rejected_count: u64,
    /// 累计 Possible Opportunities。
    pub opportunity_count: u64,
    /// 累计扫描轮次。
    pub round_count: u64,
}

impl ScannerStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录完成一轮扫描。
    pub fn record_round(&mut self) {
        self.round_count += 1;
    }

    /// 累加一轮的 HTTP 拉取统计。
    pub fn add_fetch(&mut self, fetch: &FetchStats) {
        self.request_count += fetch.request_count;
        self.success_count += fetch.success_count;
        self.failed_count += fetch.failed_count;
    }

    /// 累加一轮的市场分析统计。
    pub fn add_round(&mut self, a: &RoundAnalysis) {
        self.market_count += a.received as u64;
        self.parsed_count += a.parsed as u64;
        self.active_count += a.active as u64;
        self.missing_price_count += a.missing_prices as u64;
        self.strategy_rejected_count += a.filtered_strategy as u64;
        self.opportunity_count += a.opportunity_count() as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    /// 构造二元 `UnifiedMarket`（yes/no 给定则 outcome_count=2，否则 0）。
    fn market(
        question: &str,
        yes: Option<f64>,
        no: Option<f64>,
        active: bool,
        closed: bool,
    ) -> UnifiedMarket {
        let status = if closed {
            MarketStatus::Closed
        } else if active {
            MarketStatus::Active
        } else {
            MarketStatus::Inactive
        };
        let outcome_count = if yes.is_some() && no.is_some() { 2 } else { 0 };
        UnifiedMarket {
            market_id: question.into(),
            question: question.into(),
            description: None,
            status,
            yes_price: yes,
            no_price: no,
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn scanner_stats_accumulates() {
        let mut s = ScannerStats::new();
        let fetch = FetchStats {
            request_count: 21,
            success_count: 21,
            ..Default::default()
        };
        s.add_fetch(&fetch);

        let a = RoundAnalysis {
            received: 100,
            parsed: 100,
            active: 90,
            missing_prices: 5,
            filtered_strategy: 80,
            opportunities: vec![OppSnapshot {
                question: "Q".into(),
                yes_price: 0.4,
                no_price: 0.5,
                sum: 0.9,
                volume: 0.0,
                liquidity: 0.0,
            }],
            ..Default::default()
        };
        s.add_round(&a);
        s.record_round();

        assert_eq!(s.request_count, 21);
        assert_eq!(s.success_count, 21);
        assert_eq!(s.market_count, 100);
        assert_eq!(s.active_count, 90);
        assert_eq!(s.missing_price_count, 5);
        assert_eq!(s.strategy_rejected_count, 80);
        assert_eq!(s.opportunity_count, 1);
        assert_eq!(s.round_count, 1);
    }

    #[test]
    fn rejection_reason_strings() {
        assert_eq!(RejectionReason::Closed.as_str(), "已关闭");
        assert_eq!(RejectionReason::Inactive.as_str(), "不活跃");
        assert_eq!(RejectionReason::MissingPrice.as_str(), "缺价");
        assert_eq!(RejectionReason::InvalidData.as_str(), "数据无效");
        assert_eq!(
            RejectionReason::SumAboveThreshold.as_str(),
            "YES+NO >= 阈值"
        );
    }

    #[test]
    fn market_sample_from_market() {
        let m = market("Q", Some(0.43), Some(0.57), true, false);
        let s = MarketSample::from(&m);
        assert_eq!(s.question, "Q");
        assert!(s.active);
        assert!(!s.closed);
        assert_eq!(s.outcome_count, 2);
        assert_eq!(s.price, Some((0.43, 0.57)));
    }

    #[test]
    fn market_sample_missing_price() {
        let m = market("Q", None, None, true, false);
        let s = MarketSample::from(&m);
        assert_eq!(s.price, None);
        assert_eq!(s.outcome_count, 0);
    }

    #[test]
    fn market_sample_multi_outcome_count() {
        // 三元市场：outcome_count=3，price=None（非二元）
        let m = UnifiedMarket {
            market_id: "Q".into(),
            question: "Q".into(),
            description: None,
            status: MarketStatus::Active,
            yes_price: None,
            no_price: None,
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count: 3,
            provider: "test".into(),
            updated_at: Utc::now(),
        };
        let s = MarketSample::from(&m);
        assert_eq!(s.outcome_count, 3);
        assert_eq!(s.price, None);
    }
}
