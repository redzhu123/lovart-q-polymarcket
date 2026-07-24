# Workflow 状态机

> P2-02 | 更新: 2026-07-23

## 1. 状态枚举

| 状态 | 中文名 | 说明 |
|------|--------|------|
| Idle | 空闲 | 初始 |
| LoadingMarket | 加载市场 | GET /markets |
| LoadingOrderBook | 加载订单簿 | GET /book?token_id= |
| CheckingBalance | 检查余额 | GET /balances |
| BuildingOrder | 构建订单 | 本地构建 CLOB V2 订单 JSON |
| SubmittingOrder | 提交订单(DryRun) | 校验参数，不发送 |
| WaitingResult | 等待结果 | DryRun 模拟接受/成交 |
| SyncOrder | 同步订单 | GET /orders（查询订单状态） |
| SyncTrade | 同步成交 | GET /trades |
| SyncPosition | 同步持仓 | GET /positions |
| SyncBalance | 同步余额 | GET /balances |
| Completed | 已完成 | 终态 |
| Failed | 已失败 | 终态 |

## 2. 状态图

```
Idle
  ↓
LoadingMarket
  ↓
LoadingOrderBook
  ↓
CheckingBalance
  ↓ (完整路径)              ↓ (只读路径)
BuildingOrder             SyncPosition
  ↓                          ↓
SubmittingOrder(DryRun)    SyncBalance
  ↓                          ↓
WaitingResult             Completed
  ↓
SyncOrder
  ↓
SyncTrade
  ↓
SyncPosition
  ↓
SyncBalance
  ↓
Completed

任意步骤失败 → Failed（终态）
```

## 3. 合法转换表

| from | 允许的 to |
|------|-----------|
| Idle | LoadingMarket |
| LoadingMarket | LoadingOrderBook |
| LoadingOrderBook | CheckingBalance |
| CheckingBalance | BuildingOrder / SyncPosition / Completed |
| BuildingOrder | SubmittingOrder |
| SubmittingOrder | WaitingResult |
| WaitingResult | SyncOrder |
| SyncOrder | SyncTrade |
| SyncTrade | SyncPosition |
| SyncPosition | SyncBalance |
| SyncBalance | Completed |
| 任意非终态 | Failed |
| Completed / Failed | （终态，不可转换） |

非法转换会被记录并强制进入 Failed。所有转换输出中文日志并记入历史。

## 4. 两条路径

- **完整路径**（DryRun / Replay）：CheckingBalance → BuildingOrder → … → SyncBalance → Completed。
- **只读路径**（LiveReadOnly）：CheckingBalance → SyncPosition → SyncBalance → Completed，跳过下单三步。
