//! Trading State（V1.07 第十节）。
//!
//! 定义 Trading 组件的所有状态。
//! 所有状态变化必须日志记录。

use tracing::info;

/// Trading 系统状态（V1.07 第十节）。
///
/// 状态流转：
/// ```text
/// Disconnected → Connecting → Connected → Authenticated → Ready
///                                                          ↓
///                                             Paused ↔ Ready
///                                                          ↓
///                                             Recovering → Ready
///                                                          ↓
///                                                       Stopped
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingState {
    /// 未连接。
    Disconnected,
    /// 正在连接。
    Connecting,
    /// 已连接（未认证）。
    Connected,
    /// 已认证（可查询行情，不可下单）。
    Authenticated,
    /// 就绪（完整功能可用）。
    Ready,
    /// 已暂停（人工或风控触发）。
    Paused,
    /// 正在恢复。
    Recovering,
    /// 已停止。
    Stopped,
}

impl TradingState {
    /// 中文展示名。
    pub fn as_zh(&self) -> &'static str {
        match self {
            TradingState::Disconnected => "未连接",
            TradingState::Connecting => "连接中",
            TradingState::Connected => "已连接",
            TradingState::Authenticated => "已认证",
            TradingState::Ready => "就绪",
            TradingState::Paused => "已暂停",
            TradingState::Recovering => "恢复中",
            TradingState::Stopped => "已停止",
        }
    }

    /// 是否为运行中状态（可接受新请求）。
    pub fn is_operational(&self) -> bool {
        matches!(self, TradingState::Ready | TradingState::Authenticated)
    }

    /// 是否为终态。
    pub fn is_terminal(&self) -> bool {
        matches!(self, TradingState::Stopped)
    }

    /// 是否允许自动恢复。
    pub fn can_recover(&self) -> bool {
        matches!(
            self,
            TradingState::Disconnected | TradingState::Connected | TradingState::Authenticated
        )
    }

    /// 状态转换并记录日志。
    pub fn transition_to(&mut self, new_state: TradingState) {
        let old = *self;
        if old == new_state {
            return;
        }
        *self = new_state;
        info!("Trading 状态变更: {} → {}", old.as_zh(), new_state.as_zh());
    }
}

impl Default for TradingState {
    fn default() -> Self {
        TradingState::Disconnected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_as_zh() {
        assert_eq!(TradingState::Disconnected.as_zh(), "未连接");
        assert_eq!(TradingState::Ready.as_zh(), "就绪");
        assert_eq!(TradingState::Stopped.as_zh(), "已停止");
    }

    #[test]
    fn state_operational() {
        assert!(!TradingState::Disconnected.is_operational());
        assert!(!TradingState::Connecting.is_operational());
        assert!(TradingState::Authenticated.is_operational());
        assert!(TradingState::Ready.is_operational());
        assert!(!TradingState::Paused.is_operational());
        assert!(!TradingState::Stopped.is_operational());
    }

    #[test]
    fn state_terminal() {
        assert!(TradingState::Stopped.is_terminal());
        assert!(!TradingState::Ready.is_terminal());
    }

    #[test]
    fn state_can_recover() {
        assert!(TradingState::Disconnected.can_recover());
        assert!(TradingState::Connected.can_recover());
        assert!(!TradingState::Ready.can_recover());
        assert!(!TradingState::Stopped.can_recover());
    }

    #[test]
    fn default_is_disconnected() {
        assert_eq!(TradingState::default(), TradingState::Disconnected);
    }

    #[test]
    fn transition_records() {
        let mut state = TradingState::Disconnected;
        state.transition_to(TradingState::Connecting);
        assert_eq!(state, TradingState::Connecting);
    }

    #[test]
    fn transition_noop_same_state() {
        let mut state = TradingState::Ready;
        state.transition_to(TradingState::Ready);
        assert_eq!(state, TradingState::Ready);
    }
}
