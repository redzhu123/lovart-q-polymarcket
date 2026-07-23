//! 机会 CSV 持久化（V1.04 第十四节）。
//!
//! 保存和加载 Opportunity 到 CSV 文件，供 Python 分析使用。
//! 字段保持英文，日志全部中文。

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{Opportunity, OpportunityStatus, OpportunityType};

/// opportunity.csv 表头（英文字段）。
pub const OPPORTUNITY_CSV_HEADER: &[&str] = &[
    "id",
    "market_id",
    "question",
    "provider",
    "detected_time",
    "opportunity_type",
    "status",
    "score",
    "confidence",
    "priority",
    "spread_score",
    "liquidity_score",
    "depth_score",
    "volume_score",
    "volatility_score",
    "risk_score",
    "expected_roi",
    "expected_profit",
    "yes_price",
    "no_price",
    "sum",
    "spread",
    "volume",
    "liquidity",
    "bid_depth",
    "ask_depth",
    "version",
];

/// CSV 序列化记录（扁平化 Opportunity 为一行）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityRecord {
    pub id: String,
    pub market_id: String,
    pub question: String,
    pub provider: String,
    pub detected_time: String,
    pub opportunity_type: String,
    pub status: String,
    pub score: f64,
    pub confidence: f64,
    pub priority: u8,
    pub spread_score: f64,
    pub liquidity_score: f64,
    pub depth_score: f64,
    pub volume_score: f64,
    pub volatility_score: f64,
    pub risk_score: f64,
    pub expected_roi: f64,
    pub expected_profit: f64,
    pub yes_price: f64,
    pub no_price: f64,
    pub sum: f64,
    pub spread: Option<f64>,
    pub volume: f64,
    pub liquidity: f64,
    pub bid_depth: Option<f64>,
    pub ask_depth: Option<f64>,
    pub version: u32,
}

impl From<&Opportunity> for OpportunityRecord {
    fn from(opp: &Opportunity) -> Self {
        Self {
            id: opp.id.clone(),
            market_id: opp.market_id.clone(),
            question: opp.question.clone(),
            provider: opp.provider.clone(),
            detected_time: opp.detected_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            opportunity_type: format!("{:?}", opp.opportunity_type),
            status: format!("{:?}", opp.status),
            score: opp.score,
            confidence: opp.confidence,
            priority: opp.priority,
            spread_score: opp.spread_score,
            liquidity_score: opp.liquidity_score,
            depth_score: opp.depth_score,
            volume_score: opp.volume_score,
            volatility_score: opp.volatility_score,
            risk_score: opp.risk_score,
            expected_roi: opp.expected_roi,
            expected_profit: opp.expected_profit,
            yes_price: opp.yes_price,
            no_price: opp.no_price,
            sum: opp.sum,
            spread: opp.spread,
            volume: opp.volume,
            liquidity: opp.liquidity,
            bid_depth: opp.bid_depth,
            ask_depth: opp.ask_depth,
            version: opp.version,
        }
    }
}

/// 确保 CSV 文件就绪（目录 + 表头）。
/// 委托给 pm-storage 的通用函数。
pub fn ensure_opportunity_csv(path: impl AsRef<Path>) -> Result<()> {
    pm_storage::ensure_csv(path.as_ref(), OPPORTUNITY_CSV_HEADER)
        .context("创建机会 CSV 失败")?;
    tracing::info!("机会 CSV 就绪: {}", path.as_ref().display());
    Ok(())
}

/// 追加机会记录到 CSV。
pub fn append_opportunities(
    path: impl AsRef<Path>,
    opps: &[Opportunity],
) -> usize {
    let records: Vec<OpportunityRecord> = opps.iter().map(OpportunityRecord::from).collect();
    let written = pm_storage::append_records(path.as_ref(), &records);
    if written > 0 {
        tracing::info!(count = written, "机会数据已保存至 CSV");
    }
    written
}

/// 解析本地时间字符串。
fn parse_local(s: &str) -> Result<DateTime<Utc>> {
    let ndt = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("解析时间失败: {}", s))?;
    // 假设输入为 UTC（CSV 中存的是 UTC 格式字符串）
    Ok(DateTime::from_naive_utc_and_offset(ndt, Utc))
}

/// 从 CSV 加载历史机会。
pub fn load_opportunities(path: impl AsRef<Path>) -> Result<Vec<Opportunity>> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut rdr = csv::Reader::from_path(path).context("打开机会 CSV 失败")?;
    let mut opps: Vec<Opportunity> = Vec::new();

    for result in rdr.deserialize() {
        let record: OpportunityRecord = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "跳过无法解析的机会 CSV 行");
                continue;
            }
        };

        let detected_time = match parse_local(&record.detected_time) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "跳过时间非法的机会行");
                continue;
            }
        };

        let opp_type = match record.opportunity_type.as_str() {
            "Arbitrage" => OpportunityType::Arbitrage,
            "Spread" => OpportunityType::Spread,
            "Momentum" => OpportunityType::Momentum,
            "MeanReversion" => OpportunityType::MeanReversion,
            "Liquidity" => OpportunityType::Liquidity,
            "CrossMarket" => OpportunityType::CrossMarket,
            "PriceGap" => OpportunityType::PriceGap,
            _ => OpportunityType::Unknown,
        };

        let status = match record.status.as_str() {
            "Created" => OpportunityStatus::Created,
            "Updated" => OpportunityStatus::Updated,
            "Stable" => OpportunityStatus::Stable,
            "Weak" => OpportunityStatus::Weak,
            "Expired" => OpportunityStatus::Expired,
            "Removed" => OpportunityStatus::Removed,
            _ => OpportunityStatus::Created,
        };

        let opp = Opportunity {
            id: record.id,
            market_id: record.market_id,
            question: record.question,
            provider: record.provider,
            detected_time,
            expire_time: None,
            opportunity_type: opp_type,
            status,
            score: record.score,
            confidence: record.confidence,
            priority: record.priority,
            spread_score: record.spread_score,
            liquidity_score: record.liquidity_score,
            depth_score: record.depth_score,
            volume_score: record.volume_score,
            volatility_score: record.volatility_score,
            risk_score: record.risk_score,
            expected_roi: record.expected_roi,
            expected_profit: record.expected_profit,
            yes_price: record.yes_price,
            no_price: record.no_price,
            sum: record.sum,
            spread: record.spread,
            volume: record.volume,
            liquidity: record.liquidity,
            bid_depth: record.bid_depth,
            ask_depth: record.ask_depth,
            snapshot_id: None,
            version: record.version,
        };
        opps.push(opp);
    }

    tracing::info!(count = opps.len(), "从 CSV 加载机会完成");
    Ok(opps)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn record_from_opportunity_roundtrips_fields() {
        let opp = Opportunity::new(
            "m1".into(), "测试".into(), "test".into(), Utc::now(),
            OpportunityType::Arbitrage,
            85.0, 0.9, 85,
            25.0, 20.0, 18.0, 12.0, 5.0, 5.0,
            0.02, 2.0,
            0.42, 0.50, 0.92,
            Some(0.08), 5000.0, 8000.0,
            Some(2000.0), Some(2500.0),
        );
        let record = OpportunityRecord::from(&opp);
        assert_eq!(record.score, 85.0);
        assert_eq!(record.confidence, 0.9);
        assert_eq!(record.opportunity_type, "Arbitrage");
        assert_eq!(record.status, "Created");
    }

    #[test]
    fn csv_save_and_load_roundtrip() {
        let tmp = std::env::temp_dir().join("pm_opp_test.csv");
        let _ = std::fs::remove_file(&tmp);

        let opp = Opportunity::new(
            "m_test".into(), "测试问题".into(), "gamma".into(), Utc::now(),
            OpportunityType::Spread,
            75.0, 0.85, 75,
            20.0, 18.0, 15.0, 12.0, 5.0, 5.0,
            0.015, 1.5,
            0.45, 0.50, 0.95,
            Some(0.05), 3000.0, 5000.0,
            Some(1500.0), Some(1800.0),
        );

        // 确保 CSV 就绪
        ensure_opportunity_csv(&tmp).expect("ensure csv");

        // 追加
        let written = append_opportunities(&tmp, &[opp.clone()]);
        assert_eq!(written, 1);

        // 加载
        let loaded = load_opportunities(&tmp).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].market_id, "m_test");
        assert_eq!(loaded[0].question, "测试问题");
        assert_eq!(loaded[0].opportunity_type, OpportunityType::Spread);

        let _ = std::fs::remove_file(&tmp);
    }
}
