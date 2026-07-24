# PMS 统一持仓模型

## Position 结构

```rust
pub struct Position {
    pub position_id: String,     // POS-YYYYMMDD-NNNNNN
    pub market_id: String,       // 市场 ID
    pub asset_type: AssetType,   // Prediction/Spot/Perpetual/AMM
    pub direction: Direction,    // YES/NO
    pub side: Side,              // Buy/Sell
    pub quantity: f64,           // 持仓数量
    pub average_price: f64,      // 开仓均价
    pub current_price: f64,      // 当前标记价
    pub market_value: f64,       // 持仓市值
    pub cost_basis: f64,         // 开仓成本
    pub unrealized_pnl: f64,     // 未实现盈亏
    pub realized_pnl: f64,       // 已实现盈亏
    pub roi: f64,                // 收益率
    pub status: PositionStatus,  // Open/Closed
    pub order_ids: Vec<String>,  // 关联订单 ID
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
    pub closed_at: Option<DateTime<Local>>,
}
```

## 支持的市场类型

- **Prediction** — 预测市场（Polymarket）
- **Spot** — 现货（BTC/ETH）
- **Perpetual** — 永续合约
- **AMM** — AMM 流动性

未来新增市场类型无需修改接口。

## 生命周期

1. `Position::open()` — 开仓
2. `Position::mark()` — Mark-to-Market
3. `Position::add_quantity()` — 加仓（均价调整）
4. `Position::reduce()` — 部分平仓
5. `Position::close()` — 完全平仓

## 关键计算

- `market_value = quantity × current_price`
- `cost_basis = quantity × average_price`
- `unrealized_pnl = quantity × (current_price - average_price)`
- `realized_pnl = quantity × (exit_price - average_price)`
- `roi = pnl / cost_basis`
