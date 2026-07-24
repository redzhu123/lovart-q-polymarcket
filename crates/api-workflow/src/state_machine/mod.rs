//! Workflow 状态机（P2-02）。
//!
//! 统一交易生命周期的状态机。所有状态变化必须记录日志（中文）。
//!
//! ```text
//! Idle
//!   ↓
//! LoadingMarket
//!   ↓
//! LoadingOrderBook
//!   ↓
//! CheckingBalance
//!   ↓ (完整路径)              ↓ (只读路径)
//! BuildingOrder             SyncPosition
//!   ↓                          ↓
//! SubmittingOrder(DryRun)    SyncBalance
//!   ↓                          ↓
//! WaitingResult             Completed
//!   ↓
//! SyncOrder
//!   ↓
//! SyncTrade
//!   ↓
//! SyncPosition
//!   ↓
//! SyncBalance
//!   ↓
//! Completed
//! ```
//!
//! 任意状态出错 -> Failed（终态）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// WorkflowState
// ============================================================================

/// Workflow 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkflowState {
    /// 空闲（初始）。
    Idle,
    /// 加载市场列表。
    LoadingMarket,
    /// 加载订单簿。
    LoadingOrderBook,
    /// 检查余额。
    CheckingBalance,
    /// 构建订单（本地）。
    BuildingOrder,
    /// 提交订单（DryRun，不发送）。
    SubmittingOrder,
    /// 等待结果。
    WaitingResult,
    /// 同步订单状态。
    SyncOrder,
    /// 同步成交记录。
    SyncTrade,
    /// 同步持仓。
    SyncPosition,
    /// 同步余额。
    SyncBalance,
    /// 已完成（终态）。
    Completed,
    /// 已失败（终态）。
    Failed,
}

impl WorkflowState {
    /// 中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            WorkflowState::Idle => "空闲",
            WorkflowState::LoadingMarket => "加载市场",
            WorkflowState::LoadingOrderBook => "加载订单簿",
            WorkflowState::CheckingBalance => "检查余额",
            WorkflowState::BuildingOrder => "构建订单",
            WorkflowState::SubmittingOrder => "提交订单(DryRun)",
            WorkflowState::WaitingResult => "等待结果",
            WorkflowState::SyncOrder => "同步订单",
            WorkflowState::SyncTrade => "同步成交",
            WorkflowState::SyncPosition => "同步持仓",
            WorkflowState::SyncBalance => "同步余额",
            WorkflowState::Completed => "已完成",
            WorkflowState::Failed => "已失败",
        }
    }

    /// 是否为终态。
    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkflowState::Completed | WorkflowState::Failed)
    }

    /// 是否为写操作相关状态（用于只读校验）。
    pub fn is_write_state(&self) -> bool {
        matches!(
            self,
            WorkflowState::BuildingOrder
                | WorkflowState::SubmittingOrder
                | WorkflowState::WaitingResult
                | WorkflowState::SyncOrder
                | WorkflowState::SyncTrade
        )
    }

    /// 完整生命周期的标准顺序（用于回放 / 报告排序）。
    pub fn lifecycle_order() -> &'static [WorkflowState] {
        &[
            WorkflowState::Idle,
            WorkflowState::LoadingMarket,
            WorkflowState::LoadingOrderBook,
            WorkflowState::CheckingBalance,
            WorkflowState::BuildingOrder,
            WorkflowState::SubmittingOrder,
            WorkflowState::WaitingResult,
            WorkflowState::SyncOrder,
            WorkflowState::SyncTrade,
            WorkflowState::SyncPosition,
            WorkflowState::SyncBalance,
            WorkflowState::Completed,
        ]
    }
}

impl std::fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_zh())
    }
}

// ============================================================================
// StateTransition
// ============================================================================

/// 一次状态转换记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// 起始状态。
    pub from: WorkflowState,
    /// 目标状态。
    pub to: WorkflowState,
    /// 时间戳。
    pub at: DateTime<Utc>,
    /// 原因（中文）。
    pub reason: String,
}

// ============================================================================
// StateMachine
// ============================================================================

/// Workflow 状态机。
///
/// 维护当前状态与转换历史，强制合法转换，所有变化输出中文日志。
pub struct StateMachine {
    /// 当前状态。
    current: WorkflowState,
    /// 转换历史。
    history: Vec<StateTransition>,
}

impl StateMachine {
    /// 创建新的状态机（初始 Idle）。
    pub fn new() -> Self {
        tracing::info!("【状态机】初始化 -> {}", WorkflowState::Idle.as_zh());
        Self {
            current: WorkflowState::Idle,
            history: Vec::new(),
        }
    }

    /// 当前状态。
    pub fn current(&self) -> WorkflowState {
        self.current
    }

    /// 转换历史。
    pub fn history(&self) -> &[StateTransition] {
        &self.history
    }

    /// 判断转换是否合法。
    pub fn can_transition(from: WorkflowState, to: WorkflowState) -> bool {
        if from == to {
            return false;
        }
        // 任意非终态可转 Failed
        if to == WorkflowState::Failed && !from.is_terminal() {
            return true;
        }
        match from {
            WorkflowState::Idle => to == WorkflowState::LoadingMarket,
            WorkflowState::LoadingMarket => to == WorkflowState::LoadingOrderBook,
            WorkflowState::LoadingOrderBook => to == WorkflowState::CheckingBalance,
            // 完整路径：BuildingOrder；只读路径：SyncPosition；也可直接结束
            WorkflowState::CheckingBalance => {
                matches!(
                    to,
                    WorkflowState::BuildingOrder
                        | WorkflowState::SyncPosition
                        | WorkflowState::Completed
                )
            }
            WorkflowState::BuildingOrder => to == WorkflowState::SubmittingOrder,
            WorkflowState::SubmittingOrder => to == WorkflowState::WaitingResult,
            WorkflowState::WaitingResult => to == WorkflowState::SyncOrder,
            WorkflowState::SyncOrder => to == WorkflowState::SyncTrade,
            WorkflowState::SyncTrade => to == WorkflowState::SyncPosition,
            WorkflowState::SyncPosition => to == WorkflowState::SyncBalance,
            WorkflowState::SyncBalance => to == WorkflowState::Completed,
            // 终态不可再转换
            WorkflowState::Completed | WorkflowState::Failed => false,
        }
    }

    /// 执行状态转换。
    ///
    /// 非法转换会被记录并转为 Failed。
    pub fn transition(&mut self, to: WorkflowState, reason: &str) -> Result<(), String> {
        if self.current.is_terminal() {
            let err = format!(
                "状态机已处于终态 {}，拒绝转换至 {}",
                self.current.as_zh(),
                to.as_zh()
            );
            tracing::error!("{}", err);
            return Err(err);
        }

        if !Self::can_transition(self.current, to) {
            let err = format!(
                "非法状态转换: {} -> {}",
                self.current.as_zh(),
                to.as_zh()
            );
            tracing::error!("{}", err);
            // 非法转换 -> Failed
            self.force_failed(&err);
            return Err(err);
        }

        let from = self.current;
        let at = Utc::now();
        tracing::info!(
            "【状态转换】{} -> {}（{}）",
            from.as_zh(),
            to.as_zh(),
            reason
        );
        self.history.push(StateTransition {
            from,
            to,
            at,
            reason: reason.to_string(),
        });
        self.current = to;
        Ok(())
    }

    /// 强制进入 Failed（用于步骤失败）。
    pub fn force_failed(&mut self, reason: &str) {
        let from = self.current;
        let at = Utc::now();
        tracing::error!("【状态转换】{} -> 已失败（{}）", from.as_zh(), reason);
        self.history.push(StateTransition {
            from,
            to: WorkflowState::Failed,
            at,
            reason: reason.to_string(),
        });
        self.current = WorkflowState::Failed;
    }

    /// 重置为 Idle。
    pub fn reset(&mut self) {
        self.current = WorkflowState::Idle;
        self.history.clear();
        tracing::info!("【状态机】已重置 -> {}", WorkflowState::Idle.as_zh());
    }

    /// 是否已完成。
    pub fn is_completed(&self) -> bool {
        self.current == WorkflowState::Completed
    }

    /// 是否已失败。
    pub fn is_failed(&self) -> bool {
        self.current == WorkflowState::Failed
    }

    /// 渲染状态机图（中文，ASCII）。
    pub fn diagram_zh() -> &'static str {
        "Idle -> 加载市场 -> 加载订单簿 -> 检查余额\n\
         完整路径: 构建订单 -> 提交订单(DryRun) -> 等待结果 -> 同步订单 -> 同步成交 -> 同步持仓 -> 同步余额 -> 已完成\n\
         只读路径: 同步持仓 -> 同步余额 -> 已完成\n\
         任意步骤失败 -> 已失败"
    }
}

impl Default for StateMachine {
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

    #[test]
    fn starts_idle() {
        let sm = StateMachine::new();
        assert_eq!(sm.current(), WorkflowState::Idle);
        assert!(!sm.current().is_terminal());
    }

    #[test]
    fn happy_path_full_lifecycle() {
        let mut sm = StateMachine::new();
        assert!(sm.transition(WorkflowState::LoadingMarket, "开始加载市场").is_ok());
        assert!(sm.transition(WorkflowState::LoadingOrderBook, "市场加载完成").is_ok());
        assert!(sm.transition(WorkflowState::CheckingBalance, "订单簿加载完成").is_ok());
        assert!(sm.transition(WorkflowState::BuildingOrder, "开始构建订单").is_ok());
        assert!(sm.transition(WorkflowState::SubmittingOrder, "DryRun 提交").is_ok());
        assert!(sm.transition(WorkflowState::WaitingResult, "等待结果").is_ok());
        assert!(sm.transition(WorkflowState::SyncOrder, "同步订单").is_ok());
        assert!(sm.transition(WorkflowState::SyncTrade, "同步成交").is_ok());
        assert!(sm.transition(WorkflowState::SyncPosition, "同步持仓").is_ok());
        assert!(sm.transition(WorkflowState::SyncBalance, "同步余额").is_ok());
        assert!(sm.transition(WorkflowState::Completed, "生命周期完成").is_ok());
        assert!(sm.is_completed());
        assert_eq!(sm.history().len(), 11);
    }

    #[test]
    fn readonly_path_skips_order_steps() {
        let mut sm = StateMachine::new();
        sm.transition(WorkflowState::LoadingMarket, "").ok();
        sm.transition(WorkflowState::LoadingOrderBook, "").ok();
        sm.transition(WorkflowState::CheckingBalance, "").ok();
        // 只读路径：直接进入 SyncPosition
        assert!(sm.transition(WorkflowState::SyncPosition, "只读跳过下单").is_ok());
        assert!(sm.transition(WorkflowState::SyncBalance, "").is_ok());
        assert!(sm.transition(WorkflowState::Completed, "").is_ok());
        assert!(sm.is_completed());
    }

    #[test]
    fn illegal_transition_fails() {
        let mut sm = StateMachine::new();
        // Idle -> SubmittingOrder 非法
        let res = sm.transition(WorkflowState::SubmittingOrder, "非法跳转");
        assert!(res.is_err());
        assert!(sm.is_failed());
    }

    #[test]
    fn any_state_can_fail() {
        let mut sm = StateMachine::new();
        sm.transition(WorkflowState::LoadingMarket, "").ok();
        assert!(sm.transition(WorkflowState::Failed, "市场加载失败").is_ok());
        assert!(sm.is_failed());
        // 终态不可再转换
        assert!(sm.transition(WorkflowState::Completed, "").is_err());
    }

    #[test]
    fn write_state_detection() {
        assert!(WorkflowState::BuildingOrder.is_write_state());
        assert!(WorkflowState::SubmittingOrder.is_write_state());
        assert!(!WorkflowState::LoadingMarket.is_write_state());
        assert!(!WorkflowState::SyncPosition.is_write_state());
    }

    #[test]
    fn reset_clears_history() {
        let mut sm = StateMachine::new();
        sm.transition(WorkflowState::LoadingMarket, "").ok();
        sm.reset();
        assert_eq!(sm.current(), WorkflowState::Idle);
        assert!(sm.history().is_empty());
    }
}
