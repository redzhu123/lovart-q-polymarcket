//! 市场元数据（P3.0 第六节）。
//!
//! 统一所有市场的元数据定义。
//! 未来所有市场统一从此模块读取元数据。
//!
//! # 核心类型
//!
//! - [`MarketId`]：全局唯一市场标识符
//! - [`MarketMetadata`]：市场的完整元数据
//! - [`MarketType`]：市场类型枚举
//! - [`FeeModel`]：费率模型

use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// MarketId
// ============================================================================

/// 全局唯一市场标识符。
///
/// 格式：`{exchange}:{market_type}:{base}:{quote}`。
/// 例如：`polymarket:prediction:usdc:usdc`、`binance:spot:btc:usdt`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketId {
    /// 交易所代码（小写）。
    pub exchange: String,
    /// 市场类型。
    pub market_type: MarketType,
    /// 基础资产符号。
    pub base_asset: String,
    /// 报价资产符号。
    pub quote_asset: String,
}

impl MarketId {
    /// 创建新的市场 ID。
    pub fn new(
        exchange: impl Into<String>,
        market_type: MarketType,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
    ) -> Self {
        Self {
            exchange: exchange.into().to_lowercase(),
            market_type,
            base_asset: base_asset.into().to_uppercase(),
            quote_asset: quote_asset.into().to_uppercase(),
        }
    }

    /// 规范化的字符串表示。
    pub fn to_canonical(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.exchange,
            self.market_type.as_code(),
            self.base_asset,
            self.quote_asset
        )
    }

    /// 从规范字符串解析。
    pub fn from_canonical(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 4 {
            return None;
        }
        Some(Self {
            exchange: parts[0].to_string(),
            market_type: MarketType::from_code(parts[1])?,
            base_asset: parts[2].to_string(),
            quote_asset: parts[3].to_string(),
        })
    }
}

impl fmt::Display for MarketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_canonical())
    }
}

// ============================================================================
// MarketType
// ============================================================================

/// 市场类型枚举。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketType {
    /// 现货。
    #[serde(rename = "spot")]
    Spot,
    /// 保证金。
    #[serde(rename = "margin")]
    Margin,
    /// 永续合约。
    #[serde(rename = "perp")]
    Perpetual,
    /// 交割合约。
    #[serde(rename = "futures")]
    Futures,
    /// 期权。
    #[serde(rename = "options")]
    Options,
    /// 预测市场。
    #[serde(rename = "prediction")]
    Prediction,
    /// 其他（自定义）。
    #[serde(untagged)]
    Other(String),
}

impl MarketType {
    /// 类型代码（用于序列化）。
    pub fn as_code(&self) -> &str {
        match self {
            MarketType::Spot => "spot",
            MarketType::Margin => "margin",
            MarketType::Perpetual => "perp",
            MarketType::Futures => "futures",
            MarketType::Options => "options",
            MarketType::Prediction => "prediction",
            MarketType::Other(s) => s.as_str(),
        }
    }

    /// 中文名称。
    pub fn as_zh(&self) -> &str {
        match self {
            MarketType::Spot => "现货",
            MarketType::Margin => "保证金",
            MarketType::Perpetual => "永续合约",
            MarketType::Futures => "交割合约",
            MarketType::Options => "期权",
            MarketType::Prediction => "预测市场",
            MarketType::Other(s) => s.as_str(),
        }
    }

    /// 从代码解析。
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "spot" => Some(MarketType::Spot),
            "margin" => Some(MarketType::Margin),
            "perp" => Some(MarketType::Perpetual),
            "futures" => Some(MarketType::Futures),
            "options" => Some(MarketType::Options),
            "prediction" => Some(MarketType::Prediction),
            other => Some(MarketType::Other(other.to_string())),
        }
    }

    /// 是否为合约类型（需要保证金）。
    pub fn is_contract(&self) -> bool {
        matches!(
            self,
            MarketType::Margin | MarketType::Perpetual | MarketType::Futures | MarketType::Options
        )
    }

    /// 是否为现货/预测市场（无需保证金）。
    pub fn is_spot_like(&self) -> bool {
        matches!(self, MarketType::Spot | MarketType::Prediction)
    }
}

impl fmt::Display for MarketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_zh())
    }
}

// ============================================================================
// FeeModel
// ============================================================================

/// 费率模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeModel {
    /// Maker 费率（bps，基点）。
    pub maker_fee_bps: f64,
    /// Taker 费率（bps，基点）。
    pub taker_fee_bps: f64,
    /// 是否有 VIP 折扣层级。
    pub has_tiered_discount: bool,
    /// 费用货币。
    pub fee_currency: String,
    /// 额外说明。
    pub notes: String,
}

impl FeeModel {
    /// 零费率模型。
    pub fn zero() -> Self {
        Self {
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            has_tiered_discount: false,
            fee_currency: "USDC".to_string(),
            notes: "零费率".to_string(),
        }
    }

    /// Polymarket 费率模型。
    pub fn polymarket() -> Self {
        Self {
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            has_tiered_discount: false,
            fee_currency: "USDC".to_string(),
            notes: "Polymarket 0 费率（仅 Gas）".to_string(),
        }
    }

    /// 标准 CEX 费率模型。
    pub fn standard_cex() -> Self {
        Self {
            maker_fee_bps: 2.0,
            taker_fee_bps: 5.0,
            has_tiered_discount: true,
            fee_currency: "本地代币".to_string(),
            notes: "标准 CEX 费率".to_string(),
        }
    }

    /// 中文描述。
    pub fn summary_zh(&self) -> String {
        format!(
            "Maker: {}bps / Taker: {}bps ({}{})",
            self.maker_fee_bps,
            self.taker_fee_bps,
            self.fee_currency,
            if self.has_tiered_discount {
                "，支持 VIP 折扣"
            } else {
                ""
            }
        )
    }
}

impl Default for FeeModel {
    fn default() -> Self {
        Self::zero()
    }
}

// ============================================================================
// MarketMetadata
// ============================================================================

/// 市场完整元数据（P3.0 第六节）。
///
/// 所有市场必须提供此元数据。
/// 系统根据元数据自动配置交易参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketMetadata {
    /// 全局唯一市场 ID。
    pub market_id: MarketId,

    /// 交易所名称（如 "Polymarket", "Binance"）。
    pub exchange: String,

    /// 市场类型。
    pub market_type: MarketType,

    /// 基础资产。
    pub base_asset: String,

    /// 报价资产。
    pub quote_asset: String,

    /// 结算货币。
    pub settlement_currency: String,

    /// 交易时间（如 "24/7"），None 表示不适用。
    pub trading_hours: Option<String>,

    /// 交易所时区（IANA 时区标识）。
    pub timezone: String,

    /// 费率模型。
    pub fee_model: FeeModel,

    /// 最小报价单位（Tick Size）。
    pub tick_size: f64,

    /// 最小交易单位（Lot Size）。
    pub lot_size: f64,

    /// 最小名义金额（以报价资产计）。
    pub min_notional: f64,

    /// 最大名义金额（0 表示无限制）。
    pub max_notional: f64,

    /// 价格精度（小数位数）。
    pub price_precision: u32,

    /// 数量精度（小数位数）。
    pub quantity_precision: u32,

    /// 是否支持保证金/杠杆。
    pub supports_margin: bool,

    /// 最大杠杆倍数（1.0 表示无杠杆）。
    pub max_leverage: f64,

    /// 市场官方网站。
    pub website: Option<String>,

    /// API 文档 URL。
    pub api_docs_url: Option<String>,

    /// 额外标签（如 "defi", "cex", "prediction"）。
    pub tags: Vec<String>,

    /// 自由格式备注。
    pub notes: String,
}

impl MarketMetadata {
    /// 创建预测市场元数据（Polymarket 风格）。
    pub fn prediction_market(exchange: impl Into<String>, base_asset: impl Into<String>) -> Self {
        let exchange_str = exchange.into();
        let base_str = base_asset.into();
        Self {
            market_id: MarketId::new(
                exchange_str.clone(),
                MarketType::Prediction,
                base_str.clone(),
                "USDC",
            ),
            exchange: exchange_str,
            market_type: MarketType::Prediction,
            base_asset: base_str,
            quote_asset: "USDC".to_string(),
            settlement_currency: "USDC".to_string(),
            trading_hours: Some("24/7".to_string()),
            timezone: "UTC".to_string(),
            fee_model: FeeModel::polymarket(),
            tick_size: 0.0001,
            lot_size: 1.0,
            min_notional: 1.0,
            max_notional: 0.0,
            price_precision: 4,
            quantity_precision: 2,
            supports_margin: false,
            max_leverage: 1.0,
            website: Some("https://polymarket.com".to_string()),
            api_docs_url: Some("https://docs.polymarket.com".to_string()),
            tags: vec!["prediction".to_string(), "defi".to_string()],
            notes: "预测市场 — YES/NO 二元结果".to_string(),
        }
    }

    /// 创建现货市场元数据（Binance 风格）。
    pub fn spot_market(
        exchange: impl Into<String>,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
    ) -> Self {
        let exchange_str = exchange.into();
        let base = base_asset.into();
        let quote = quote_asset.into();
        Self {
            market_id: MarketId::new(
                exchange_str.clone(),
                MarketType::Spot,
                base.clone(),
                quote.clone(),
            ),
            exchange: exchange_str,
            market_type: MarketType::Spot,
            base_asset: base,
            quote_asset: quote.clone(),
            settlement_currency: quote,
            trading_hours: Some("24/7".to_string()),
            timezone: "UTC".to_string(),
            fee_model: FeeModel::standard_cex(),
            tick_size: 0.01,
            lot_size: 0.001,
            min_notional: 10.0,
            max_notional: 0.0,
            price_precision: 2,
            quantity_precision: 3,
            supports_margin: false,
            max_leverage: 1.0,
            website: None,
            api_docs_url: None,
            tags: vec!["spot".to_string(), "cex".to_string()],
            notes: "标准现货市场".to_string(),
        }
    }

    /// 渲染为中文摘要。
    pub fn summary_zh(&self) -> String {
        let mut lines = vec![
            format!("【{} 市场元数据】", self.exchange),
            format!("  ID: {}", self.market_id),
            format!("  类型: {}", self.market_type.as_zh()),
            format!("  交易对: {}/{}", self.base_asset, self.quote_asset),
            format!("  结算货币: {}", self.settlement_currency),
        ];

        if let Some(ref hours) = self.trading_hours {
            lines.push(format!("  交易时间: {}", hours));
        }
        lines.push(format!("  时区: {}", self.timezone));
        lines.push(format!("  {}", self.fee_model.summary_zh()));
        lines.push(format!(
            "  Tick: {} / Lot: {}",
            self.tick_size, self.lot_size
        ));
        lines.push(format!(
            "  最小/最大名义: {} / {}",
            self.min_notional,
            if self.max_notional > 0.0 {
                self.max_notional.to_string()
            } else {
                "无限制".to_string()
            }
        ));
        lines.push(format!(
            "  精度: 价格{}位 / 数量{}位",
            self.price_precision, self.quantity_precision
        ));

        if self.supports_margin {
            lines.push(format!("  杠杆: 最大 {}x", self.max_leverage));
        }

        if !self.tags.is_empty() {
            lines.push(format!("  标签: {}", self.tags.join(", ")));
        }

        lines.join("\n")
    }

    /// 完整中文报告。
    pub fn report_zh(&self) -> String {
        let mut lines = vec![
            "══════ 市场元数据报告 ══════".to_string(),
            self.summary_zh(),
            String::new(),
        ];

        if let Some(ref website) = self.website {
            lines.push(format!("  官网: {}", website));
        }
        if let Some(ref api_docs) = self.api_docs_url {
            lines.push(format!("  API 文档: {}", api_docs));
        }
        if !self.notes.is_empty() {
            lines.push(format!("  备注: {}", self.notes));
        }

        lines.push("════════════════════════════".to_string());
        lines.join("\n")
    }
}

impl Default for MarketMetadata {
    fn default() -> Self {
        Self::prediction_market("Unknown", "ASSET")
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_id_canonical() {
        let id = MarketId::new("Binance", MarketType::Spot, "BTC", "USDT");
        assert_eq!(id.to_canonical(), "binance:spot:BTC:USDT");
    }

    #[test]
    fn market_id_parse_roundtrip() {
        let canonical = "polymarket:prediction:USDC:USDC";
        let id = MarketId::from_canonical(canonical).unwrap();
        assert_eq!(id.to_canonical(), canonical);
    }

    #[test]
    fn market_type_is_contract() {
        assert!(!MarketType::Spot.is_contract());
        assert!(!MarketType::Prediction.is_contract());
        assert!(MarketType::Perpetual.is_contract());
        assert!(MarketType::Futures.is_contract());
    }

    #[test]
    fn fee_model_zero() {
        let fee = FeeModel::zero();
        assert_eq!(fee.maker_fee_bps, 0.0);
        assert_eq!(fee.taker_fee_bps, 0.0);
    }

    #[test]
    fn prediction_metadata() {
        let meta = MarketMetadata::prediction_market("Polymarket", "TEST");
        assert_eq!(meta.market_type, MarketType::Prediction);
        assert_eq!(meta.settlement_currency, "USDC");
        assert!(meta.summary_zh().contains("Polymarket"));
        assert!(meta.summary_zh().contains("预测市场"));
    }

    #[test]
    fn spot_metadata() {
        let meta = MarketMetadata::spot_market("Binance", "BTC", "USDT");
        assert_eq!(meta.market_type, MarketType::Spot);
        assert_eq!(meta.base_asset, "BTC");
        assert_eq!(meta.quote_asset, "USDT");
    }
}
