# PMS 投资组合管理系统 — 架构文档

## 概述

PMS（Portfolio Management System）是企业级投资组合管理系统，是系统唯一的资金/持仓/盈亏/估值/风险敞口管理中心。

## 架构位置

```
Strategy → Risk → Execution → OMS → Gateway → Exchange
                                            ↓
                                      Trade Event
                                            ↓
                                          PMS
                                            ↓
                                  Portfolio / PnL / Exposure
```

## 核心职责

1. **资金管理** — 总资产/可用资金/冻结资金
2. **持仓管理** — 统一持仓模型（Prediction/Spot/Perpetual/AMM）
3. **盈亏计算** — 已实现/未实现/当日盈亏/胜率/盈亏比
4. **估值引擎** — NAV/市值/投资组合价值
5. **风险敞口** — 多空/资产类型/单市场敞口
6. **事件驱动** — 监听 OMS EventBus 自动更新

## 模块结构

| 模块 | 文件 | 职责 |
|------|------|------|
| domain | `domain.rs` | 统一领域对象 |
| portfolio | `portfolio.rs` | PortfolioManager |
| position | `position.rs` | PositionManager |
| account | `account.rs` | AccountManager |
| pnl | `pnl.rs` | PnLEngine |
| valuation | `valuation.rs` | ValuationEngine |
| exposure | `exposure.rs` | ExposureEngine |
| events | `events.rs` | PmsEventSubscriber |
| repository | `repository/` | 持久化仓库 |
| metrics | `metrics.rs` | 指标计算器 |

## 禁止事项

- 禁止真实交易/真实资金/Wallet/签名
- 禁止修改 OMS / Gateway / Execution
- PMS 仅负责资产管理

## 设计原则

- 事件驱动（不主动调用 OMS）
- Repository 模式（Memory/CSV/SQLite 预留）
- 中文 tracing 日志
- 所有市场统一 Domain 类型
