# Market Metadata（P3.0 第六节）

## 概述

统一所有市场的元数据定义。每个市场必须提供 `MarketMetadata`，系统根据元数据自动配置交易参数。

## MarketId

全局唯一市场标识符，格式：`{exchange}:{market_type}:{base}:{quote}`。

```rust
let id = MarketId::new("Binance", MarketType::Spot, "BTC", "USDT");
assert_eq!(id.to_canonical(), "binance:spot:BTC:USDT");

// 解析
let parsed = MarketId::from_canonical("polymarket:prediction:USDC:USDC").unwrap();
```

## MarketType

| 类型 | 代码 | 说明 |
|------|------|------|
| `Spot` | spot | 现货市场 |
| `Margin` | margin | 保证金市场 |
| `Perpetual` | perp | 永续合约 |
| `Futures` | futures | 交割合约 |
| `Options` | options | 期权 |
| `Prediction` | prediction | 预测市场 |

## FeeModel

```rust
pub struct FeeModel {
    pub maker_fee_bps: f64,    // Maker 费率（bps）
    pub taker_fee_bps: f64,    // Taker 费率（bps）
    pub has_tiered_discount: bool,  // VIP 折扣层级
    pub fee_currency: String,  // 费用货币
    pub notes: String,         // 说明
}
```

预定义模型：
- `FeeModel::zero()` — 零费率
- `FeeModel::polymarket()` — Polymarket（0 费率，仅 Gas）
- `FeeModel::standard_cex()` — 标准 CEX（Maker 2bps / Taker 5bps）

## MarketMetadata

完整元数据字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `market_id` | MarketId | 全局唯一 ID |
| `exchange` | String | 交易所名称 |
| `market_type` | MarketType | 市场类型 |
| `base_asset` | String | 基础资产 |
| `quote_asset` | String | 报价资产 |
| `settlement_currency` | String | 结算货币 |
| `trading_hours` | Option\<String\> | 交易时间 |
| `timezone` | String | 时区 |
| `fee_model` | FeeModel | 费率模型 |
| `tick_size` | f64 | 最小报价单位 |
| `lot_size` | f64 | 最小交易单位 |
| `min_notional` | f64 | 最小名义金额 |
| `max_notional` | f64 | 最大名义金额 |
| `price_precision` | u32 | 价格精度 |
| `quantity_precision` | u32 | 数量精度 |
| `supports_margin` | bool | 是否支持保证金 |
| `max_leverage` | f64 | 最大杠杆 |
| `website` | Option\<String\> | 官网 |
| `api_docs_url` | Option\<String\> | API 文档 |
| `tags` | Vec\<String\> | 标签 |
| `notes` | String | 备注 |

## 工厂函数

```rust
// 预测市场
let meta = MarketMetadata::prediction_market("Polymarket", "USDC");

// 现货市场
let meta = MarketMetadata::spot_market("Binance", "BTC", "USDT");
```
