# Capability System（P3.0 第三节）

## 概述

每个市场通过 `CapabilitySet` 声明自己的能力。系统根据能力自动启用功能，**禁止写死布尔值**。

## MarketCapability 枚举

### 数据能力
- `ReadMarket` — 读取市场列表
- `ReadOrderBook` — 读取订单簿
- `ReadTrades` — 读取成交记录
- `HistoricalData` — 历史数据（K线、历史订单簿等）

### 交易能力
- `PaperTrading` — 纸面交易（模拟交易）
- `LiveTrading` — 真实交易（需钱包/签名/API Key）
- `CancelOrder` — 取消订单
- `ReplaceOrder` — 替换订单
- `BatchOrders` — 批量下单

### 账户能力
- `Wallet` — 钱包操作
- `Balance` — 余额查询
- `Settlement` — 结算

### 传输能力
- `Rest` — REST API
- `WebSocket` — WebSocket 实时推送
- `Streaming` — 流式数据（gRPC stream / SSE）
- `FIX` — FIX 协议

### 市场类型
- `Spot` — 现货
- `Margin` — 保证金（接口预留）
- `Perpetual` — 永续合约（接口预留）
- `Futures` — 交割合约
- `Options` — 期权
- `Prediction` — 预测市场

### 扩展能力
- `MultiAsset` — 多资产支持
- `MultiChain` — 多链支持
- `CrossMargin` — 全仓保证金
- `IsolatedMargin` — 逐仓保证金

### 高级能力
- `Leverage` — 杠杆交易
- `Staking` — 质押/借贷
- `Launchpad` — Launchpad / IEO

## CapabilitySet

```rust
let mut caps = CapabilitySet::new();
caps.add(MarketCapability::Spot);
caps.add(MarketCapability::LiveTrading);

// 检查能力
assert!(caps.has(&MarketCapability::Spot));

// 批量检查
assert!(caps.has_all(&[MarketCapability::Spot, MarketCapability::LiveTrading]));

// 集合运算
let a = CapabilitySet::from_caps(&[MarketCapability::Spot, MarketCapability::Rest]);
let b = CapabilitySet::from_caps(&[MarketCapability::Spot, MarketCapability::WebSocket]);
let common = a.intersection(&b);  // {Spot}
let all = a.union(&b);           // {Spot, Rest, WebSocket}
```

## 预定义模板

### 预测市场（Polymarket 风格）
```
数据: ReadMarket / ReadOrderBook / ReadTrades / HistoricalData
交易: PaperTrading / LiveTrading / CancelOrder / ReplaceOrder
账户: Wallet / Balance / Settlement
传输: Rest / WebSocket
市场: Prediction
扩展: MultiChain
```

### 现货交易所（Binance 风格）
```
数据: ReadMarket / ReadOrderBook / ReadTrades / HistoricalData
交易: PaperTrading / LiveTrading / CancelOrder / ReplaceOrder / BatchOrders
账户: Wallet / Balance
传输: Rest / WebSocket / Streaming
市场: Spot / Margin / Perpetual
扩展: MultiAsset / CrossMargin / IsolatedMargin
高级: Leverage / Staking / Launchpad
```

## 能力矩阵渲染

```rust
let caps = CapabilitySet::prediction_market_full();
println!("{}", caps.render_table("Polymarket"));
```

输出：
```
【Polymarket】
  数据能力:
    ✅ 读取市场
    ✅ 读取订单簿
    ✅ 读取成交
    ✅ 历史数据
  交易能力:
    ✅ 纸面交易
    ✅ 真实交易
    ✅ 取消订单
    ✅ 替换订单
  ...
```
