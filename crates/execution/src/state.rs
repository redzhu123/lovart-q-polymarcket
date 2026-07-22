//! 订单状态机（Execution Simulator）。
//!
//! Simulation Only -- 仅为模拟成交生命周期定义的状态枚举，不对应任何真实交易所状态。
//!
//! 订单生命周期：
//!   Pending -> PartiallyFilled -> Filled            （正常成交，可能分多批）
//!   Pending -> PartiallyFilled -> Cancelled         （部分成交后超时，保留已成交部分）
//!   Pending -> Cancelled                            （超时取消，无成交；当前模型下极少出现）
//!   Pending -> Expired                              （流动性失败 / 超时，整单零成交作废）
//!   （提交即被风控拦截）-> Rejected                  （未进入 Pending）
//!
//! Cancelled 与 Expired 的区别（与 fill 模块配合）：
//!   - Cancelled：到达最大等待时间时已有部分成交 -> 保留已成交部分，取消剩余。
//!   - Expired  ：到达最大等待时间时一次未成交（含"流动性失败"模拟）-> 整单作废、释放全部锁定资金。

/// 订单状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    /// 已提交，等待成交。
    Pending,
    /// 部分成交（已成交一部分，仍有剩余待成交）。
    PartiallyFilled,
    /// 完全成交。
    Filled,
    /// 已取消（部分成交后超时，保留已成交部分）。
    Cancelled,
    /// 已过期（超时且零成交，整单作废）。
    Expired,
    /// 已拒绝（提交时被风控拦截，未进入 Pending）。
    Rejected,
}

impl OrderStatus {
    /// 用于 CSV 输出与控制台展示的字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "Pending",
            OrderStatus::PartiallyFilled => "PartiallyFilled",
            OrderStatus::Filled => "Filled",
            OrderStatus::Cancelled => "Cancelled",
            OrderStatus::Expired => "Expired",
            OrderStatus::Rejected => "Rejected",
        }
    }

    /// 是否为终态（不再变化）。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled
                | OrderStatus::Cancelled
                | OrderStatus::Expired
                | OrderStatus::Rejected
        )
    }
}

/// 终态原因（写入 execution_orders.csv 的 cancel_reason 列）。
/// 非取消 / 过期 / 拒绝终态（如 Filled、或仍 Pending / PartiallyFilled）记为 None。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalReason {
    /// 超过最大等待时间（-> Cancelled 或 Expired）。
    Timeout,
    /// 待处理订单数已达上限（-> Rejected）。
    MaxPending,
    /// 可用现金不足（-> Rejected）。
    InsufficientCash,
    /// 价格非法（-> Rejected）。
    InvalidPrice,
    /// SELL 时找不到可平仓位（-> Rejected）。
    NoPosition,
    /// 非终态 / 正常成交，无原因。
    None,
}

impl TerminalReason {
    /// 用于 CSV 输出与控制台展示的字符串（None 输出空串）。
    pub fn as_str(&self) -> &'static str {
        match self {
            TerminalReason::Timeout => "Timeout",
            TerminalReason::MaxPending => "MaxPending",
            TerminalReason::InsufficientCash => "InsufficientCash",
            TerminalReason::InvalidPrice => "InvalidPrice",
            TerminalReason::NoPosition => "NoPosition",
            TerminalReason::None => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_detection() {
        assert!(OrderStatus::Filled.is_terminal());
        assert!(OrderStatus::Cancelled.is_terminal());
        assert!(OrderStatus::Expired.is_terminal());
        assert!(OrderStatus::Rejected.is_terminal());
        assert!(!OrderStatus::Pending.is_terminal());
        assert!(!OrderStatus::PartiallyFilled.is_terminal());
    }

    #[test]
    fn reason_as_str() {
        assert_eq!(TerminalReason::Timeout.as_str(), "Timeout");
        assert_eq!(TerminalReason::None.as_str(), "");
    }
}
