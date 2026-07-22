# pm-shadow

影子交易系统（Simulation Only）。

## 职责
- `ShadowTrade` / `TradeStatus`：模拟交易（开仓 entry_sum、平仓估算 PnL/ROI）。
- `ShadowStats`：累计统计（总数 / 胜负 / 均值 / 最佳 / 最差 ROI / 均时长）。
- `ShadowEngine`：维护未平仓交易，机会出现开仓、结束平仓。
- `ShadowTradeRecord` + CSV：复用 `pm-storage` 原语写 `shadow_trades.csv`。
- `load_history`：启动时从 CSV 重建统计与 trade_id 基线。

## 依赖
`pm-core`, `pm-models`（`FinishedOpportunity`）, `pm-storage`, `serde`, `anyhow`, `chrono`。
不依赖 `pm-tracker`（DTO 已下沉到 models）。

## 用途
- 被 `pm-scanner::driver`（实盘扫描）与 `pm-backtest`（历史回测）复用同一 Shadow 策略。
- 与 `pm-paper` 并存：shadow 估算单机会理论套利，paper 在组合层做资金管理。

## 设计约束
- Simulation Only：所有收益为模拟估算，代码显式标注，不伪装真实收益。
- 禁止 `unwrap/expect/panic`；CSV 写失败只返回 0。
