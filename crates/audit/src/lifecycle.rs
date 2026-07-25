//! 统一数据生命周期阶段定义。
//!
//! 完整生命周期为：
//! ```text
//! Market → Candidate → Opportunity → ShadowTrade → PaperOrder → Execution → Settlement → ClosedPosition → Portfolio → Report
//! ```
//!
//! 任何对象必须经过生命周期，禁止跨阶段创建，禁止跳过生命周期。

/// 生命周期阶段（按顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecycleStage {
    /// 原始市场数据（从 API 拉取）。
    MarketScanned,
    /// 候选机会（市场进入分析管道）。
    Candidate,
    /// 套利机会（通过所有过滤，产生 Opportunity）。
    Opportunity,
    /// 影子交易（理论模拟，Shadow Model）。
    ShadowTrade,
    /// 纸面订单（Paper Trading 开仓）。
    PaperOrder,
    /// 执行订单（Execution Simulator 订单）。
    Execution,
    /// 结算（Settlement Engine 处理）。
    Settlement,
    /// 已平仓持仓。
    ClosedPosition,
    /// 投资组合快照。
    Portfolio,
    /// 最终报告。
    Report,
}

impl LifecycleStage {
    /// 中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            LifecycleStage::MarketScanned => "市场扫描",
            LifecycleStage::Candidate => "候选分析",
            LifecycleStage::Opportunity => "机会发现",
            LifecycleStage::ShadowTrade => "影子交易",
            LifecycleStage::PaperOrder => "纸面订单",
            LifecycleStage::Execution => "执行订单",
            LifecycleStage::Settlement => "结算",
            LifecycleStage::ClosedPosition => "已平仓持仓",
            LifecycleStage::Portfolio => "投资组合",
            LifecycleStage::Report => "报告",
        }
    }

    /// 英文标识符（用于 CSV / 日志 key）。
    pub fn as_key(&self) -> &'static str {
        match self {
            LifecycleStage::MarketScanned => "market_scanned",
            LifecycleStage::Candidate => "candidate",
            LifecycleStage::Opportunity => "opportunity",
            LifecycleStage::ShadowTrade => "shadow_trade",
            LifecycleStage::PaperOrder => "paper_order",
            LifecycleStage::Execution => "execution",
            LifecycleStage::Settlement => "settlement",
            LifecycleStage::ClosedPosition => "closed_position",
            LifecycleStage::Portfolio => "portfolio",
            LifecycleStage::Report => "report",
        }
    }

    /// 生命周期阶段序号（用于排序比较）。
    pub fn ordinal(&self) -> u8 {
        match self {
            LifecycleStage::MarketScanned => 0,
            LifecycleStage::Candidate => 1,
            LifecycleStage::Opportunity => 2,
            LifecycleStage::ShadowTrade => 3,
            LifecycleStage::PaperOrder => 4,
            LifecycleStage::Execution => 5,
            LifecycleStage::Settlement => 6,
            LifecycleStage::ClosedPosition => 7,
            LifecycleStage::Portfolio => 8,
            LifecycleStage::Report => 9,
        }
    }

    /// 前一个阶段（用于校验合法性）。
    pub fn previous(&self) -> Option<LifecycleStage> {
        match self {
            LifecycleStage::MarketScanned => None,
            LifecycleStage::Candidate => Some(LifecycleStage::MarketScanned),
            LifecycleStage::Opportunity => Some(LifecycleStage::Candidate),
            LifecycleStage::ShadowTrade => Some(LifecycleStage::Opportunity),
            LifecycleStage::PaperOrder => Some(LifecycleStage::Opportunity),
            LifecycleStage::Execution => Some(LifecycleStage::PaperOrder),
            LifecycleStage::Settlement => Some(LifecycleStage::Execution),
            LifecycleStage::ClosedPosition => Some(LifecycleStage::Settlement),
            LifecycleStage::Portfolio => Some(LifecycleStage::ClosedPosition),
            LifecycleStage::Report => Some(LifecycleStage::Portfolio),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_stage_ordering() {
        assert!(LifecycleStage::MarketScanned < LifecycleStage::Candidate);
        assert!(LifecycleStage::Candidate < LifecycleStage::Opportunity);
        assert!(LifecycleStage::Opportunity < LifecycleStage::ShadowTrade);
        assert!(LifecycleStage::Execution < LifecycleStage::Settlement);
    }

    #[test]
    fn lifecycle_stage_ordinal_increasing() {
        let stages = [
            LifecycleStage::MarketScanned,
            LifecycleStage::Candidate,
            LifecycleStage::Opportunity,
            LifecycleStage::ShadowTrade,
            LifecycleStage::PaperOrder,
            LifecycleStage::Execution,
            LifecycleStage::Settlement,
            LifecycleStage::ClosedPosition,
            LifecycleStage::Portfolio,
            LifecycleStage::Report,
        ];
        for i in 1..stages.len() {
            assert!(
                stages[i].ordinal() > stages[i - 1].ordinal(),
                "ordinal not increasing: {:?} -> {:?}",
                stages[i - 1],
                stages[i]
            );
        }
    }

    #[test]
    fn previous_stage_chain() {
        assert_eq!(
            LifecycleStage::Opportunity.previous(),
            Some(LifecycleStage::Candidate)
        );
        assert_eq!(
            LifecycleStage::PaperOrder.previous(),
            Some(LifecycleStage::Opportunity)
        );
        assert_eq!(
            LifecycleStage::Execution.previous(),
            Some(LifecycleStage::PaperOrder)
        );
        assert_eq!(LifecycleStage::MarketScanned.previous(), None);
    }

    #[test]
    fn all_stages_have_zh_names() {
        let stages = [
            LifecycleStage::MarketScanned,
            LifecycleStage::Candidate,
            LifecycleStage::Opportunity,
            LifecycleStage::ShadowTrade,
            LifecycleStage::PaperOrder,
            LifecycleStage::Execution,
            LifecycleStage::Settlement,
            LifecycleStage::ClosedPosition,
            LifecycleStage::Portfolio,
            LifecycleStage::Report,
        ];
        for stage in &stages {
            let zh = stage.as_zh();
            assert!(!zh.is_empty(), "empty zh for {:?}", stage);
        }
    }
}
