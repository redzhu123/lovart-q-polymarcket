//! OMS 集成测试 — 状态机（P2-04 第十节）。

use pm_oms::prelude::*;

#[test]
fn diagram_zh_contains_all_statuses() {
    let d = StateMachine::diagram_zh();
    for s in StateMachine::new().all_states() {
        assert!(
            d.contains(s.as_zh()) || d.contains(s.as_str()),
            "diagram 缺少 {:?} ({})",
            s,
            s.as_zh()
        );
    }
}

#[test]
fn happy_path_all_legal() {
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
        assert!(sm.validate_transition(w[0], w[1]).is_ok());
    }
}

#[test]
fn terminal_states_immutable() {
    let sm = StateMachine::new();
    let terminals = [
        OrderStatus::Filled,
        OrderStatus::Cancelled,
        OrderStatus::Rejected,
        OrderStatus::Expired,
    ];
    let to_states = [
        OrderStatus::Created,
        OrderStatus::Validated,
        OrderStatus::PendingSubmit,
        OrderStatus::Submitted,
        OrderStatus::Accepted,
    ];
    for t in &terminals {
        for s in &to_states {
            assert!(
                sm.validate_transition(*t, *s).is_err(),
                "{:?} 不应能转移到 {:?}",
                t,
                s
            );
        }
    }
}

#[test]
fn rejected_or_expired_from_any_active_state() {
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
fn partially_filled_self_loop_allowed() {
    let sm = StateMachine::new();
    assert!(
        sm.validate_transition(OrderStatus::PartiallyFilled, OrderStatus::PartiallyFilled)
            .is_ok()
    );
}

#[test]
fn created_can_be_cancelled_directly() {
    let sm = StateMachine::new();
    assert!(
        sm.validate_transition(OrderStatus::Created, OrderStatus::Cancelled)
            .is_ok()
    );
}

#[test]
fn validated_can_be_cancelled() {
    let sm = StateMachine::new();
    assert!(
        sm.validate_transition(OrderStatus::Validated, OrderStatus::Cancelled)
            .is_ok()
    );
}

#[test]
fn illegal_skip_rejected() {
    let sm = StateMachine::new();
    // 跳级
    assert!(
        sm.validate_transition(OrderStatus::Created, OrderStatus::Submitted)
            .is_err()
    );
    assert!(
        sm.validate_transition(OrderStatus::Validated, OrderStatus::Accepted)
            .is_err()
    );
}

#[test]
fn chinese_error_messages() {
    let sm = StateMachine::new();
    let err = sm
        .validate_transition(OrderStatus::Filled, OrderStatus::Created)
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("终态"), "got: {}", msg);
}
