# pm-portfolio

组合资金管理（Simulation Only）。

## 职责
- `Position` / `PositionStatus`：模拟持仓（开仓 / mark / 平仓 / cost_basis / market_value）。
- `Order` / `OrderStatus`：paper 简单订单（Pending->Filled/Cancelled，立即成交）。
- `Portfolio`：cash / available_cash / locked_cash / total_value / total_pnl / ROI；开仓扣款 / 平仓入账 / 重估。
- `RiskManager` + `RiskPolicy`：最大持仓、单笔上限、最大日亏、待处理订单上限、现金检查（统一风控闸）。

## 依赖
`pm-core`（`Side`）, `chrono`。不依赖 `pm-models`（`RiskPolicy` 自带，由 driver 从 `Config` 注入），保持低耦合。

## 用途
- 被 `pm-paper` 直接复用（PaperTradingEngine 内持 `Portfolio`）。
- 未来 live execution 可复用同一组合/风控抽象。
- `pm-strategy` 通过 `pm-paper` 间接使用。

## 设计约束
- Simulation Only：所有资金为模拟 USDC。
- 不变量：`INITIAL_CAPITAL = cash + locked_cash - realized_pnl`。
- 禁止 `unwrap/expect/panic`；风控拒绝返回 `Result` / 枚举原因。
