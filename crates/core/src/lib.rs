//! pm-core：跨 crate 公共原语。
//!
//! 仅放被多个 crate 共享、且无业务行为的底层类型：
//! - [`Side`]：订单方向（Buy/Sell），被 pm-portfolio 与 pm-execution 共用，故置于 core 避免相互依赖。
//! - [`CoreError`]：跨 crate 通用错误（thiserror）。
//!
//! 不放任何业务逻辑；业务 struct 归各 engine crate，共享 DTO 归 pm-models。

/// 订单方向。被 pm-portfolio 与 pm-execution 共用，故定义在此。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    /// 用于 CSV 输出与控制台展示的字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        }
    }
}

/// 跨 crate 通用错误。各 crate 的领域错误用各自的 thiserror 类型；此处仅放真正通用的。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("无效参数: {0}")]
    InvalidArgument(String),
    #[error("数据非法: {0}")]
    InvalidData(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_as_str_roundtrip() {
        assert_eq!(Side::Buy.as_str(), "BUY");
        assert_eq!(Side::Sell.as_str(), "SELL");
    }

    #[test]
    fn side_eq_copy() {
        let a = Side::Buy;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(a, Side::Sell);
    }

    #[test]
    fn core_error_display() {
        let e = CoreError::InvalidArgument("price<=0".into());
        assert_eq!(e.to_string(), "无效参数: price<=0");
    }
}
