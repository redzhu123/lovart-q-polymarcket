# pm-cli 命令参考

**用法:** `cargo run -- <command>`  
**版本:** V1.08 + P2.02/P2.04/P2.05/P2.06/P3.0

---

## 核心扫描与诊断

| 命令 | 说明 |
|------|------|
| `scan` | 正常扫描 + 纸面交易 + 执行模拟器（默认模式） |
| `diagnose` | 诊断模式（单次扫描 + 完整诊断报告，不进入循环） |
| `datasource` | 数据源诊断（Provider / 能力 / 健康 / 缓存 / 校验 / 快照） |

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
| `reset --yes` | 实际删除所有历史 CSV 数据 |
| `explain` | 完整数据链路分析报告（= `explain pipeline`） |
| `explain pipeline` | 完整数据链路分析报告 |
| `explain rejections` | 拒绝原因分析 |
| `explain <id>` | 解释某个机会的详细评分 |
| `audit` | 自动数据一致性审计 |
| `trace --order <id>` | 订单链路追踪（Market → Opportunity → PaperOrder → Execution → Settlement） |

## 市场微观结构（V1.03）

| 命令 | 说明 |
|------|------|
| `market` | 市场列表（前20个活跃市场） |
| `orderbook` | 订单簿（拉取并展示前10个市场订单簿） |
| `spread` | 价差分析（前10个市场） |
| `liquidity` | 流动性分析（前10个市场，含深度分析） |

## 机会引擎（V1.04）

| 命令 | 说明 |
|------|------|
| `opportunities` | 列出全部套利机会 |
| `opps` | 同 `opportunities` |
| `top` | Top 10 机会（按评分排序） |

## 风险引擎（V1.05）

| 命令 | 说明 |
|------|------|
| `risk` | 风险仪表盘 |
| `explain-risk` | 风险规则说明（阈值/仓位/暴露/流动性等所有规则） |
| `risk-replay` | 风险回放（模拟多种场景） |

## 执行引擎（V1.06）

| 命令 | 说明 |
|------|------|
| `orders` | 订单列表（经 Gateway 查询活跃订单 + 历史 CSV 计数） |
| `execution` | 执行状态（配置/队列/调度器/CSV 路径） |
| `queue` | 队列查看（当前队列状态） |
| `exec-replay <order_id>` | 订单回放（指定订单的生命周期时间线） |

## Trading 基础设施（V1.07）

| 命令 | 说明 |
|------|------|
| `provider` | Provider 诊断（MockTradingProvider） |
| `health` | Health 诊断 |
| `session` | Session 诊断 |
| `connection` | Connection 诊断（含凭据诊断） |

## Exchange Gateway（V1.08）

| 命令 | 说明 |
|------|------|
| `gateway` | Gateway 状态与诊断（安全摘要 + 断路器状态） |
| `account` | 账户详情（余额 + 持仓） |
| `balance` | 余额查询 |

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
| `oms-orders` | OMS 订单列表（CSV 持久化，含状态分布 + Metrics） |
| `oms-order <id>` | OMS 订单详情（含完整状态历史时间线） |
| `oms-events` | OMS 事件流（最近50条） |
| `oms-demo` | 创建 5 个示例订单（演示用） |

## PMS 投资组合管理系统（P2-05）

| 命令 | 说明 |
|------|------|
| `portfolio` | 投资组合仪表盘 |
| `positions` | 全部持仓列表 |
| `pnl` | 盈亏报告 |
| `exposure` | 风险敞口报告 |

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
| `ledger` | 查看资金流水（最近20条） |
| `fees` | 查看手续费规则与示例（标准费率/零费率/示例计算） |

## 多市场统一框架（P3.0）

| 命令 | 说明 |
|------|------|
| `markets` | 列出所有已安装市场（能力模板 + 注册表） |
| `markets health` | 多市场健康检查（REST/WS/网关/认证/延迟） |
