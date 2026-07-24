//! 机会统计（V1.04 第十节）。
//!
//! 累计统计：发现数 / 过滤数 / 保留数 / 高优先级数 / 平均 ROI / 平均置信度。
//! 输出全中文。

use std::collections::HashMap;

use crate::model::OpportunityType;

/// 机会引擎统计累加器。
#[derive(Debug, Clone, Default)]
pub struct OpportunityStatistics {
    /// 扫描轮数。
    pub rounds: u64,
    /// 接收到的市场总数。
    pub markets_received: u64,
    /// 有有效价格的市场数。
    pub markets_with_price: u64,
    /// 引擎发现的机会数（通过 SUM 阈值）。
    pub found: u64,
    /// 被过滤的机会数（低于 min_score）。
    pub filtered_low_score: u64,
    /// 最终保留的机会数。
    pub kept: u64,
    /// 新机会数。
    pub new_count: u64,
    /// 更新机会数。
    pub updated_count: u64,
    /// 高优先级机会数（score ≥ 80）。
    pub high_priority: u64,
    /// 过期机会数。
    pub expired: u64,
    /// 保留机会的平均 ROI（累计 ROI / kept）。
    pub total_roi: f64,
    /// 保留机会的平均置信度（累计 confidence / kept）。
    pub total_confidence: f64,
    /// 最高评分。
    pub max_score: f64,
    /// 最低评分（保留的）。
    pub min_score: f64,
    /// 各类型计数。
    pub type_counts: HashMap<OpportunityType, u64>,
}

impl OpportunityStatistics {
    /// 创建新的统计累加器。
    pub fn new() -> Self {
        Self {
            min_score: 100.0, // 初始化为最高，后续用 min 更新
            ..Default::default()
        }
    }

    /// 累加一轮的处理结果。
    pub fn accumulate(
        &mut self,
        found: u64,
        filtered: u64,
        kept: u64,
        new_count: u64,
        updated_count: u64,
        high_priority: u64,
        avg_roi: f64,
        avg_confidence: f64,
        max_score: f64,
        min_score_in_round: f64,
        type_counts: HashMap<OpportunityType, u64>,
    ) {
        self.rounds += 1;
        self.found += found;
        self.filtered_low_score += filtered;
        self.kept += kept;
        self.new_count += new_count;
        self.updated_count += updated_count;
        self.high_priority += high_priority;

        if kept > 0 {
            self.total_roi += avg_roi * kept as f64; // 恢复总和再累加
            self.total_confidence += avg_confidence * kept as f64;
        }

        self.max_score = self.max_score.max(max_score);
        if kept > 0 {
            self.min_score = self.min_score.min(min_score_in_round);
        }

        for (t, count) in type_counts {
            *self.type_counts.entry(t).or_insert(0) += count;
        }
    }

    /// 平均 ROI（0.0 如果没有保留的机会）。
    pub fn avg_roi(&self) -> f64 {
        if self.kept == 0 {
            0.0
        } else {
            self.total_roi / self.kept as f64
        }
    }

    /// 平均置信度（0.0 如果没有保留的机会）。
    pub fn avg_confidence(&self) -> f64 {
        if self.kept == 0 {
            0.0
        } else {
            self.total_confidence / self.kept as f64
        }
    }

    /// 打印中文统计报告。
    pub fn print_report(&self) {
        println!("======================================");
        println!();
        println!("机会引擎统计（V1.04）");
        println!();
        println!("--------------------------------------");
        println!();
        println!("  扫描轮数           : {}", self.rounds);
        println!("  接收市场数         : {}", self.markets_received);
        println!("  有价市场数         : {}", self.markets_with_price);
        println!();
        println!("  发现机会           : {}", self.found);
        println!("  过滤（低分）       : {}", self.filtered_low_score);
        println!("  保留机会           : {}", self.kept);
        println!("  新机会             : {}", self.new_count);
        println!("  更新机会           : {}", self.updated_count);
        println!("  过期机会           : {}", self.expired);
        println!();
        println!("  高优先级（≥80）    : {}", self.high_priority);
        println!("  平均 ROI           : {:.2}%", self.avg_roi() * 100.0);
        println!(
            "  平均置信度         : {:.0}%",
            self.avg_confidence() * 100.0
        );
        println!("  最高评分           : {:.1}", self.max_score);
        if self.kept > 0 {
            println!("  最低评分           : {:.1}", self.min_score);
        }
        println!();
        if !self.type_counts.is_empty() {
            println!("  类型分布：");
            let mut types: Vec<_> = self.type_counts.iter().collect();
            types.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
            for (t, c) in &types {
                println!("    {:<10} : {}", t.as_zh(), c);
            }
            println!();
        }
        println!("======================================");
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stats_has_defaults() {
        let s = OpportunityStatistics::new();
        assert_eq!(s.rounds, 0);
        assert_eq!(s.kept, 0);
        assert_eq!(s.max_score, 0.0);
    }

    #[test]
    fn accumulate_updates_counts() {
        let mut s = OpportunityStatistics::new();
        let mut tc = HashMap::new();
        tc.insert(OpportunityType::Arbitrage, 3);
        tc.insert(OpportunityType::Spread, 5);
        s.accumulate(10, 4, 6, 3, 3, 2, 0.05, 0.8, 92.0, 55.0, tc);

        assert_eq!(s.rounds, 1);
        assert_eq!(s.found, 10);
        assert_eq!(s.filtered_low_score, 4);
        assert_eq!(s.kept, 6);
        assert_eq!(s.new_count, 3);
        assert_eq!(s.updated_count, 3);
        assert_eq!(s.high_priority, 2);
        assert_eq!(s.max_score, 92.0);
        assert_eq!(s.min_score, 55.0);
    }

    #[test]
    fn avg_roi_and_confidence() {
        let mut s = OpportunityStatistics::new();
        let tc = HashMap::new();
        // Round 1: 2 kept, avg ROI 0.05
        s.accumulate(5, 3, 2, 1, 1, 0, 0.05, 0.8, 80.0, 70.0, tc.clone());
        // Round 2: 4 kept, avg ROI 0.10
        s.accumulate(6, 2, 4, 2, 2, 2, 0.10, 0.9, 95.0, 60.0, tc);

        // Total ROI = 0.05*2 + 0.10*4 = 0.10 + 0.40 = 0.50
        // Avg = 0.50 / 6 = 0.0833...
        let avg = s.avg_roi();
        assert!(
            (avg - 0.08333).abs() < 0.01,
            "avg ROI should be ~0.083, got {avg}"
        );

        // Total confidence = 0.8*2 + 0.9*4 = 1.6 + 3.6 = 5.2
        // Avg = 5.2 / 6 = 0.866...
        let avg_conf = s.avg_confidence();
        assert!(
            (avg_conf - 0.8666).abs() < 0.02,
            "avg confidence should be ~0.867, got {avg_conf}"
        );
    }

    #[test]
    fn avg_returns_zero_when_no_kept() {
        let s = OpportunityStatistics::new();
        assert_eq!(s.avg_roi(), 0.0);
        assert_eq!(s.avg_confidence(), 0.0);
    }
}
