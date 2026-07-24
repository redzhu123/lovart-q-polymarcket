# API 调用顺序

> P2-02 | 更新: 2026-07-23

## 1. 完整生命周期（DryRun / Replay）

| # | 步骤 | API | 方法 | 说明 |
|---|------|-----|------|------|
| 1 | 加载市场 | /markets | GET | 拉取市场列表 |
| 2 | 加载订单簿 | /book?token_id= | GET | 拉取目标 token 订单簿 |
| 3 | 检查余额 | /balances | GET | 查询账户余额 |
| 4 | 构建订单 | —（本地） | POST(DryRun) | 构造 CLOB V2 订单 JSON，不发送 |
| 5 | 提交订单(DryRun) | —（本地） | — | 校验参数，不发送 |
| 6 | 等待结果 | —（本地） | — | DryRun 模拟接受/成交 |
| 7 | 同步订单 | /orders | GET | 查询订单状态 |
| 8 | 同步成交 | /trades | GET | 查询成交记录 |
| 9 | 同步持仓 | /positions | GET | 查询持仓 |
| 10 | 同步余额 | /balances | GET | 同步余额 |
| 11 | 完成 | — | — | 生命周期完成 |

### 校验顺序约束

- 提交订单后必须查询订单状态（SyncOrder 在 SubmittingOrder 之后）。
- 订单成交后必须同步持仓（SyncPosition 在 SyncOrder 之后）。
- 持仓更新后必须同步余额（SyncBalance 在 SyncPosition 之后）。
- 所有写操作（POST /order）必须 `dry_run=true`（未真实发送）。

## 2. 只读生命周期（LiveReadOnly）

| # | 步骤 | API | 方法 | 说明 |
|---|------|-----|------|------|
| 1 | 加载市场 | /markets | GET | 读取市场列表 |
| 2 | 加载订单簿 | /book?token_id= | GET | 读取订单簿 |
| 3 | 检查余额 | /balances | GET | 如已认证则读取，否则跳过 |
| 4 | 同步持仓 | /positions | GET | 如已认证则读取，否则跳过 |
| 5 | 同步余额 | /balances | GET | 如已认证则读取，否则跳过 |
| 6 | 完成 | — | — | 生命周期完成 |

### 禁止操作

- ❌ Place Order（POST /order）
- ❌ Cancel Order（DELETE /order）

校验器强制：trace 中不得出现任何 POST / DELETE / PUT / PATCH，且不得出现下单相关状态（BuildingOrder / SubmittingOrder / WaitingResult / SyncOrder / SyncTrade）。

## 3. 端点与认证

| 端点 | 需认证 | Mock 数据 |
|------|--------|-----------|
| GET /time | 否 | server-time.json |
| GET /markets | 否 | markets.json |
| GET /market?condition_id= | 否 | market-detail.json |
| GET /book?token_id= | 否 | orderbook.json |
| GET /trades | 是 | trades.json |
| GET /balances | 是 | balance.json |
| GET /orders | 是 | orders.json |
| GET /positions | 是 | positions.json |

Mock 数据统一存放于顶层 `fixtures/`，与 `pm-api-test` 共享，禁止重复。
