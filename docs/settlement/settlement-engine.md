# Settlement Engine（成交结算引擎）

## 概述

Settlement Engine 是成交事件唯一处理中心。

## 架构

```
Trade Fill Event
      │
      ▼
┌──────────────────────────┐
│  Settlement Engine        │
│  process_fill()           │  ← 唯一入口
│      │                   │
│      ▼                   │
│  Validation (7 规则)      │
│      │                   │
│      ▼                   │
│  Fee Calculation          │
│      │                   │
│      ▼                   │
│  Position Update          │
│      │                   │
│      ▼                   │
│  Balance Update           │
│      │                   │
│      ▼                   │
│  PnL Update               │
│      │                   │
│      ▼                   │
│  Ledger Entry             │
│      │                   │
│      ▼                   │
│  Settlement Completed     │
└──────────────────────────┘
```

## 约束

- 禁止 OMS 修改资金
- 禁止 PMS 直接处理成交
- 禁止 Gateway 更新持仓
- 所有成交必须经过 Settlement Engine

## 核心 API

### process_fill()

```rust
pub fn process_fill(&mut self, event: &TradeFillEvent) -> SettlementResult
```

接收成交事件，执行完整结算工作流，返回结算结果。

## 配置

```rust
pub struct SettlementConfig {
    pub initial_capital: f64,
    pub default_account_id: String,
    pub fee_rule: FeeRule,
    pub enable_event_bus: bool,
}
```

## CLI 命令

```bash
cargo run -- settlement   # 查看最近结算
cargo run -- ledger       # 查看资金流水
cargo run -- fees         # 查看手续费规则
```
