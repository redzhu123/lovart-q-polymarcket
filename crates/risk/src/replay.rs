//! Risk Replay（V1.05 第十一节）。
//!
//! 能够重新计算历史 Risk，方便验证参数。
//!
//! 用法：
//! ```text
//! cargo run -- risk-replay
//! ```
//!
//! 读取历史 CSV 数据，逐条重新评估风险，输出对比报告。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::config::RiskConfig;
use crate::context::RiskContext;
use crate::engine::{RiskDecision, RiskEngine, RiskEvaluation, TradeSuggestion};

/// 单条回放记录：输入 + 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskReplayRecord {
    /// 时间戳。
    pub time: String,
    /// 市场 ID。
    pub market_id: String,
    /// 问题。
    pub question: String,
    /// 方向。
    pub side: String,
    /// 价格。
    pub price: f64,
    /// 名义金额。
    pub notional: f64,
    /// 可用现金。
    pub available_cash: f64,
    /// 持仓数。
    pub position_count: usize,
    /// 当日盈亏。
    pub daily_pnl: f64,
    /// 连续亏损。
    pub consecutive_losses: usize,
    /// 决策结果。
    pub decision: String,
    /// 风险评分。
    pub risk_score: f64,
    /// 说明。
    pub explanation: String,
}

/// Risk Replay 引擎。
pub struct RiskReplay {
    config: RiskConfig,
    records: Vec<RiskReplayRecord>,
    /// 累计状态（模拟历史演进）。
    cumulative_rejections: u64,
}

impl RiskReplay {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            records: Vec::new(),
            cumulative_rejections: 0,
        }
    }

    /// 从历史数据进行回放评估。
    ///
    /// `history` 是 (time, suggestion, context) 的列表。
    pub fn replay(
        &mut self,
        history: &[(DateTime<Local>, TradeSuggestion, RiskContext)],
    ) -> Vec<RiskReplayRecord> {
        let mut engine = RiskEngine::new(self.config.clone());
        let mut records = Vec::new();

        for (time, suggestion, ctx) in history {
            let eval = engine.evaluate(ctx, suggestion);
            let rec = self.eval_to_record(time, suggestion, ctx, &eval);
            records.push(rec);

            if eval.decision == RiskDecision::Reject {
                self.cumulative_rejections += 1;
            }
        }

        self.records = records.clone();
        records
    }

    fn eval_to_record(
        &self,
        time: &DateTime<Local>,
        suggestion: &TradeSuggestion,
        ctx: &RiskContext,
        eval: &RiskEvaluation,
    ) -> RiskReplayRecord {
        RiskReplayRecord {
            time: time.format("%Y-%m-%d %H:%M:%S").to_string(),
            market_id: suggestion.market_id.clone(),
            question: suggestion.question.clone(),
            side: format!("{:?}", suggestion.side),
            price: suggestion.price,
            notional: suggestion.notional,
            available_cash: ctx.available_cash,
            position_count: ctx.open_position_count,
            daily_pnl: ctx.daily_realized_pnl,
            consecutive_losses: ctx.consecutive_losses,
            decision: eval.decision.as_zh().to_string(),
            risk_score: eval.score.total,
            explanation: eval.explain.one_line_zh(),
        }
    }

    /// 生成回放报告。
    pub fn report_zh(&self) -> String {
        let total = self.records.len();
        let accepted = self.records.iter().filter(|r| r.decision == "接受").count();
        let reviewed = self.records.iter().filter(|r| r.decision == "需审核").count();
        let rejected = self.records.iter().filter(|r| r.decision == "拒绝").count();

        let avg_score = if total > 0 {
            self.records.iter().map(|r| r.risk_score).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let mut lines = Vec::new();
        lines.push("【风险回放报告】".to_string());
        lines.push(String::new());
        lines.push(format!("  总评估次数：  {}", total));
        lines.push(format!("  接受：        {}（{:.0}%）", accepted, if total > 0 { accepted as f64 / total as f64 * 100.0 } else { 0.0 }));
        lines.push(format!("  需审核：      {}（{:.0}%）", reviewed, if total > 0 { reviewed as f64 / total as f64 * 100.0 } else { 0.0 }));
        lines.push(format!("  拒绝：        {}（{:.0}%）", rejected, if total > 0 { rejected as f64 / total as f64 * 100.0 } else { 0.0 }));
        lines.push(format!("  平均风险评分：{:.0}/100", avg_score));
        lines.push(String::new());

        // Top 5 最高风险评分
        if total > 0 {
            let mut sorted: Vec<_> = self.records.clone();
            sorted.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap_or(std::cmp::Ordering::Equal));
            lines.push("  Top 5 最低风险：".to_string());
            for r in sorted.iter().take(5) {
                lines.push(format!(
                    "    {} | {} | 评分 {:.0} | {}",
                    r.time, r.question.chars().take(25).collect::<String>(), r.risk_score, r.decision
                ));
            }
            lines.push(String::new());

            // Bottom 5 最危险
            sorted.reverse();
            lines.push("  Top 5 最高风险：".to_string());
            for r in sorted.iter().take(5) {
                lines.push(format!(
                    "    {} | {} | 评分 {:.0} | {}",
                    r.time, r.question.chars().take(25).collect::<String>(), r.risk_score, r.decision
                ));
            }
        }

        lines.join("\n")
    }

    /// 保存回放记录到 CSV。
    pub fn save_csv(&self, path: &str) -> anyhow::Result<()> {
        if self.records.is_empty() {
            return Ok(());
        }
        let mut wtr = csv::Writer::from_path(path)?;
        for r in &self.records {
            wtr.serialize(r)?;
        }
        wtr.flush()?;
        Ok(())
    }

    /// 获取记录。
    pub fn records(&self) -> &[RiskReplayRecord] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn replay_produces_records() {
        let mut replay = RiskReplay::new(RiskConfig::default());
        let now = Local::now();
        let mut ctx = RiskContext::minimal(10000.0, 9000.0, now);
        ctx.market_liquidity = 10000.0; // 满足流动性最低要求
        ctx.suggested_price = 0.5;
        ctx.suggested_notional = 100.0;
        let sug = TradeSuggestion::new("mkt1", "Q1", pm_core::Side::Buy, 0.5, 100.0, "Test");
        let history = vec![(now, sug, ctx)];
        let records = replay.replay(&history);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].decision, "接受");
    }

    #[test]
    fn replay_reports_stats() {
        let mut replay = RiskReplay::new(RiskConfig::default());
        let now = Local::now();

        // 正常
        let ctx1 = RiskContext::minimal(10000.0, 9000.0, now);
        let sug1 = TradeSuggestion::new("mkt1", "Q1", pm_core::Side::Buy, 0.5, 100.0, "Test");

        // 拒绝：高亏损
        let mut ctx2 = RiskContext::minimal(10000.0, 9000.0, now);
        ctx2.daily_realized_pnl = -1200.0;
        let sug2 = TradeSuggestion::new("mkt2", "Q2", pm_core::Side::Buy, 0.5, 100.0, "Test");

        let history = vec![(now, sug1, ctx1), (now, sug2, ctx2)];
        replay.replay(&history);

        let report = replay.report_zh();
        assert!(report.contains("风险回放报告"));
        assert!(report.contains("总评估次数"));
        assert!(report.contains("拒绝"));
    }
}
