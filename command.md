# pm-cli 命令参考

**用法:** `cargo run -- <command>`  
**版本:** V2.0（命令合并重构）

---

## 核心扫描与诊断

| 命令 | 说明 |
|------|------|
| `scan` | 正常扫描 + 纸面交易 + 执行模拟器（默认模式） |
| `diagnose` | 诊断模式（单次扫描 + 完整诊断报告，不进入循环） |
| `datasource` | 数据源诊断（Provider / 能力 / 健康 / 缓存 / 校验 / 快照） |

## EVM DEX 循环套利

当前 V2 运行时支持 Factory 自动发现、多池二至四跳循环套利。路由层另提供 Uniswap V3 Quoter、Curve `get_dy`、0x 聚合报价及 RPC 复核适配器。默认使用 `shadow` 模式，只读取链上状态并在本地模拟，不连接钱包、不签名、不广播交易。

| 命令 | 说明 |
|------|------|
| `cargo run -p pm-cli-app -- dex-arb dex-arbitrage.toml` | 使用指定配置启动 DEX 循环套利扫描器 |
| `cargo run -p pm-cli-app -- dex-multi multi-chain-arbitrage.toml` | 启动多链并行 Shadow 扫描器 |
| `cargo run -p pm-cli-app -- cross-chain-paper cross-chain-paper.toml` | 评估一份跨链预置库存快照 |
| `cargo test -p pm-arbitrage --all-features` | 运行套利模块单元测试和集成测试 |
| `cargo check -p pm-cli-app` | 检查 CLI 与套利模块是否能够正常编译 |

Windows PowerShell 使用本地 HTTP 代理（当前代理端口为 `7897`）：

```powershell
$env:HTTP_PROXY="http://127.0.0.1:7897"
$env:HTTPS_PROXY="http://127.0.0.1:7897"
cargo run -p pm-cli-app -- dex-arb dex-arbitrage.toml
```

如果代理提供的是 SOCKS5 协议：

```powershell
$env:HTTP_PROXY="socks5h://127.0.0.1:7897"
$env:HTTPS_PROXY="socks5h://127.0.0.1:7897"
cargo run -p pm-cli-app -- dex-arb dex-arbitrage.toml
```

正常运行时，CLI 每 30 轮输出一次扫描心跳。没有套利输出表示当前池间价差未通过手续费、Gas 成本和风控门槛，并非程序停止。

启动输出会显示盈利评估使用的成本数据：

```text
实时成本数据：Gas Price=... Gwei（RPC 实时建议价）；1 POL=... USDC（链上 V2 池储备）
Gas 用量来源：Shadow 两跳/三跳/四跳基础估算=...；simulate_only 才使用 eth_estimateGas。
```

其中 Gas Price 和 POL/USDC 是实时链上数据；Shadow 模式的 Gas units 是保守估算。要获得真实 Gas units，需要部署 `V2ArbitrageExecutor` 并切换到 `simulate_only`。

快速扫描可使用以下配置。它每 500 毫秒检查一次链头；PublicNode 免费端点只允许最新日志，因此不设置回退区块：

```toml
confirmation_mode = "latest"
poll_interval_ms = 500
log_query_delay_blocks = 0
worker_count = 4
```

`worker_count` 同时限制全池储备刷新并发数。公共 RPC 建议使用 `4`；提高到 `8` 可能更快，但也更容易触发限流。实际有效扫描频率仍受 Polygon 出块速度约束，同一区块内重复轮询不会产生新的池状态。

公共 RPC 如果返回 `eth_getLogs HTTP 403`、`invalid block range params` 或要求归档节点 Token，运行时会自动降级为按新区块调用各池 `getReserves`。这种模式仍使用真实链上储备，但请求数量会增加。自建或付费归档 RPC 可根据其索引延迟适当配置 `log_query_delay_blocks`。

`simulate_only` 模式需要先部署并配置 `execution.executor_address`，它会执行 `eth_call` 和 `eth_estimateGas`。当前版本禁止 `live` 模式，不会发送真实交易。

### 多链与跨链纸面评估

`dex-multi` 会为每条启用链建立独立 RPC、池状态、路由和成本模型，并发扫描但不把不同链拼成原子交易。当前配置启用 Polygon、Base 和 Arbitrum One：Base 扫描 Uniswap V2/PancakeSwap V2，Arbitrum 扫描 Uniswap V2/PancakeSwap V2/SushiSwap V2。

`multi-chain-arbitrage.toml` 通过 `cross_chain_config_path` 启用跨链超级图。扫描器将 `(链 ID, 统一资产, 代币地址)` 作为节点，同链 V2 池作为 Swap 边，桥作为跨链边；使用受最大 6 步、最多 2 次桥接约束的 Bellman-Ford 最短路径寻找负权循环。边权为扣费后汇率的 `-ln(rate)`，只用于候选筛选，最终结果会使用 `U256` 按池储备逐腿复算。

```toml
cross_chain_config_path = "cross-chain-routing.toml"
```

当前跨链桥边使用 `cross-chain-routing.toml` 中的保守费率、固定成本和耗时估值。发现路径后仍需 LI.FI/Across 实时报价复核；跨链执行采用预置库存纸面模型，不具备原子性。

`cross-chain-paper` 使用预置库存模型：源链买入与目标链卖出分别核算，同时扣除双链 Gas、桥接再平衡成本和风险缓冲。配置中的 `observed_at_ms`、DEX 报价和桥报价必须来自同一时段；示例默认 `enabled = false`，避免把静态数字误认为实时机会。

详细设计与配置边界见 `docs/multi-chain-dex-arbitrage.md`。

## 历史回放与回测

| 命令 | 说明 |
|------|------|
| `replay` | 历史回放 |
| `paper` | 基于历史机会的纸面交易 |
| `backtest` | 完整回测 |
| `execution-test` | 执行模拟器压测 |

## 报告与审计

| 命令 | 说明 |
|------|------|
| `report` | 汇总报告（读取所有 CSV，打印平台级统计） |
| `reset` | 清空 data/*.csv（预览模式，加 `--yes` 真删） |
| `explain` | 完整数据链路分析报告（= `explain pipeline`） |
| `explain pipeline` | 完整数据链路分析报告 |
| `explain rejections` | 拒绝原因分析 |
| `explain <id>` | 解释某个机会的详细评分 |
| `audit` | 自动数据一致性审计 |
| `trace --order <id>` | 订单链路追踪（Market → Opportunity → PaperOrder → Execution → Settlement） |

## 市场数据（V1.03 + P3.0）

| 命令 | 说明 |
|------|------|
| `market` | 市场列表（前20个活跃市场） |
| `orderbook` | 订单簿（拉取并展示前10个市场订单簿） |
| `orderbook spread` | 价差分析（前10个市场） |
| `orderbook liquidity` | 流动性分析（前10个市场，含深度分析） |
| `markets` | 列出所有已安装市场（能力模板 + 注册表） |
| `markets health` | 多市场健康检查（REST/WS/网关/认证/延迟） |

## 机会引擎（V1.04）

| 命令 | 说明 |
|------|------|
| `opportunities` | 机会列表（默认 Top 10，加 `--all` 查看全部） |
| `opportunities --all` | 列出全部套利机会 |
| `opps` | 同 `opportunities` |
| `opportunity <id>` | 机会详情解释 |

## 风控引擎（V1.05）

| 命令 | 说明 |
|------|------|
| `risk` | 风险仪表盘 |
| `risk explain` | 风险规则说明（阈值/仓位/暴露/流动性等所有规则） |
| `risk replay` | 风险回放（模拟多种场景） |

## 执行引擎（V1.06）

| 命令 | 说明 |
|------|------|
| `exec` | 执行状态（配置/队列/调度器/CSV 路径） |
| `exec orders` | 订单列表（经 Gateway 查询活跃订单 + 历史 CSV 计数） |
| `exec queue` | 队列查看（当前队列状态） |
| `exec replay <id>` | 订单回放（指定订单的生命周期时间线） |

## Trading 基础设施（V1.07）

| 命令 | 说明 |
|------|------|
| `trading provider` | Provider 诊断（MockTradingProvider） |
| `trading health` | Health 诊断 |
| `trading session` | Session 诊断 |
| `trading connection` | Connection 诊断（含凭据诊断） |

## Exchange Gateway（V1.08）

| 命令 | 说明 |
|------|------|
| `gateway` | Gateway 状态与诊断（安全摘要 + 断路器状态） |
| `gateway account` | 账户详情（余额 + 持仓） |
| `gateway balance` | 余额查询 |

## API Workflow（P2-02）

| 命令 | 说明 |
|------|------|
| `workflow` | 显示当前 Workflow（配置 + 状态机 + 最近报告） |
| `workflow dryrun` | 执行 DryRun Workflow（默认，无网络/无下单） |
| `workflow replay` | 执行 Replay Workflow（从 fixtures 回放） |
| `workflow live` | 执行 Live ReadOnly Workflow（真实只读，禁止下单/撤单） |

## OMS 订单管理系统（P2-04）

| 命令 | 说明 |
|------|------|
| `oms` | OMS 健康概览 + 11态状态机图 |
| `oms orders` | OMS 订单列表（CSV 持久化，含状态分布 + Metrics） |
| `oms order <id>` | OMS 订单详情（含完整状态历史时间线） |
| `oms events` | OMS 事件流（最近50条） |
| `oms demo` | 创建 5 个示例订单（演示用） |

## PMS 投资组合管理系统（P2-05）

| 命令 | 说明 |
|------|------|
| `portfolio` | 投资组合仪表盘 |
| `portfolio positions` | 全部持仓列表 |
| `portfolio pnl` | 盈亏报告 |
| `portfolio exposure` | 风险敞口报告 |

## 认证与钱包（P2-06）

| 命令 | 说明 |
|------|------|
| `auth health` | 认证健康诊断（凭据/会话/Token/认证） |
| `auth session` | 会话诊断 |
| `auth credential` | 凭据诊断（脱敏显示） |
| `wallet health` | 钱包健康诊断（钱包/余额/授权/Nonce） |
| `wallet balance` | 余额查询 |
| `wallet account` | 账户列表（脱敏显示） |

## 结算引擎（P2-06）

| 命令 | 说明 |
|------|------|
| `settlement` | 查看最近结算（含模拟数据演示） |
| `settlement ledger` | 查看资金流水（最近20条） |
| `settlement fees` | 查看手续费规则与示例（标准费率/零费率/示例计算） |
