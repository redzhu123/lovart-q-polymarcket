//! OMS Order State Machine（P2-04 第三节）。
//!
//! 集中管理订单状态的所有合法转移。所有 OMS API 在变更 `Order.status` 前
//! 必须调用 [`StateMachine::validate_transition`] 校验；非法转移会被拒绝并
//! 记录中文错误日志。
//!
//! # 设计原则
//!
//! - **白名单**：只允许预定义的转移路径，其余一律拒绝。
//! - **不可逆**：终态（Filled / Cancelled / Rejected / Expired）不接受任何再转移。
//! - **聚合态**：Completed 是聚合终态，仅用于统计展示，运行时不会进入。
//!
//! # 状态图
//!
//! ```text
//!                    ┌─────────────┐
//!                    │   Created   │ 初始态
//!                    └──────┬──────┘
//!                           │ validator
//!                           ▼
//!                    ┌─────────────┐
//!                    │  Validated  │
//!                    └──────┬──────┘
//!                           │ oms
//!                           ▼
//!                 ┌───────────────────┐
//!                 │  PendingSubmit    │ 决策完成，排队中
//!                 └──────┬────────────┘
//!                        │ oms
//!                        ▼
//!                 ┌───────────────────┐
//!                 │    Submitted      │ 已发往 Gateway
//!                 └──────┬────────────┘
//!                        │ gateway
//!                        ▼
//!                 ┌───────────────────┐
//!       ┌────────►│    Accepted       │
//!       │         └────┬──────┬───────┘
//!       │              │      │ gateway
//!       │              ▼      ▼
//!       │       ┌──────────┐ ┌──────────┐
//!       │       │Partial.. │ │ Cancelled│（终态）
//!       │       └────┬─────┘ └──────────┘
//!       │            │
//!       │            ▼
//!       │     ┌──────────┐
//!       └─────│  Filled  │（终态）
//!             └──────────┘
//!
//!   Created/Validated/PendingSubmit/Submitted/Accepted/PartiallyFilled
//!   任一非终态可进入：Rejected / Expired（终态）
//! ```

use crate::order::{OrderStatus, StatusChange};
use chrono::{DateTime, Local};
use std::collections::HashMap;

// ============================================================================
// StateTransition — 单次合法转移定义
// ============================================================================

/// 状态机转移描述（from → to）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateTransition {
    pub from: OrderStatus,
    pub to: OrderStatus,
}

impl StateTransition {
    pub const fn new(from: OrderStatus, to: OrderStatus) -> Self {
        Self { from, to }
    }
}

// ============================================================================
// StateMachine
// ============================================================================

/// OMS 状态机：管理所有合法状态转移。
#[derive(Debug, Clone)]
pub struct StateMachine {
    /// 合法转移表（from → [to]）。
    transitions: HashMap<OrderStatus, Vec<OrderStatus>>,
    /// 所有状态列表（用于 diagram 渲染）。
    all_states: Vec<OrderStatus>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    /// 创建并初始化标准状态机。
    pub fn new() -> Self {
        let mut transitions: HashMap<OrderStatus, Vec<OrderStatus>> = HashMap::new();

        // Created → Validated | PendingSubmit | Cancelled | Rejected | Expired
        transitions.insert(
            OrderStatus::Created,
            vec![
                OrderStatus::Validated,
                OrderStatus::PendingSubmit,
                OrderStatus::Cancelled,
                OrderStatus::Rejected,
                OrderStatus::Expired,
            ],
        );

        // Validated → PendingSubmit | Cancelled | Rejected | Expired
        transitions.insert(
            OrderStatus::Validated,
            vec![
                OrderStatus::PendingSubmit,
                OrderStatus::Cancelled,
                OrderStatus::Rejected,
                OrderStatus::Expired,
            ],
        );

        // PendingSubmit → Submitted | Cancelled | Rejected | Expired
        transitions.insert(
            OrderStatus::PendingSubmit,
            vec![
                OrderStatus::Submitted,
                OrderStatus::Cancelled,
                OrderStatus::Rejected,
                OrderStatus::Expired,
            ],
        );

        // Submitted → Accepted | PartiallyFilled | Filled | Cancelled | Rejected | Expired
        transitions.insert(
            OrderStatus::Submitted,
            vec![
                OrderStatus::Accepted,
                OrderStatus::PartiallyFilled,
                OrderStatus::Filled,
                OrderStatus::Cancelled,
                OrderStatus::Rejected,
                OrderStatus::Expired,
            ],
        );

        // Accepted → PartiallyFilled | Filled | Cancelled | Rejected | Expired
        transitions.insert(
            OrderStatus::Accepted,
            vec![
                OrderStatus::PartiallyFilled,
                OrderStatus::Filled,
                OrderStatus::Cancelled,
                OrderStatus::Rejected,
                OrderStatus::Expired,
            ],
        );

        // PartiallyFilled → Filled | PartiallyFilled | Cancelled | Rejected | Expired
        transitions.insert(
            OrderStatus::PartiallyFilled,
            vec![
                OrderStatus::PartiallyFilled,
                OrderStatus::Filled,
                OrderStatus::Cancelled,
                OrderStatus::Rejected,
                OrderStatus::Expired,
            ],
        );

        // 终态：Filled / Cancelled / Rejected / Expired → 无转移
        // Completed：聚合态，运行时无转移

        let all_states = vec![
            OrderStatus::Created,
            OrderStatus::Validated,
            OrderStatus::PendingSubmit,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::PartiallyFilled,
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
            OrderStatus::Expired,
            OrderStatus::Completed,
        ];

        Self {
            transitions,
            all_states,
        }
    }

    /// 校验转移是否合法。
    ///
    /// # 参数
    ///
    /// - `from`：当前状态。
    /// - `to`：目标状态。
    ///
    /// # 返回
    ///
    /// - `Ok(())`：合法。
    /// - `Err(TransitionError)`：非法，附带中文原因。
    pub fn validate_transition(
        &self,
        from: OrderStatus,
        to: OrderStatus,
    ) -> Result<(), TransitionError> {
        // 终态禁止再转移
        if from.is_terminal() {
            return Err(TransitionError::from_terminal(
                from,
                to,
                format!("状态「{}」为终态，不允许再转移", from.as_zh()),
            ));
        }

        // 聚合态禁止作为 from
        if from == OrderStatus::Completed {
            return Err(TransitionError::from_terminal(
                from,
                to,
                "状态「已完成」为聚合态，不允许作为转移起点".into(),
            ));
        }

        // 同状态自转：仅 PartiallyFilled 合法（多次部分成交）；其余一律拒绝
        if from == to && from != OrderStatus::PartiallyFilled {
            return Err(TransitionError::same_state(
                from,
                format!("不允许自转（{} → {}）", from.as_zh(), to.as_zh()),
            ));
        }

        // 查白名单
        match self.transitions.get(&from) {
            Some(allowed) if allowed.contains(&to) => Ok(()),
            Some(allowed) => Err(TransitionError::not_allowed(
                from,
                to,
                format!(
                    "从「{}」到「{}」的转移不在白名单（允许的目标：{}）",
                    from.as_zh(),
                    to.as_zh(),
                    allowed
                        .iter()
                        .map(|s| s.as_zh())
                        .collect::<Vec<_>>()
                        .join("、")
                ),
            )),
            None => Err(TransitionError::not_allowed(
                from,
                to,
                format!("状态「{}」无任何合法转移", from.as_zh()),
            )),
        }
    }

    /// 获取某个状态允许的目标状态列表。
    pub fn allowed_targets(&self, from: OrderStatus) -> Vec<OrderStatus> {
        self.transitions.get(&from).cloned().unwrap_or_default()
    }

    /// 返回全部状态。
    pub fn all_states(&self) -> &[OrderStatus] {
        &self.all_states
    }

    /// 渲染 ASCII 状态机图（中文）。
    pub fn diagram_zh() -> String {
        let mut out = String::new();
        out.push_str("OMS 订单状态机（11 态 + 1 聚合）：\n");
        out.push_str("\n");
        out.push_str("  已创建 (Created)\n");
        out.push_str("     │ validator 通过\n");
        out.push_str("     ▼\n");
        out.push_str("  已校验 (Validated)\n");
        out.push_str("     │ OMS 决策\n");
        out.push_str("     ▼\n");
        out.push_str("  待提交 (PendingSubmit)\n");
        out.push_str("     │ oms 提交\n");
        out.push_str("     ▼\n");
        out.push_str("  已提交 (Submitted)\n");
        out.push_str("     │ gateway 接受\n");
        out.push_str("     ▼\n");
        out.push_str("  已接受 (Accepted) ─── gateway 取消 ──► 已取消 (Cancelled)（终态）\n");
        out.push_str("     │\n");
        out.push_str("     ├── 部分成交 ──► 部分成交 (PartiallyFilled) ──► 部分成交 （持续）\n");
        out.push_str("     │                    │\n");
        out.push_str("     │                    └─► 完全成交 (Filled)（终态）\n");
        out.push_str("     └─► 完全成交 (Filled)（终态）\n");
        out.push_str("\n");
        out.push_str("  任一非终态（已创建/已校验/待提交/已提交/已接受/部分成交）\n");
        out.push_str("     可进入 已拒绝 (Rejected) / 已过期 (Expired)（终态）\n");
        out.push_str("\n");
        out.push_str("  已完成 (Completed)：聚合终态，用于统计展示。\n");
        out.push('\n');
        out
    }

    /// 把 OrderStatus 列表渲染成简短一行（CLI 用）。
    pub fn status_line_zh(statuses: &[OrderStatus]) -> String {
        statuses
            .iter()
            .map(|s| s.as_zh())
            .collect::<Vec<_>>()
            .join(" → ")
    }
}

// ============================================================================
// TransitionError
// ============================================================================

/// 状态机转移错误（中文）。
#[derive(Debug, Clone, thiserror::Error)]
pub enum TransitionError {
    #[error("从终态转移被拒绝：{message}")]
    FromTerminal {
        message: String,
        from: OrderStatus,
        to: OrderStatus,
    },

    #[error("不允许自转：{message}")]
    SameState { message: String, state: OrderStatus },

    #[error("转移路径不在白名单：{message}")]
    NotAllowed {
        message: String,
        from: OrderStatus,
        to: OrderStatus,
    },
}

impl TransitionError {
    fn from_terminal(from: OrderStatus, to: OrderStatus, message: String) -> Self {
        Self::FromTerminal { message, from, to }
    }
    fn same_state(state: OrderStatus, message: String) -> Self {
        Self::SameState { message, state }
    }
    fn not_allowed(from: OrderStatus, to: OrderStatus, message: String) -> Self {
        Self::NotAllowed { message, from, to }
    }
}

// ============================================================================
// StatusChange 应用辅助
// ============================================================================

/// 检查并应用一次状态转移（在 Order 上调用 transition 之前的可选前置校验）。
///
/// 失败时返回错误；调用方可以选择拒绝或记录。
pub fn check_and_record(
    sm: &StateMachine,
    history: &mut Vec<StatusChange>,
    current: OrderStatus,
    target: OrderStatus,
    reason: &str,
    actor: &str,
    now: DateTime<Local>,
) -> Result<(), TransitionError> {
    sm.validate_transition(current, target)?;
    history.push(StatusChange::new(current, target, reason, actor, now));
    Ok(())
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions_allowed() {
        let sm = StateMachine::new();

        let path = [
            OrderStatus::Created,
            OrderStatus::Validated,
            OrderStatus::PendingSubmit,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::PartiallyFilled,
            OrderStatus::Filled,
        ];
        for w in path.windows(2) {
            assert!(
                sm.validate_transition(w[0], w[1]).is_ok(),
                "{:?} → {:?} should be allowed",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn partial_filled_self_loop_allowed() {
        let sm = StateMachine::new();
        assert!(
            sm.validate_transition(OrderStatus::PartiallyFilled, OrderStatus::PartiallyFilled)
                .is_ok()
        );
    }

    #[test]
    fn terminal_states_reject_all_transitions() {
        let sm = StateMachine::new();
        let targets = [
            OrderStatus::Created,
            OrderStatus::Validated,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
            OrderStatus::Expired,
        ];
        for terminal in [
            OrderStatus::Filled,
            OrderStatus::Cancelled,
            OrderStatus::Rejected,
            OrderStatus::Expired,
        ] {
            for t in &targets {
                let result = sm.validate_transition(terminal, *t);
                assert!(
                    result.is_err(),
                    "{:?} → {:?} should be rejected",
                    terminal,
                    t
                );
            }
        }
    }

    #[test]
    fn illegal_skip_transitions_rejected() {
        let sm = StateMachine::new();
        // 跳过 Validated 直接进入 Submitted：非法
        assert!(
            sm.validate_transition(OrderStatus::Created, OrderStatus::Submitted)
                .is_err()
        );
        // Validated 跳到 Accepted：非法
        assert!(
            sm.validate_transition(OrderStatus::Validated, OrderStatus::Accepted)
                .is_err()
        );
    }

    #[test]
    fn rejected_and_expired_from_any_active_state() {
        let sm = StateMachine::new();
        let active = [
            OrderStatus::Created,
            OrderStatus::Validated,
            OrderStatus::PendingSubmit,
            OrderStatus::Submitted,
            OrderStatus::Accepted,
            OrderStatus::PartiallyFilled,
        ];
        for s in active {
            assert!(sm.validate_transition(s, OrderStatus::Rejected).is_ok());
            assert!(sm.validate_transition(s, OrderStatus::Expired).is_ok());
        }
    }

    #[test]
    fn allowed_targets_query() {
        let sm = StateMachine::new();
        let targets = sm.allowed_targets(OrderStatus::Submitted);
        assert!(targets.contains(&OrderStatus::Accepted));
        assert!(targets.contains(&OrderStatus::Cancelled));
        assert!(targets.contains(&OrderStatus::Filled)); // Gateway 可能直接返回 Filled
    }

    #[test]
    fn diagram_chinese_not_empty() {
        let d = StateMachine::diagram_zh();
        assert!(d.contains("OMS"));
        assert!(d.contains("已创建"));
        assert!(d.contains("完全成交"));
    }

    #[test]
    fn diagram_contains_all_statuses() {
        let d = StateMachine::diagram_zh();
        for s in StateMachine::new().all_states.iter() {
            assert!(
                d.contains(s.as_zh()) || d.contains(s.as_str()),
                "diagram 缺少状态 {:?} ({})",
                s,
                s.as_zh()
            );
        }
    }

    #[test]
    fn error_messages_in_chinese() {
        let sm = StateMachine::new();
        let err = sm
            .validate_transition(OrderStatus::Created, OrderStatus::Submitted)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("白名单") || msg.contains("不允许"),
            "got: {}",
            msg
        );

        let err = sm
            .validate_transition(OrderStatus::Filled, OrderStatus::Created)
            .unwrap_err();
        assert!(err.to_string().contains("终态"));
    }
}
