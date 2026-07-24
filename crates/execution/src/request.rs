//! Execution Request（V1.06 第二节）。
//!
//! Execution Request 是 Strategy 通过 Risk 后向 Execution Pipeline 提交的请求。
//! 它是 Pipeline 的唯一入口，任何模块不得绕过。

use pm_core::Side;

use crate::order::Direction;

/// 执行请求 —— Strategy → Execution Pipeline 的输入。
///
/// 包含一个交易决策的所有必要信息。
/// Strategy 负责创建 Request，Risk 负责批准（risk_id 字段），
/// Execution 负责执行。
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    /// 市场 ID。
    pub market_id: String,
    /// 问题描述（用于日志/展示）。
    pub question: String,
    /// 数据源 Provider（"gamma" | "clob" | "mock"）。
    pub provider: String,
    /// 订单方向（YES / NO）。
    pub direction: Direction,
    /// 买卖方向。
    pub side: Side,
    /// 下单价格。
    pub price: f64,
    /// 下单数量（份额）。
    pub quantity: f64,
    /// 策略 ID（谁发起的）。
    pub strategy_id: String,
    /// 风控 ID（哪个 Risk 决策批准的，空串表示未经过 Risk）。
    pub risk_id: String,
    /// 机会 ID（关联的套利机会）。
    pub opportunity_id: String,
    /// 优先级（越大越优先，默认 0）。
    pub priority: u32,
    /// 客户端订单 ID（调用方指定，用于去重，空串则自动生成）。
    pub client_order_id: String,
}

impl ExecutionRequest {
    /// 创建新的执行请求。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        market_id: &str,
        question: &str,
        provider: &str,
        direction: Direction,
        side: Side,
        price: f64,
        quantity: f64,
        strategy_id: &str,
        risk_id: &str,
        opportunity_id: &str,
    ) -> Self {
        Self {
            market_id: market_id.to_string(),
            question: question.to_string(),
            provider: provider.to_string(),
            direction,
            side,
            price,
            quantity,
            strategy_id: strategy_id.to_string(),
            risk_id: risk_id.to_string(),
            opportunity_id: opportunity_id.to_string(),
            priority: 0,
            client_order_id: String::new(),
        }
    }

    /// 设置优先级（链式调用）。
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// 设置客户端订单 ID（链式调用）。
    pub fn with_client_order_id(mut self, id: &str) -> Self {
        self.client_order_id = id.to_string();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request() {
        let req = ExecutionRequest::new(
            "mkt-1",
            "测试市场?",
            "mock",
            Direction::Yes,
            Side::Buy,
            0.45,
            222.22,
            "TestStrategy",
            "RISK-OK",
            "OPP-01",
        );
        assert_eq!(req.market_id, "mkt-1");
        assert_eq!(req.question, "测试市场?");
        assert!((req.price - 0.45).abs() < 1e-9);
        assert_eq!(req.priority, 0);
    }

    #[test]
    fn builder_pattern() {
        let req = ExecutionRequest::new(
            "mkt-1",
            "Q?",
            "mock",
            Direction::Yes,
            Side::Buy,
            0.5,
            100.0,
            "S",
            "R",
            "O",
        )
        .with_priority(10)
        .with_client_order_id("my-id");
        assert_eq!(req.priority, 10);
        assert_eq!(req.client_order_id, "my-id");
    }
}
