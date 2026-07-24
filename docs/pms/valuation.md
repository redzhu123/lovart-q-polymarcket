# Valuation Engine — 估值引擎

## 功能

统一估值，提供投资组合的标准化估值指标。

## 估值指标

| 指标 | 说明 |
|------|------|
| position_value | 持仓总价值（活跃持仓市值之和） |
| cash_value | 现金价值（可用资金 + 冻结资金） |
| portfolio_value | 投资组合总价值（现金 + 持仓） |
| total_exposure | 总敞口（= 持仓价值） |
| nav | 净资产价值（= portfolio_value） |
| market_value | 总市值 |

## 支持未来

- 接入实时行情刷新估值
- 多货币换算
- 历史估值快照
