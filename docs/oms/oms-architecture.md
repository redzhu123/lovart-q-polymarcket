# OMS 架构总览（P2-04）

> Polmaryet 量化平台 V1.09 — Order Management System
> **生成时间**：2026-07-23
> **状态**：✅ 已完成

---

## 1. 定位

OMS 是 **Execution** 与 **Gateway** 之间唯一的订单管理层。

```
┌────────────────┐
│ Strategy       │ (策略生成信号)
└────────┬───────┘
         │
┌────────▼───────┐
│ Risk Engine    │ (风控审核)
└────────┬───────┘
         │
┌────────▼───────┐
│ Execution      │ (V1.06 — 队列 + 调度器)
└────────┬───────┘
         │
┌────────▼───────┐  ★ 唯一入口  ★
│   OMS (P2-04)  │  Order 生命周期管理层
└────────┬───────┘
         │
┌────────▼───────┐
│ Gateway (P2-03)│  (交易所抽象层)
└────────┬───────┘
         │
┌────────▼───────┐
│   Exchange     │ (MockGateway / PolymarketGateway)
└────────────────┘
```

**禁止**：

- ❌ Execution 直接调用 Gateway
- ❌ Gateway 保存业务状态
- ❌ Strategy / Risk 持有 Order 引用

---

## 2. 核心职责

| 职责 | 模块 | 说明 |
| --- | --- | --- |
| 订单创建 | `lifecycle::create_order` | 生成 OMS ID，写入 Repository，发布 OrderCreated |
| 订单校验 | `validator` + `lifecycle::validate_order` | 9 条规则检查，失败直接 Rejected（不发 Gateway） |
| 订单提交 | `lifecycle::submit_order` | 状态机校验 → PendingSubmit → Submitted → 调用 Gateway |
| 订单取消 | `lifecycle::cancel_order` | 仅活跃订单可取消 |
| 订单替换 | `lifecycle::replace_order` | cancel(old) + create(new) + submit(new) |
| 状态管理 | `state_machine` | 11 态 + 1 聚合（Completed）白名单校验 |
| 事件分发 | `events` + `EventBus` | OrderEvent → Subscriber（Portfolio / Metrics / Audit） |
| 持久化 | `repository` | Memory / CSV / SQLite（接口预留） |
| 启动恢复 | `recovery` | 程序启动 → 同步所有活跃订单 → 与 Gateway 对齐 |
| 撮合预检 | `matcher` | 价格偏离度评估（WARN / REJECT） |
| 指标统计 | `metrics::OmsMetrics` | 通过订阅事件聚合指标 |

---

## 3. 模块结构

```
crates/oms/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 模块入口 + 工厂 + prelude
│   ├── api.rs              # Oms 顶层 API（业务层唯一入口）
│   ├── order.rs            # 统一 Domain Order（11 态生命周期）
│   ├── state_machine.rs    # 状态机白名单 + 校验
│   ├── lifecycle.rs        # 订单生命周期编排（create/validate/submit/cancel/replace）
│   ├── validator.rs        # 9 条校验规则
│   ├── events.rs           # OrderEvent + EventBus + Subscriber
│   ├── repository/
│   │   ├── mod.rs          # OrderRepository trait
│   │   ├── memory.rs       # InMemoryRepository
│   │   ├── csv.rs          # CsvRepository
│   │   └── sqlite.rs       # SqliteRepository（接口预留）
│   ├── recovery.rs         # 启动恢复 + sync_order
│   ├── matcher.rs          # 价格偏离评估
│   └── metrics.rs          # OmsMetrics + Subscriber
└── tests/
    ├── lifecycle.rs        # 生命周期集成测试
    ├── validation.rs       # 校验集成测试
    ├── state_machine.rs    # 状态机集成测试
    ├── events.rs           # 事件集成测试
    ├── recovery.rs         # 恢复集成测试
    └── integration.rs      # 端到端集成测试
```

---

## 4. 数据流示例

### 创建订单 → 提交到 Gateway

```
Execution → OMS::create_order(input)
  ├── Order::new（生成 OMS ID, 初始 Created）
  ├── repository.save
  ├── event_bus.publish(OrderCreated)
  └── 返回 Order

Execution → OMS::validate_order(order, ctx)
  ├── validator.validate (9 条规则)
  ├── 成功 → transition Created → Validated + publish(OrderValidated)
  └── 失败 → transition Created → Rejected + publish(ValidationFailed + OrderRejected)

Execution → OMS::submit_order(order)
  ├── transition Validated → PendingSubmit + publish(OrderPendingSubmit)
  ├── transition PendingSubmit → Submitted + publish(OrderSubmitted)
  ├── gateway.submit_order(request)
  ├── Gateway 返回 GatewayResult
  └── apply_gateway_result:
       ├── Accepted     → transition Submitted → Accepted + publish(OrderAccepted)
       ├── PartiallyFilled → transition + update_fill + publish(OrderPartiallyFilled)
       ├── Filled       → transition + update_fill + publish(OrderFilled)
       ├── Cancelled    → transition + publish(OrderCancelled)
       ├── Rejected     → transition + publish(GatewayError + OrderRejected)
       └── Expired      → transition + publish(OrderExpired)
```

---

## 5. 业务约束（再次声明）

- ❌ 禁止自动交易
- ❌ 禁止真实资金
- ❌ 禁止 Wallet
- ❌ 禁止签名
- ❌ 禁止修改 Strategy / Risk
- ✅ OMS 只负责订单生命周期管理
- ✅ 所有日志使用 `tracing`，中文输出
- ✅ 所有状态变化输出中文日志

---

## 6. 与已有模块的关系

| 模块 | 与 OMS 关系 |
| --- | --- |
| `pm-core` | OMS 依赖 `Side` |
| `pm-models` | OMS 依赖 `Config`（CLI 用） |
| `pm-execution` | OMS **复用** `Direction` 类型，**调用** `OrderRequest` |
| `pm-gateway` | OMS **依赖** `ExchangeGateway` trait，**不直接** 持有 |
| `pm-storage` | OMS 依赖 `ensure_csv` 工具 |
| `pm-strategy` | OMS **不依赖**（禁止） |
| `pm-risk` | OMS **不依赖**（禁止） |

---

## 7. CLI 命令

```
cargo run -- oms              # 健康概览 + 状态机图
cargo run -- oms-orders       # 订单列表（CSV 持久化）
cargo run -- oms-order <id>   # 订单详情（含状态历史）
cargo run -- oms-events       # 事件流
cargo run -- oms-demo         # 创建 5 个示例订单
```

CSV 文件位置：
- `data/oms_orders.csv`
- `data/oms_events.csv`
- `data/oms_status_changes.csv`

---

## 8. 测试覆盖

| 测试套件 | 覆盖范围 | 测试数 |
| --- | --- | --- |
| `lifecycle.rs` | 完整生命周期 | 7 |
| `validation.rs` | 9 条规则 | 9 |
| `state_machine.rs` | 11 态白名单 | 9 |
| `events.rs` | EventBus + Subscriber | 7 |
| `recovery.rs` | 启动恢复 + 持久化 | 6 |
| `integration.rs` | 端到端 + CSV + Gateway | 9 |
| **lib unit tests** | 各模块内部 | 108 |
| **合计** | | **155+** |

---

## 9. 后续阶段

P2-05+ 可扩展：

- 真实 SQLite 持久化（`SqliteRepository` 实现）
- 多 Gateway 并行（拆单逻辑）
- 智能路由（best execution）
- TWAP / VWAP 算法订单
- Live Trading 模式（默认 DryRun 已就绪）

---

**结论**：OMS 是企业级交易链路的咽喉。Execution 与 Gateway 之间所有订单交互必经 OMS，
禁止任何旁路。
