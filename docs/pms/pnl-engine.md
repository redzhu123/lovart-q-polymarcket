# PnL Engine — 盈亏引擎

## 功能

统一计算所有盈亏指标，是系统唯一的盈亏计算来源。

## 计算指标

| 指标 | 说明 |
|------|------|
| realized_pnl | 已实现盈亏（已平仓持仓之和） |
| unrealized_pnl | 未实现盈亏（活跃持仓按标记价计算） |
| daily_pnl | 当日盈亏（当前总盈亏 - 当日基准） |
| total_pnl | 累计总盈亏 |
| roi | 收益率 = total_pnl / initial_capital |
| win_rate | 胜率 = winning_trades / total_trades |
| avg_profit | 平均盈利（盈利交易均值） |
| avg_loss | 平均亏损（亏损交易均值） |
| profit_factor | 盈亏比 = avg_profit / avg_loss |

## 使用

```rust
let engine = PnLEngine::new(10000.0);
let report = engine.calculate(positions, portfolio);
engine.print_zh(&report);
```

## 当日盈亏

跨日时需调用 `reset_day(current_total_pnl)` 重置基准。
