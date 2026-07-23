//! Risk Event（V1.05 第九节）。
//!
//! 所有风险事件记录到 CSV。
//! 事件类型：
//! - PositionLimit：持仓达上限
//! - ExposureLimit：暴露达上限
//! - DailyLossLimit：日亏损达上限
//! - LiquidityWarning：流动性警告
//! - DrawdownWarning：回撤警告
//! - RiskReject：风险拒绝
//! - ConsecutiveLossWarning：连续亏损警告
//! - ConsecutiveLossLimit：连续亏损达上限

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// 风险事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskEventKind {
    /// 持仓达上限。
    PositionLimit,
    /// 暴露达上限。
    ExposureLimit,
    /// 日亏损达上限。
    DailyLossLimit,
    /// 流动性警告。
    LiquidityWarning,
    /// 回撤警告。
    DrawdownWarning,
    /// 风险拒绝。
    RiskReject,
    /// 连续亏损警告。
    ConsecutiveLossWarning,
    /// 连续亏损达上限。
    ConsecutiveLossLimit,
    /// 资金占用警告。
    CapitalUsageWarning,
    /// 风险审核（需人工查看）。
    RiskReview,
}

impl RiskEventKind {
    pub fn as_zh(&self) -> &'static str {
        match self {
            RiskEventKind::PositionLimit => "持仓上限",
            RiskEventKind::ExposureLimit => "暴露上限",
            RiskEventKind::DailyLossLimit => "日亏损上限",
            RiskEventKind::LiquidityWarning => "流动性警告",
            RiskEventKind::DrawdownWarning => "回撤警告",
            RiskEventKind::RiskReject => "风险拒绝",
            RiskEventKind::ConsecutiveLossWarning => "连续亏损警告",
            RiskEventKind::ConsecutiveLossLimit => "连续亏损上限",
            RiskEventKind::CapitalUsageWarning => "资金占用警告",
            RiskEventKind::RiskReview => "风险审核",
        }
    }

    /// 严重程度（0=通知，1=警告，2=严重，3=阻止）。
    pub fn severity(&self) -> u8 {
        match self {
            RiskEventKind::LiquidityWarning => 1,
            RiskEventKind::DrawdownWarning => 1,
            RiskEventKind::ConsecutiveLossWarning => 1,
            RiskEventKind::CapitalUsageWarning => 1,
            RiskEventKind::RiskReview => 1,
            RiskEventKind::PositionLimit => 2,
            RiskEventKind::ExposureLimit => 2,
            RiskEventKind::ConsecutiveLossLimit => 2,
            RiskEventKind::DailyLossLimit => 3,
            RiskEventKind::RiskReject => 3,
        }
    }
}

/// 风险事件。
#[derive(Debug, Clone)]
pub struct RiskEvent {
    /// 事件类型。
    pub kind: RiskEventKind,
    /// 事件描述（中文）。
    pub description: String,
    /// 相关市场（如有）。
    pub market_id: Option<String>,
    /// 相关数值。
    pub value: Option<f64>,
    /// 阈值。
    pub threshold: Option<f64>,
    /// 时间。
    pub time: DateTime<Local>,
}

impl RiskEvent {
    pub fn new(kind: RiskEventKind, description: String) -> Self {
        Self {
            kind,
            description,
            market_id: None,
            value: None,
            threshold: None,
            time: Local::now(),
        }
    }

    pub fn with_market(mut self, market_id: &str) -> Self {
        self.market_id = Some(market_id.to_string());
        self
    }

    pub fn with_value(mut self, value: f64) -> Self {
        self.value = Some(value);
        self
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = Some(threshold);
        self
    }
}

/// 风险事件 CSV 记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskEventRecord {
    pub time: String,
    pub event_type: String,
    pub severity: u8,
    pub description: String,
    pub market_id: String,
    pub value: String,
    pub threshold: String,
}

impl From<&RiskEvent> for RiskEventRecord {
    fn from(ev: &RiskEvent) -> Self {
        Self {
            time: ev.time.format("%Y-%m-%d %H:%M:%S").to_string(),
            event_type: ev.kind.as_zh().to_string(),
            severity: ev.kind.severity(),
            description: ev.description.clone(),
            market_id: ev.market_id.clone().unwrap_or_default(),
            value: ev.value.map(|v| format!("{:.4}", v)).unwrap_or_default(),
            threshold: ev.threshold.map(|v| format!("{:.4}", v)).unwrap_or_default(),
        }
    }
}

/// 事件收集器。
#[derive(Debug, Default)]
pub struct RiskEventCollector {
    events: Vec<RiskEvent>,
    /// 分类计数。
    pub position_limits: usize,
    pub exposure_limits: usize,
    pub daily_loss_limits: usize,
    pub liquidity_warnings: usize,
    pub drawdown_warnings: usize,
    pub risk_rejects: usize,
    pub consecutive_loss_warnings: usize,
    pub consecutive_loss_limits: usize,
    pub capital_usage_warnings: usize,
    pub risk_reviews: usize,
}

impl RiskEventCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, event: RiskEvent) {
        match event.kind {
            RiskEventKind::PositionLimit => self.position_limits += 1,
            RiskEventKind::ExposureLimit => self.exposure_limits += 1,
            RiskEventKind::DailyLossLimit => self.daily_loss_limits += 1,
            RiskEventKind::LiquidityWarning => self.liquidity_warnings += 1,
            RiskEventKind::DrawdownWarning => self.drawdown_warnings += 1,
            RiskEventKind::RiskReject => self.risk_rejects += 1,
            RiskEventKind::ConsecutiveLossWarning => self.consecutive_loss_warnings += 1,
            RiskEventKind::ConsecutiveLossLimit => self.consecutive_loss_limits += 1,
            RiskEventKind::CapitalUsageWarning => self.capital_usage_warnings += 1,
            RiskEventKind::RiskReview => self.risk_reviews += 1,
        }
        self.events.push(event);
    }

    pub fn total(&self) -> usize {
        self.events.len()
    }

    pub fn events(&self) -> &[RiskEvent] {
        &self.events
    }

    /// 转为 CSV 记录列表。
    pub fn to_records(&self) -> Vec<RiskEventRecord> {
        self.events.iter().map(RiskEventRecord::from).collect()
    }

    /// 保存到 CSV。
    pub fn save_to_csv(&self, path: &str) -> anyhow::Result<()> {
        let records = self.to_records();
        if records.is_empty() {
            return Ok(());
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let mut wtr = csv::Writer::from_writer(file);
        for r in &records {
            wtr.serialize(r)?;
        }
        wtr.flush()?;
        Ok(())
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("风险事件总数：{}", self.total()));
        if self.risk_rejects > 0 {
            lines.push(format!("  风险拒绝：{} 次", self.risk_rejects));
        }
        if self.daily_loss_limits > 0 {
            lines.push(format!("  日亏损上限：{} 次", self.daily_loss_limits));
        }
        if self.position_limits > 0 {
            lines.push(format!("  持仓上限：{} 次", self.position_limits));
        }
        if self.drawdown_warnings > 0 {
            lines.push(format!("  回撤警告：{} 次", self.drawdown_warnings));
        }
        if self.liquidity_warnings > 0 {
            lines.push(format!("  流动性警告：{} 次", self.liquidity_warnings));
        }
        if self.consecutive_loss_warnings > 0 {
            lines.push(format!("  连续亏损警告：{} 次", self.consecutive_loss_warnings));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_zh_all_variants() {
        let kinds = [
            RiskEventKind::PositionLimit,
            RiskEventKind::ExposureLimit,
            RiskEventKind::DailyLossLimit,
            RiskEventKind::LiquidityWarning,
            RiskEventKind::DrawdownWarning,
            RiskEventKind::RiskReject,
            RiskEventKind::ConsecutiveLossWarning,
            RiskEventKind::ConsecutiveLossLimit,
            RiskEventKind::CapitalUsageWarning,
            RiskEventKind::RiskReview,
        ];
        for k in &kinds {
            assert!(!k.as_zh().is_empty());
            assert!(k.severity() <= 3);
        }
    }

    #[test]
    fn collector_counts_by_kind() {
        let mut collector = RiskEventCollector::new();
        collector.record(RiskEvent::new(
            RiskEventKind::RiskReject,
            "测试拒绝".into(),
        ));
        collector.record(RiskEvent::new(
            RiskEventKind::RiskReject,
            "再次拒绝".into(),
        ));
        collector.record(RiskEvent::new(
            RiskEventKind::LiquidityWarning,
            "流动性不足".into(),
        ));
        assert_eq!(collector.total(), 3);
        assert_eq!(collector.risk_rejects, 2);
        assert_eq!(collector.liquidity_warnings, 1);
    }

    #[test]
    fn record_conversion() {
        let ev = RiskEvent::new(
            RiskEventKind::DailyLossLimit,
            "当日亏损已达上限".into(),
        )
        .with_market("test-market")
        .with_value(-1100.0)
        .with_threshold(1000.0);
        let rec = RiskEventRecord::from(&ev);
        assert_eq!(rec.event_type, "日亏损上限");
        assert_eq!(rec.severity, 3);
        assert_eq!(rec.market_id, "test-market");
    }

    #[test]
    fn summary_zh_contains_counts() {
        let mut collector = RiskEventCollector::new();
        collector.record(RiskEvent::new(RiskEventKind::RiskReject, "test".into()));
        collector.record(RiskEvent::new(RiskEventKind::DrawdownWarning, "test".into()));
        let summary = collector.summary_zh();
        assert!(summary.contains("2"));
        assert!(summary.contains("风险拒绝"));
        assert!(summary.contains("回撤警告"));
    }
}
