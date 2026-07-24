# Position Settlement（持仓结算）

## 概述

所有成交驱动的持仓变化均由 Position Manager 统一处理。

## 持仓模型

```rust
pub struct PositionState {
    pub position_id: String,       // SPOS-YYYYMMDD-NNNNNN
    pub market_id: String,
    pub direction: Direction,      // YES/NO
    pub side: Side,                // Buy/Sell
    pub quantity: f64,             // 持仓数量
    pub average_price: f64,        // 开仓均价
    pub cost_basis: f64,           // 开仓成本
    pub mark_price: f64,           // 标记价
    pub market_value: f64,         // 市值
    pub unrealized_pnl: f64,       // 未实现盈亏
    pub realized_pnl: f64,         // 已实现盈亏
    pub is_closed: bool,
}
```

## 操作

| 操作 | 说明 |
|------|------|
| `open()` | 新开仓 |
| `add_fill()` | 加仓（均价调整） |
| `reduce()` | 减仓（返回已实现盈亏） |
| `mark()` | 标记价格（mark-to-market） |

## 关键规则

- Buy = 开多仓
- Sell = 平仓/减仓
- 同一市场 + 同一方向 = 同一持仓
- 不同方向 = 不同持仓
