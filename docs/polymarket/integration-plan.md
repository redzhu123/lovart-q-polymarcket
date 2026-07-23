# Polymarket 真实 Gateway 接入方案

> 最后更新: 2026-07-23 | 目标: 将项目从 Mock/Simulation Only 渐进迁移到真实 Polymarket 交易

---

## 1. 前置条件清单

在开始任何接入工作之前，必须确认：

### 1.1 基础设施

| # | 条件 | 说明 |
|---|------|------|
| 1 | Polygon 钱包 | 创建专用交易钱包（与主资金钱包分离） |
| 2 | MATIC 余额 | Polygon 链上有足够 MATIC 支付 Gas |
| 3 | pUSD 获得 | 通过 Polymarket 前端充值 pUSD（或 wrap USDC.e） |
| 4 | CTF Exchange 授权 | `approve(CTF_EXCHANGE, MAX_UINT256)` on pUSD |
| 5 | Conditional Tokens 授权 | `setApprovalForAll(CTF_EXCHANGE, true)` |
| 6 | API 网络可达 | `https://clob.polymarket.com` 和 `https://gamma-api.polymarket.com` 可访问 |
| 7 | API Key | 通过 L1 EIP-712 认证获取 L2 API 凭证 |

### 1.2 开发环境

| # | 条件 | 说明 |
|---|------|------|
| 1 | `ethers` v2 crate | EIP-712 签名支持 |
| 2 | `hmac` + `sha2` crate | L2 HMAC 签名 |
| 3 | `tokio-tungstenite` crate | WebSocket 支持 |
| 4 | 测试钱包私钥 | 仅用于 devnet/mainnet-fork 测试，绝不能包含真实资金 |

---

## 2. 项目现状评估

### 2.1 已完成（可直接利用）

| 组件 | 状态 | 说明 |
|------|------|------|
| `MarketDataProvider` trait | ✅ 成熟 | GammaProvider + ClobProvider + MockProvider |
| `ExecutionGateway` trait | ✅ 有 Mock 实现 | MockGateway 实现了完整的 submit/cancel/status |
| `Order` 模型 (11 状态) | ✅ 完整 | 状态机覆盖全部生命周期 |
| `ExecutionPipeline` | ✅ 完整 | Builder → Validator → Queue → Scheduler |
| `ExecutionValidator` (8 rules) | ✅ 完整 | 需要新增 TickSize/Balance 规则 |
| `Scheduler` (Token Bucket) | ✅ 完整 | 需要适配 CLOB 限流参数 |
| `Portfolio` (资金管理) | ✅ 完整 | 需要对接真实余额 |
| `RiskEngine` (11 rules) | ✅ 完整 | Mock 资产上下文中运行，需验证在真实场景 |
| `TradingProvider` trait | ✅ 有 Mock 实现 | MockTradingProvider 返回固定模拟数据 |
| `CredentialManager` | ✅ 有接口 | 需集成 L1/L2 认证 |
| 代理支持 | ✅ 已有 | `HTTPS_PROXY=http://127.0.0.1:7890` |

### 2.2 缺失（需要从零实现）

| 组件 | 优先级 | 工作量估计 |
|------|--------|-----------|
| EIP-712 订单签名器 | P0 🔴 | 3-5 天 |
| L2 HMAC 请求签名中间件 | P0 🔴 | 1-2 天 |
| CLOB HTTP Client (`POST /order`, `DELETE /order` 等) | P0 🔴 | 2-3 天 |
| Tick Size 验证规则 | P1 🟡 | 0.5 天 |
| 余额/授权验证规则 | P1 🟡 | 0.5 天 |
| WebSocket Market Client | P2 🟢 | 3-5 天 |
| WebSocket User Client | P1 🟡 | 2-3 天 |
| Data API Client (对账) | P2 🟢 | 1-2 天 |
| 真实订单 → 本地 Order 状态映射 | P1 🟡 | 1 天 |
| 异步状态更新机制 (WS 驱动) | P1 🟡 | 2-3 天 |
| `GatewayConfig` + 安全开关 | P0 🔴 | 0.5 天 |
| CLI `live` 模式 | P2 🟢 | 1 天 |

---

## 3. 六阶段迁移路线图

### Phase A: 影子模式 (Shadow Mode) — 第 1-2 周

**目标**: 实现完整的代码路径但不上链

**具体任务**:

```
A1. 实现 EIP-712 订单签名模块
    └── crates/gateway/src/clob/signing.rs
         ├── OrderSigner trait
         ├── EcdsaSigner (EOA)
         └── 单元测试 (已知测试向量验证签名)

A2. 实现 L2 HMAC 中间件
    └── crates/gateway/src/clob/auth.rs
         ├── L2HeaderBuilder
         ├── 时间同步 (GET /time)
         └── 单元测试 (固定 timestamp+secret 验证 HMAC)

A3. 实现 CLOB HTTP Client (Shadow Mode)
    └── crates/gateway/src/clob/client.rs
         ├── 所有 POST/DELETE 请求构造
         ├── 请求日志记录（Shadow 模式）
         └── live_enabled=false 时仅 log 不发送

A4. ShadowGateway 实现 ExecutionGateway trait
    └── crates/gateway/src/clob/gateway.rs
         ├── submit_order → log "[SHADOW] Would submit order: {json}"
         ├── cancel_order → log "[SHADOW] Would cancel: {id}"
         └── get_order / get_balance → 返回 Mock 数据
```

**验收标准**:
- [ ] ShadowGateway 通过所有现有 Execution 测试
- [ ] 日志中包含完整的、格式正确的订单 JSON
- [ ] 签名模块的单元测试使用已知测试向量验证通过
- [ ] HMAC 模块的单元测试使用固定输入验证通过

### Phase B: 只读模式 (Read-Only) — 第 2-4 周

**目标**: 所有查询操作使用真实数据

**具体任务**:

```
B1. 实现真实查询
    ├── GET /balances   → 真实余额
    ├── GET /tick-size   → 真实 tick size
    ├── GET /book        → 真实订单簿
    ├── GET /neg-risk    → 真实 neg_risk 标志
    └── GET /time        → 时间同步

B2. 新增 Validator 规则
    └── crates/execution/src/validator.rs
         ├── TickSizeRule: 价格对齐市场 tick size
         └── AllowanceRule: pUSD allowance 充足

B3. 集成到 Scanner
    ├── 用 CLOB /book 替换或补充 Gamma 的 normalized 价格
    ├── 基于真实 bid/ask 计算套利机会
    └── 非 SUM<0.99，而是 bid_yes + bid_no > 1.0

B4. 集成到 OrderBuilder
    └── 精度转换: f64 价格 → uint256 makerAmount/takerAmount
         └── 对齐 tick_size
```

**验收标准**:
- [ ] Scanner 能从 CLOB 获取真实订单簿
- [ ] 基于真实 bid/ask 的套利检测正常工作
- [ ] 余额查询返回真实数据（非固定 10000）
- [ ] TickSize 验证规则在测试中正确拒绝/接受订单
- [ ] 无真实交易发生（下单仍走 Mock）

### Phase C: 沙盒模式 (Sandbox) — 第 4-5 周

**目标**: 以最小金额完成首次真实订单

**具体任务**:

```
C1. 开启 live_enabled = true
    └── 设置单笔最大金额 = 1 pUSD

C2. 实现完整的 POST /order 流程
    ├── L2 认证头
    ├── EIP-712 签名
    ├── 发送请求
    └── 解析响应 → OrderStatus::Accepted

C3. 实现订单取消
    └── DELETE /order

C4. 实现订单状态轮询
    ├── GET /order/{id} (定时轮询)
    └── 更新本地 Order 状态

C5. 沙盒自动撤单
    ├── 下单后 5 秒自动取消
    └── 确保测试不留下活跃订单
```

**验收标准**:
- [ ] 至少成功完成 10 次 "下单→成交/取消" 循环
- [ ] 零未取消的活跃订单残留
- [ ] 所有订单在 Data API `/trades` 中可查
- [ ] 余额变动与订单金额一致

### Phase D: 影子交易 (Paper Trading) — 第 5-7 周

**目标**: 真实市场数据 + 真实订单流程 + 虚拟资金

**具体任务**:

```
D1. 实现 WebSocket User Channel
    └── crates/gateway/src/ws/user.rs
         ├── 连接认证
         ├── order 事件处理 → 更新 Order 状态
         └── trade 事件处理 → 更新 Position

D2. 实现 WebSocket Market Channel
    └── crates/gateway/src/ws/market.rs
         ├── 订阅关注的 token_id
         ├── book 事件 → 更新本地订单簿缓存
         └── tick_size_change 事件 → 更新 tick size 缓存

D3. Paper Trading 模式
    ├── 所有订单真实提交到 CLOB
    ├── 但 Portfolio 使用虚拟资金
    ├── 订单金额虚拟（但实际使用极小金额）
    └── 对比纸面 PnL vs 实际 PnL

D4. 实现异步状态更新
    └── crates/execution/src/pipeline.rs
         └── WS 事件驱动的状态转换（替代同步等待）
```

**验收标准**:
- [ ] WebSocket 连接稳定运行 ≥ 24 小时（自动重连正常）
- [ ] Paper Trading 运行 ≥ 1000 次扫描周期
- [ ] 纸面 PnL 与实际 PnL 偏差 < 5%
- [ ] WS 断开后自动降级到 REST 轮询

### Phase E: 小额真金 (Small Live) — 第 7-9 周

**目标**: 完全真实的完整交易循环，风险可控

**具体任务**:

```
E1. 真实资金配置
    ├── 初始资金: 100 pUSD
    ├── 单笔最大: 5 pUSD
    ├── 持仓限制: 3 个
    ├── 日亏损上限: 10 pUSD
    └── 总亏损上限: 30 pUSD

E2. 实现 Data API 对账
    ├── crates/gateway/src/data_api/client.rs
    ├── 定时 (每 5 分钟) 从 /positions 获取真实持仓
    ├── 对比本地 Portfolio 记录
    └── 差异自动修正（以 Data API 为准）

E3. 熔断机制
    ├── 连续 N 次订单失败 → 自动切换为 readonly
    ├── 余额低于阈值 → 停止交易 + 告警
    ├── 异常价格检测 → 拒绝订单
    └── CLOB 返回 503 (cancel-only) → 暂停新订单

E4. 增强风控
    └── RiskEngine 接入真实持仓和余额数据
```

**验收标准**:
- [ ] 小额交易连续运行 ≥ 7 天无异常
- [ ] 零意外亏损（所有亏损在风控限制内）
- [ ] 对账机制检测到 ≥1 次差异并成功修正
- [ ] 熔断机制在触发条件满足时正确激活

### Phase F: 全量交易 (Full Live) — 第 9-12 周

**目标**: 生产级真实交易

**具体任务**:

```
F1. 移除 Mock 降级（或保留仅为测试）
    └── ExecutionPipeline 默认使用 RealGateway

F2. 生产级监控
    ├── Prometheus metrics (订单量、成交率、PnL、延迟)
    ├── 告警规则 (余额异常、连续失败、WebSocket 断开)
    └── 健康检查端点

F3. 持久化增强
    ├── 订单生命周期日志（完整审计追踪）
    ├── 每日 PnL 报告
    └── 交易统计 Dashboard

F4. 性能优化
    ├── 批量 API 调用 (/books, /prices, /orders)
    ├── 连接池复用
    └── 本地缓存优化
```

**验收标准**:
- [ ] 30 天无计划外停机
- [ ] 订单成功率 ≥ 95%
- [ ] PnL 在可接受范围内
- [ ] 无监管/合规问题

---

## 4. 关键风险与缓解

| 风险 | 严重性 | 缓解措施 |
|------|--------|----------|
| **EIP-712 签名实现错误** | 🔴 高 | 使用已知测试向量验证；与官方 SDK 输出对比 |
| **Tick Size 计算错误** | 🔴 高 | 从 CLOB API 实时获取；WS 监听 tick_size_change |
| **网络分区/超时导致重复下单** | 🔴 高 | client_order_id 幂等性；超时后先查询再决定重试 |
| **CLOB 版本升级不兼容** | 🟡 中 | 监控官方公告；维护 SDK 版本矩阵 |
| **pUSD 余额不足导致订单被拒** | 🟡 中 | 下单前查询余额；Cache 有效期 ≤ 5s |
| **API Key 泄露** | 🔴 高 | 凭证加密存储；Git 忽略敏感文件；环境变量注入 |
| **Polygon Gas 不足** | 🟡 中 | 监控 MATIC 余额；Gas 价格预警 |
| **WebSocket 断连** | 🟡 中 | 自动重连 + 指数退避；降级 REST 轮询 |
| **策略在真实环境表现差于模拟** | 🟡 中 | 逐步放量（Phase E 中验证）；严格的亏损限制 |

---

## 5. 不实施的功能（明确排除）

以下功能**不在**当前调研和接入范围内，避免范围蔓延：

| 功能 | 排除原因 |
|------|----------|
| AMM 交易 | CLOB V2 已移除 AMM，仅限订单簿交易 |
| 跨链桥接 | 由 Polymarket 前端处理，API 侧不需要 |
| Token Split/Merge | 高级 CTF 操作，低优先级 |
| Rewards 自动化 | 非交易核心功能 |
| 多钱包并行 | 增加复杂度，后续版本考虑 |
| 前端/UI | 本项目为纯后端量化系统 |
| Sports 市场专项 | API 接口与普通市场相同 |

---

## 6. 依赖时间线

```
Week 1-2:  Phase A ─── 签名 + 认证基础
Week 2-4:  Phase B ─── 只读数据流打通
Week 4-5:  Phase C ─── 沙盒首次真实订单
Week 5-7:  Phase D ─── WS + Paper Trading
Week 7-9:  Phase E ─── 小额真金验证
Week 9-12: Phase F ─── 全量生产
```

**关键路径**: Phase A (签名+认证) → Phase C (首次真实订单) 是最关键的路径，约 4-5 周。

---

## 7. 代码改动影响面

### 7.1 需要修改的 Crate

| Crate | 改动量 | 类型 |
|-------|--------|------|
| `crates/gateway/` | 🔴 大量新增 | 新建 CLOB/WS/Data API 客户端 |
| `crates/execution/` | 🟡 中等改动 | Gateway trait 扩展、验证规则、状态管理 |
| `crates/trading/` | 🟡 中等改动 | Credential、Session、Provider 真实实现 |
| `crates/scanner/` | 🟢 轻度改动 | 新增 WsMarketProvider、调整机会检测逻辑 |
| `crates/risk/` | 🟢 轻度改动 | 接入真实余额/持仓数据 |
| `crates/portfolio/` | 🟢 轻度改动 | 真实资金同步 + 对账 |
| `crates/models/` | 🟢 轻度改动 | 新增 GatewayConfig 等配置结构 |
| `apps/cli/` | 🟢 轻度改动 | 新增 CLI 子命令 |
| `crates/core/` | ⚪ 无改动 | - |
| `crates/utils/` | ⚪ 无改动 | - |
| `crates/storage/` | ⚪ 无改动 | - |
| `crates/orderbook/` | ⚪ 无改动 | - |
| `crates/opportunity/` | ⚪ 无改动 | - |

### 7.2 不变的部分

以下组件**完全不受影响**：
- `pm-core` (Side, CoreError)
- `pm-utils` (格式化、统计工具)
- `pm-storage` (CSV 读写)
- `pm-models` 的核心 DTO (UnifiedMarket, PriceLevel, OrderBook)
- `pm-orderbook` (分析逻辑，数据来源可切换)
- `pm-opportunity` (机会引擎)
- `pm-tracker` / `pm-recorder` (记录层)
- `pm-metrics` (指标计数器)

---

## 8. 配置示例

### 8.1 provider.toml (扩展)

```toml
environment = "paper"       # "paper" | "sandbox" | "live"
default_provider = "clob"   # "mock" | "gamma" | "clob"

[polymarket]
http_url = "https://clob.polymarket.com"
data_api_url = "https://data-api.polymarket.com"
ws_market_url = "wss://ws-subscriptions-clob.polymarket.com/ws/market"
ws_user_url = "wss://ws-subscriptions-clob.polymarket.com/ws/user"
chain_id = 137
ctf_exchange = "0xE111180000d2663C0091e4f400237545B87B996B"
neg_risk_adapter = "0xe222E2E2E2E2E2E2E2E2E2E2E2E2E2E2E2e2220F59"
signature_type = 3                       # POLY_1271 (新用户推荐)

[polymarket.credential]
wallet_address = "0x..."                 # 或环境变量 POLY_ADDRESS
api_key = "..."                          # 或环境变量 POLY_API_KEY
api_secret = "..."                       # 或环境变量 POLY_SECRET
api_passphrase = "..."                   # 或环境变量 POLY_PASSPHRASE

[polymarket.session]
ttl_seconds = 3600
heartbeat_interval_seconds = 30
health_check_interval_seconds = 60
time_sync_on_start = true

[polymarket.rate_limit]
max_orders_per_second = 10
max_orders_per_minute = 100
burst_size = 5
max_book_requests_per_second = 50

[polymarket.ws]
enabled = false                          # Phase D 启用
auto_reconnect = true
reconnect_max_retries = 10
ping_interval_seconds = 10
```

### 8.2 config.toml (扩展)

```toml
# 现有配置保持不变
# ...

[gateway]
mode = "mock"                # Phase A: "shadow" | Phase B: "readonly" | Phase C+: "live"
live_enabled = false         # 总安全开关
max_order_notional = 5.0     # 单笔最大 pUSD
max_daily_loss = 10.0        # 日亏损上限
max_total_loss = 30.0        # 总亏损上限
circuit_breaker_enabled = true
circuit_breaker_consecutive_failures = 5
```

---

## 9. 检查清单 (Go/No-Go)

进入下一阶段之前，必须完成前阶段的所有检查项：

### Phase A → B: Go 条件
- [ ] 所有 Phase A 单元测试通过（≥50 测试）
- [ ] ShadowGateway 输出日志经人工审查格式正确
- [ ] 签名测试向量验证通过
- [ ] Code review 完成

### Phase B → C: Go 条件
- [ ] `live_enabled` 总开关测试通过（false 时零真实请求）
- [ ] 余额查询返回真实数据
- [ ] Tick size 规则正确拒绝 3+ 个错误价格
- [ ] 完整 Scan 周期使用 CLOB 数据无 panic

### Phase C → D: Go 条件
- [ ] ≥10 次沙盒订单全部无异常
- [ ] 零活跃订单残留
- [ ] 全部订单在 Data API 可验证

### Phase D → E: Go 条件
- [ ] WS 稳定运行 ≥ 24h
- [ ] Paper Trading 运行 ≥ 1000 周期
- [ ] 纸面 PnL 偏差 < 5%

### Phase E → F: Go 条件
- [ ] ≥7 天无异常小额交易
- [ ] 零意外亏损
- [ ] 对账机制验证通过
- [ ] 熔断机制验证通过
