//! Provider Capability（V1.07 第七节）。
//!
//! 声明每个 Provider 的能力，启动时输出能力表格。
//! 所有能力检查必须通过本模块，禁止写死布尔值。

use std::fmt;

// ============================================================================
// Capability Flags
// ============================================================================

/// Provider 能力声明（V1.07 第七节）。
///
/// 每个 Provider 必须在构造时声明其能力。
/// 启动时打印能力表格供运维确认。
#[derive(Debug, Clone)]
pub struct Capability {
    /// Provider 名称。
    pub provider_name: String,
    /// 是否支持连接。
    pub can_connect: bool,
    /// 是否支持行情查询。
    pub can_market_data: bool,
    /// 是否支持订单簿。
    pub can_orderbook: bool,
    /// 是否支持下单（Dry Run 模式下为 false）。
    pub can_order: bool,
    /// 是否支持签名。
    pub can_sign: bool,
    /// 是否支持钱包操作。
    pub can_wallet: bool,
    /// 是否支持真实交易（与钱包/签名组合）。
    pub can_real_trading: bool,
    /// 是否支持 WebSocket。
    pub can_websocket: bool,
    /// 是否支持取消订单。
    pub can_cancel: bool,
    /// 是否支持查询持仓。
    pub can_positions: bool,
    /// 是否支持查询余额。
    pub can_balance: bool,
    /// 支持的深度档位数（0 表示不支持订单簿深度）。
    pub depth_levels: usize,
    /// 速率限制（每秒最大请求数，0 表示无限制）。
    pub rate_limit: u32,
    /// 额外说明。
    pub notes: Vec<String>,
}

impl Capability {
    /// Mock Provider 的能力。
    pub fn mock() -> Self {
        Self {
            provider_name: "Mock".to_string(),
            can_connect: true,
            can_market_data: true,
            can_orderbook: true,
            can_order: false,
            can_sign: false,
            can_wallet: false,
            can_real_trading: false,
            can_websocket: false,
            can_cancel: true,
            can_positions: true,
            can_balance: true,
            depth_levels: 10,
            rate_limit: 0,
            notes: vec!["模拟数据".to_string(), "不产生真实订单".to_string()],
        }
    }

    /// Polymarket Provider 的能力（未来实现）。
    pub fn polymarket() -> Self {
        Self {
            provider_name: "Polymarket".to_string(),
            can_connect: true,
            can_market_data: true,
            can_orderbook: true,
            can_order: true,
            can_sign: true,
            can_wallet: true,
            can_real_trading: true,
            can_websocket: true,
            can_cancel: true,
            can_positions: true,
            can_balance: true,
            depth_levels: 10,
            rate_limit: 10,
            notes: vec![
                "真实 CLOB 订单簿".to_string(),
                "需 API Key + Secret + Wallet".to_string(),
                "Polygon 链上结算".to_string(),
            ],
        }
    }

    /// 渲染为能力表格（中文，V1.07 第七节格式）。
    pub fn render_table(&self) -> String {
        let check = |b: bool| if b { "✅" } else { "❌" };
        format!(
            "\
【Provider 能力】
{}
连接        {}
行情        {}
订单簿      {}
下单        {}
签名        {}
钱包        {}
真实交易    {}
WebSocket   {}
取消订单    {}
查询持仓    {}
查询余额    {}
深度档位    {}
速率限制    {} 次/秒\
",
            self.provider_name,
            check(self.can_connect),
            check(self.can_market_data),
            check(self.can_orderbook),
            check(self.can_order),
            check(self.can_sign),
            check(self.can_wallet),
            check(self.can_real_trading),
            check(self.can_websocket),
            check(self.can_cancel),
            check(self.can_positions),
            check(self.can_balance),
            self.depth_levels,
            if self.rate_limit > 0 {
                self.rate_limit.to_string()
            } else {
                "无限制".to_string()
            },
        )
    }

    /// 对比两个 Provider 的能力差异。
    pub fn diff(a: &Capability, b: &Capability) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "【能力对比】{} vs {}",
            a.provider_name, b.provider_name
        ));
        lines.push(String::new());

        let fields: Vec<(&str, &dyn Fn(&Capability) -> bool)> = vec![
            ("连接", &|c: &Capability| c.can_connect),
            ("行情", &|c: &Capability| c.can_market_data),
            ("订单簿", &|c: &Capability| c.can_orderbook),
            ("下单", &|c: &Capability| c.can_order),
            ("签名", &|c: &Capability| c.can_sign),
            ("钱包", &|c: &Capability| c.can_wallet),
            ("真实交易", &|c: &Capability| c.can_real_trading),
            ("WebSocket", &|c: &Capability| c.can_websocket),
            ("取消订单", &|c: &Capability| c.can_cancel),
            ("查询持仓", &|c: &Capability| c.can_positions),
            ("查询余额", &|c: &Capability| c.can_balance),
        ];

        for (name, getter) in fields {
            let a_val = getter(a);
            let b_val = getter(b);
            let same = if a_val == b_val { "" } else { " ⚠️ 差异" };
            lines.push(format!(
                "  {:<10}  {}  |  {}  {}",
                name,
                if a_val { "✅" } else { "❌" },
                if b_val { "✅" } else { "❌" },
                same,
            ));
        }

        lines.join("\n")
    }
}

impl Default for Capability {
    fn default() -> Self {
        Capability::mock()
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render_table())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_capability_no_real_trading() {
        let cap = Capability::mock();
        assert!(cap.can_connect);
        assert!(cap.can_market_data);
        assert!(!cap.can_order);
        assert!(!cap.can_real_trading);
        assert!(!cap.can_sign);
    }

    #[test]
    fn polymarket_capability_full() {
        let cap = Capability::polymarket();
        assert!(cap.can_connect);
        assert!(cap.can_real_trading);
        assert!(cap.can_sign);
        assert!(cap.can_wallet);
        assert!(cap.can_websocket);
    }

    #[test]
    fn render_table_contains_checks() {
        let cap = Capability::mock();
        let table = cap.render_table();
        assert!(table.contains("Mock"));
        assert!(table.contains("✅"));
        assert!(table.contains("❌"));
        assert!(table.contains("连接"));
    }

    #[test]
    fn diff_detects_differences() {
        let mock = Capability::mock();
        let pm = Capability::polymarket();
        let diff = Capability::diff(&mock, &pm);
        assert!(diff.contains("⚠️ 差异"));
    }

    #[test]
    fn default_is_mock() {
        let cap = Capability::default();
        assert_eq!(cap.provider_name, "Mock");
    }
}
