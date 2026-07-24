//! 市场能力声明系统（P3.0 第三节）。
//!
//! 每个市场通过此模块声明自己的能力。
//! 系统根据能力自动启用功能，不得写死布尔值。
//!
//! # 设计原则
//!
//! - 使用枚举而非布尔值，确保类型安全
//! - 每个市场在注册时必须声明完整能力集
//! - 能力查询通过 `has_capability()` 方法，禁止直接访问字段

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

// ============================================================================
// MarketCapability 枚举
// ============================================================================

/// 市场能力枚举（P3.0 第三节）。
///
/// 声明每个市场支持的能力。系统根据这些能力自动启用功能。
///
/// # 分类
///
/// - **数据能力**：ReadMarket / ReadOrderBook / ReadTrades / HistoricalData
/// - **交易能力**：PaperTrading / LiveTrading / CancelOrder / ReplaceOrder
/// - **账户能力**：Wallet / Balance / Settlement
/// - **传输能力**：Rest / WebSocket / Streaming / FIX
/// - **市场类型**：Spot / Margin / Perpetual / Futures / Options / Prediction
/// - **扩展能力**：MultiAsset / MultiChain / CrossMargin / IsolatedMargin
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketCapability {
    // ===== 数据能力 =====
    /// 读取市场列表。
    ReadMarket,
    /// 读取订单簿。
    ReadOrderBook,
    /// 读取成交记录。
    ReadTrades,
    /// 读取历史数据（K线、历史订单簿等）。
    HistoricalData,

    // ===== 交易能力 =====
    /// 纸面交易（模拟交易）。
    PaperTrading,
    /// 真实交易（需钱包/签名/API Key）。
    LiveTrading,
    /// 取消订单。
    CancelOrder,
    /// 替换订单（Cancel + New）。
    ReplaceOrder,
    /// 批量下单。
    BatchOrders,

    // ===== 账户能力 =====
    /// 钱包操作。
    Wallet,
    /// 余额查询。
    Balance,
    /// 结算。
    Settlement,

    // ===== 传输能力 =====
    /// REST API。
    Rest,
    /// WebSocket 实时推送。
    WebSocket,
    /// 流式数据（gRPC stream / SSE 等）。
    Streaming,
    /// FIX 协议。
    FIX,

    // ===== 市场类型 =====
    /// 现货市场。
    Spot,
    /// 保证金市场（接口预留）。
    Margin,
    /// 永续合约（接口预留）。
    Perpetual,
    /// 交割合约。
    Futures,
    /// 期权。
    Options,
    /// 预测市场（如 Polymarket）。
    Prediction,

    // ===== 扩展能力 =====
    /// 多资产支持。
    MultiAsset,
    /// 多链支持。
    MultiChain,
    /// 全仓保证金。
    CrossMargin,
    /// 逐仓保证金。
    IsolatedMargin,

    // ===== 高级能力 =====
    /// 杠杆交易。
    Leverage,
    /// 质押/借贷。
    Staking,
    /// Launchpad / IEO。
    Launchpad,
}

impl MarketCapability {
    /// 能力的中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            // 数据能力
            MarketCapability::ReadMarket => "读取市场",
            MarketCapability::ReadOrderBook => "读取订单簿",
            MarketCapability::ReadTrades => "读取成交",
            MarketCapability::HistoricalData => "历史数据",
            // 交易能力
            MarketCapability::PaperTrading => "纸面交易",
            MarketCapability::LiveTrading => "真实交易",
            MarketCapability::CancelOrder => "取消订单",
            MarketCapability::ReplaceOrder => "替换订单",
            MarketCapability::BatchOrders => "批量下单",
            // 账户能力
            MarketCapability::Wallet => "钱包",
            MarketCapability::Balance => "余额查询",
            MarketCapability::Settlement => "结算",
            // 传输能力
            MarketCapability::Rest => "REST API",
            MarketCapability::WebSocket => "WebSocket",
            MarketCapability::Streaming => "流式数据",
            MarketCapability::FIX => "FIX 协议",
            // 市场类型
            MarketCapability::Spot => "现货",
            MarketCapability::Margin => "保证金",
            MarketCapability::Perpetual => "永续合约",
            MarketCapability::Futures => "交割合约",
            MarketCapability::Options => "期权",
            MarketCapability::Prediction => "预测市场",
            // 扩展能力
            MarketCapability::MultiAsset => "多资产",
            MarketCapability::MultiChain => "多链",
            MarketCapability::CrossMargin => "全仓保证金",
            MarketCapability::IsolatedMargin => "逐仓保证金",
            // 高级能力
            MarketCapability::Leverage => "杠杆交易",
            MarketCapability::Staking => "质押/借贷",
            MarketCapability::Launchpad => "Launchpad",
        }
    }

    /// 能力的分类（中文）。
    pub fn category_zh(&self) -> &'static str {
        match self {
            MarketCapability::ReadMarket
            | MarketCapability::ReadOrderBook
            | MarketCapability::ReadTrades
            | MarketCapability::HistoricalData => "数据能力",

            MarketCapability::PaperTrading
            | MarketCapability::LiveTrading
            | MarketCapability::CancelOrder
            | MarketCapability::ReplaceOrder
            | MarketCapability::BatchOrders => "交易能力",

            MarketCapability::Wallet | MarketCapability::Balance | MarketCapability::Settlement => {
                "账户能力"
            }

            MarketCapability::Rest
            | MarketCapability::WebSocket
            | MarketCapability::Streaming
            | MarketCapability::FIX => "传输能力",

            MarketCapability::Spot
            | MarketCapability::Margin
            | MarketCapability::Perpetual
            | MarketCapability::Futures
            | MarketCapability::Options
            | MarketCapability::Prediction => "市场类型",

            MarketCapability::MultiAsset
            | MarketCapability::MultiChain
            | MarketCapability::CrossMargin
            | MarketCapability::IsolatedMargin => "扩展能力",

            MarketCapability::Leverage
            | MarketCapability::Staking
            | MarketCapability::Launchpad => "高级能力",
        }
    }

    /// 是否为交易相关能力。
    pub fn is_trading(&self) -> bool {
        matches!(
            self,
            MarketCapability::PaperTrading
                | MarketCapability::LiveTrading
                | MarketCapability::CancelOrder
                | MarketCapability::ReplaceOrder
                | MarketCapability::BatchOrders
        )
    }

    /// 是否为数据相关能力。
    pub fn is_data(&self) -> bool {
        matches!(
            self,
            MarketCapability::ReadMarket
                | MarketCapability::ReadOrderBook
                | MarketCapability::ReadTrades
                | MarketCapability::HistoricalData
        )
    }
}

impl fmt::Display for MarketCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_zh())
    }
}

// ============================================================================
// CapabilitySet 能力集合
// ============================================================================

/// 市场能力集合。
///
/// 每个市场在注册时必须声明完整的 [`CapabilitySet`]。
/// 支持并集、交集、差集操作，便于能力查询和比对。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitySet {
    capabilities: HashSet<MarketCapability>,
}

impl CapabilitySet {
    /// 创建空的能力集合。
    pub fn new() -> Self {
        Self {
            capabilities: HashSet::new(),
        }
    }

    /// 从枚举数组创建能力集合。
    pub fn from_caps(caps: &[MarketCapability]) -> Self {
        Self {
            capabilities: caps.iter().cloned().collect(),
        }
    }

    /// 添加一个能力。
    pub fn add(&mut self, cap: MarketCapability) {
        self.capabilities.insert(cap);
    }

    /// 批量添加能力。
    pub fn add_all(&mut self, caps: &[MarketCapability]) {
        for cap in caps {
            self.capabilities.insert(cap.clone());
        }
    }

    /// 移除一个能力。
    pub fn remove(&mut self, cap: &MarketCapability) {
        self.capabilities.remove(cap);
    }

    /// 检查是否拥有某个能力。
    pub fn has(&self, cap: &MarketCapability) -> bool {
        self.capabilities.contains(cap)
    }

    /// 检查是否拥有所有指定能力。
    pub fn has_all(&self, caps: &[MarketCapability]) -> bool {
        caps.iter().all(|c| self.has(c))
    }

    /// 检查是否拥有至少一个指定能力。
    pub fn has_any(&self, caps: &[MarketCapability]) -> bool {
        caps.iter().any(|c| self.has(c))
    }

    /// 能力数量。
    pub fn count(&self) -> usize {
        self.capabilities.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// 列出所有能力。
    pub fn list_all(&self) -> Vec<MarketCapability> {
        let mut caps: Vec<MarketCapability> = self.capabilities.iter().cloned().collect();
        caps.sort_by(|a, b| a.as_zh().cmp(b.as_zh()));
        caps
    }

    /// 按分类列出能力。
    pub fn list_by_category(&self, category: &str) -> Vec<MarketCapability> {
        self.capabilities
            .iter()
            .filter(|c| c.category_zh() == category)
            .cloned()
            .collect()
    }

    /// 所有分类。
    pub fn categories(&self) -> Vec<String> {
        let mut cats: HashSet<String> = self
            .capabilities
            .iter()
            .map(|c| c.category_zh().to_string())
            .collect();
        let mut sorted: Vec<String> = cats.drain().collect();
        sorted.sort();
        sorted
    }

    /// 并集：合并两个能力集合。
    pub fn union(&self, other: &CapabilitySet) -> CapabilitySet {
        let mut result = self.capabilities.clone();
        for cap in &other.capabilities {
            result.insert(cap.clone());
        }
        CapabilitySet {
            capabilities: result,
        }
    }

    /// 交集：两个能力集合的共同能力。
    pub fn intersection(&self, other: &CapabilitySet) -> CapabilitySet {
        let result: HashSet<MarketCapability> = self
            .capabilities
            .intersection(&other.capabilities)
            .cloned()
            .collect();
        CapabilitySet {
            capabilities: result,
        }
    }

    /// 差集：我有而对方没有的能力。
    pub fn difference(&self, other: &CapabilitySet) -> CapabilitySet {
        let result: HashSet<MarketCapability> = self
            .capabilities
            .difference(&other.capabilities)
            .cloned()
            .collect();
        CapabilitySet {
            capabilities: result,
        }
    }

    /// 渲染为中文表格。
    pub fn render_table(&self, title: &str) -> String {
        let mut lines = vec![format!("【{}】", title), String::new()];

        for category in self.categories() {
            let caps = self.list_by_category(&category);
            if caps.is_empty() {
                continue;
            }
            lines.push(format!("  {}:", category));
            for cap in &caps {
                lines.push(format!("    ✅ {}", cap.as_zh()));
            }
        }

        lines.join("\n")
    }

    // ===== 预定义的能力集合 =====

    /// 预测市场（Polymarket 风格）的能力集合。
    pub fn prediction_market_full() -> Self {
        Self::from_caps(&[
            // 数据
            MarketCapability::ReadMarket,
            MarketCapability::ReadOrderBook,
            MarketCapability::ReadTrades,
            MarketCapability::HistoricalData,
            // 交易
            MarketCapability::PaperTrading,
            MarketCapability::LiveTrading,
            MarketCapability::CancelOrder,
            MarketCapability::ReplaceOrder,
            // 账户
            MarketCapability::Wallet,
            MarketCapability::Balance,
            MarketCapability::Settlement,
            // 传输
            MarketCapability::Rest,
            MarketCapability::WebSocket,
            // 市场类型
            MarketCapability::Prediction,
            // 扩展
            MarketCapability::MultiChain,
        ])
    }

    /// 现货交易所（Binance 风格）的能力集合。
    pub fn spot_exchange_full() -> Self {
        Self::from_caps(&[
            // 数据
            MarketCapability::ReadMarket,
            MarketCapability::ReadOrderBook,
            MarketCapability::ReadTrades,
            MarketCapability::HistoricalData,
            // 交易
            MarketCapability::PaperTrading,
            MarketCapability::LiveTrading,
            MarketCapability::CancelOrder,
            MarketCapability::ReplaceOrder,
            MarketCapability::BatchOrders,
            // 账户
            MarketCapability::Wallet,
            MarketCapability::Balance,
            // 传输
            MarketCapability::Rest,
            MarketCapability::WebSocket,
            MarketCapability::Streaming,
            // 市场类型
            MarketCapability::Spot,
            MarketCapability::Margin,
            MarketCapability::Perpetual,
            // 扩展
            MarketCapability::MultiAsset,
            MarketCapability::CrossMargin,
            MarketCapability::IsolatedMargin,
            // 高级
            MarketCapability::Leverage,
            MarketCapability::Staking,
            MarketCapability::Launchpad,
        ])
    }

    /// 仅数据（只读观察者）的能力集合。
    pub fn data_only() -> Self {
        Self::from_caps(&[
            MarketCapability::ReadMarket,
            MarketCapability::ReadOrderBook,
            MarketCapability::ReadTrades,
            MarketCapability::HistoricalData,
            MarketCapability::Rest,
        ])
    }

    /// 纸面交易能力集合。
    pub fn paper_trading() -> Self {
        Self::from_caps(&[
            MarketCapability::ReadMarket,
            MarketCapability::ReadOrderBook,
            MarketCapability::ReadTrades,
            MarketCapability::PaperTrading,
            MarketCapability::CancelOrder,
            MarketCapability::ReplaceOrder,
            MarketCapability::Balance,
            MarketCapability::Rest,
            MarketCapability::Spot,
        ])
    }
}

impl fmt::Display for CapabilitySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render_table("能力集合"))
    }
}

impl PartialEq for CapabilitySet {
    fn eq(&self, other: &Self) -> bool {
        self.capabilities == other.capabilities
    }
}

impl Eq for CapabilitySet {}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ===== MarketCapability 测试 =====

    #[test]
    fn capability_zh_names() {
        assert_eq!(MarketCapability::ReadMarket.as_zh(), "读取市场");
        assert_eq!(MarketCapability::LiveTrading.as_zh(), "真实交易");
        assert_eq!(MarketCapability::Spot.as_zh(), "现货");
        assert_eq!(MarketCapability::Prediction.as_zh(), "预测市场");
        assert_eq!(MarketCapability::WebSocket.as_zh(), "WebSocket");
    }

    #[test]
    fn capability_categories() {
        assert_eq!(MarketCapability::ReadMarket.category_zh(), "数据能力");
        assert_eq!(MarketCapability::LiveTrading.category_zh(), "交易能力");
        assert_eq!(MarketCapability::Wallet.category_zh(), "账户能力");
        assert_eq!(MarketCapability::Rest.category_zh(), "传输能力");
        assert_eq!(MarketCapability::Prediction.category_zh(), "市场类型");
        assert_eq!(MarketCapability::MultiAsset.category_zh(), "扩展能力");
    }

    #[test]
    fn capability_is_trading() {
        assert!(MarketCapability::LiveTrading.is_trading());
        assert!(MarketCapability::PaperTrading.is_trading());
        assert!(MarketCapability::CancelOrder.is_trading());
        assert!(!MarketCapability::ReadMarket.is_trading());
        assert!(!MarketCapability::Wallet.is_trading());
    }

    #[test]
    fn capability_is_data() {
        assert!(MarketCapability::ReadMarket.is_data());
        assert!(MarketCapability::HistoricalData.is_data());
        assert!(!MarketCapability::LiveTrading.is_data());
    }

    // ===== CapabilitySet 测试 =====

    #[test]
    fn empty_set() {
        let set = CapabilitySet::new();
        assert!(set.is_empty());
        assert_eq!(set.count(), 0);
    }

    #[test]
    fn add_and_has() {
        let mut set = CapabilitySet::new();
        set.add(MarketCapability::ReadMarket);
        assert!(set.has(&MarketCapability::ReadMarket));
        assert!(!set.has(&MarketCapability::LiveTrading));
        assert_eq!(set.count(), 1);
    }

    #[test]
    fn has_all_and_has_any() {
        let set = CapabilitySet::from_caps(&[
            MarketCapability::ReadMarket,
            MarketCapability::ReadOrderBook,
            MarketCapability::Spot,
        ]);

        assert!(set.has_all(&[MarketCapability::ReadMarket, MarketCapability::Spot]));
        assert!(!set.has_all(&[MarketCapability::ReadMarket, MarketCapability::LiveTrading,]));
        assert!(set.has_any(&[MarketCapability::LiveTrading, MarketCapability::Spot]));
        assert!(!set.has_any(&[MarketCapability::LiveTrading, MarketCapability::Margin]));
    }

    #[test]
    fn set_operations() {
        let a = CapabilitySet::from_caps(&[
            MarketCapability::ReadMarket,
            MarketCapability::Spot,
            MarketCapability::Rest,
        ]);
        let b = CapabilitySet::from_caps(&[
            MarketCapability::ReadMarket,
            MarketCapability::Prediction,
            MarketCapability::WebSocket,
        ]);

        // 并集
        let union = a.union(&b);
        assert_eq!(union.count(), 5);
        assert!(union.has(&MarketCapability::Spot));
        assert!(union.has(&MarketCapability::Prediction));

        // 交集
        let intersection = a.intersection(&b);
        assert_eq!(intersection.count(), 1);
        assert!(intersection.has(&MarketCapability::ReadMarket));

        // 差集
        let diff = a.difference(&b);
        assert_eq!(diff.count(), 2);
        assert!(diff.has(&MarketCapability::Spot));
        assert!(!diff.has(&MarketCapability::ReadMarket));
    }

    #[test]
    fn preset_sets() {
        let pm = CapabilitySet::prediction_market_full();
        assert!(pm.has(&MarketCapability::Prediction));
        assert!(pm.has(&MarketCapability::LiveTrading));
        assert!(pm.has(&MarketCapability::Wallet));
        assert!(!pm.has(&MarketCapability::Spot));

        let spot = CapabilitySet::spot_exchange_full();
        assert!(spot.has(&MarketCapability::Spot));
        assert!(spot.has(&MarketCapability::Margin));
        assert!(spot.has(&MarketCapability::Perpetual));
        assert!(!spot.has(&MarketCapability::Prediction));
    }

    #[test]
    fn render_table_contains_zh() {
        let set = CapabilitySet::from_caps(&[MarketCapability::ReadMarket, MarketCapability::Spot]);
        let table = set.render_table("测试");
        assert!(table.contains("读取市场"));
        assert!(table.contains("现货"));
        assert!(table.contains("测试"));
    }

    #[test]
    fn list_by_category() {
        let set = CapabilitySet::from_caps(&[
            MarketCapability::ReadMarket,
            MarketCapability::ReadOrderBook,
            MarketCapability::LiveTrading,
        ]);
        let data_caps = set.list_by_category("数据能力");
        assert_eq!(data_caps.len(), 2);
        let trading_caps = set.list_by_category("交易能力");
        assert_eq!(trading_caps.len(), 1);
    }

    #[test]
    fn categories_sorted() {
        let set = CapabilitySet::from_caps(&[
            MarketCapability::ReadMarket,
            MarketCapability::LiveTrading,
        ]);
        let cats = set.categories();
        assert!(cats.len() >= 1);
        // 应该按字母排序
        for i in 1..cats.len() {
            assert!(cats[i - 1] <= cats[i]);
        }
    }
}
