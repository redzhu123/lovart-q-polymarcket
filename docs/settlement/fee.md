# Fee Engine（手续费引擎）

## 概述

统一手续费模型，所有手续费由 Fee Engine 统一计算。

## 手续费类型

| 类型 | 说明 | 默认费率 |
|------|------|---------|
| Maker Fee | 挂单手续费 | 0.02% |
| Taker Fee | 吃单手续费 | 0.05% |
| Trading Fee | 交易手续费 | 0.01% |
| Settlement Fee | 结算手续费（预留） | 0.00% |

## 费率规则

```rust
pub struct FeeRule {
    pub name: String,
    pub maker_rate: f64,      // 小数形式，如 0.0002 = 0.02%
    pub taker_rate: f64,
    pub trading_rate: f64,
    pub settlement_rate: f64,  // 预留
    pub min_fee: f64,          // 最低手续费
    pub max_fee: f64,          // 最高手续费（0=无上限）
}
```

## 预置规则

- **Standard** — 标准费率（maker=0.02%, taker=0.05%, trading=0.01%）
- **ZeroFee** — 零费率（模拟环境使用）
