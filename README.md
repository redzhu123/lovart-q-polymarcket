# Polymarket Quant Platform V1.02

> Research Platform：持续采集数据 / 发现机会 / 验证策略 / 模拟交易 / 统计收益。
> **Simulation Only** -- 不连接钱包 / 不真实交易 / 不签名 / 不下单 / 无 Polygon / WebSocket / 数据库 / Redis。

## 定位
不是自动赚钱机器人、不是 MEV / 高频 / 链上套利机器人。是一个**可长期维护的量化研究平台**：
持续采集 -> 发现机会 -> 验证策略 -> 模拟交易 -> 统计收益。未来可替换 Execution 接入真实交易。

## Workspace 结构
```
apps/
  scanner/   pm-scanner      专用持续扫描二进制（cargo run -p pm-scanner-app）
  cli/       pm-cli          统一研究 CLI（默认二进制，cargo run -- <mode>）
crates/
  core/      pm-core         跨 crate 公共原语（Side / Error）
  models/    pm-models       共享 DTO（市场/机会/配置）+ Config::load
  utils/     pm-utils        纯工具（格式化 / 统计数学）
  storage/   pm-storage      通用 CSV 原语 + 机会文件读取
  tracker/   pm-tracker      机会生命周期跟踪器
  scanner/   pm-scanner      扫描子系统：datasource(数据层) + market + driver + display + stats + pipeline + health + diagnostics
  recorder/  pm-recorder     机会生命周期 CSV 记录器
  shadow/    pm-shadow       影子交易（Simulation Only）
  portfolio/ pm-portfolio    组合资金管理 + RiskManager
  paper/     pm-paper        Paper Trading 引擎 + 历史回放
  execution/ pm-execution    Execution Simulator（订单执行模拟）
  strategy/  pm-strategy     Strategy trait + DefaultStrategy
  metrics/   pm-metrics      统一指标计数器
  backtest/  pm-backtest     历史回放 + 回测 + 报告
config.toml                  全部可调参数（不读环境变量）
rust-toolchain.toml          固定 MSVC target（可复现）
data/                        运行期 CSV
```

## 依赖图（DAG，无环）
```
core / utils (leaf)
  -> models -> storage
  -> tracker, recorder, shadow, portfolio, paper, execution
  -> strategy, metrics, backtest
  -> scanner (driver)
  -> apps (cli / scanner)
```
详细依赖见各 crate README。

## 运行
```
cargo run -- scan            # 扫描 + Shadow + Paper + Execution（增强可观测性）
cargo run -- diagnose        # 诊断模式：单次扫描 + 完整诊断报告（不进入循环）
cargo run -- datasource      # 数据源诊断：Provider / 能力 / 健康 / 缓存 / 校验 / 快照
cargo run -- replay          # 历史回放
cargo run -- paper           # 历史机会走 Paper 引擎回放
cargo run -- backtest        # 完整回测
cargo run -- execution-test  # Execution 压测
cargo run -- report          # 汇总报告
cargo run -p pm-scanner-app  # 专用持续扫描
cargo build --workspace      # 构建全部
cargo test  --workspace      # 全部测试
```

## V1.01 可观测性与诊断（Observability & Diagnostics）

**唯一目标：建立完整可观测性，不新增任何交易/策略/套利逻辑。** 用于回答
"为什么 Opportunity=0 / Paper Order=0 / Execution Order=0" 这类定位问题。

- **日志级别** `config.toml` 的 `[logging] log_level`：`ERROR/WARN/INFO/DEBUG/TRACE`
  （默认 `DEBUG`）。`ERROR` 仅错误；`INFO` 统计仪表盘；`DEBUG` +HTTP/过滤/样本明细；
  `TRACE` +全量市场转储。
- **启动健康检查**（`crates/scanner/src/health.rs`）：Config / CSV / Storage / Clock /
  Memory（Windows `GlobalMemoryStatusEx`）/ API / JSON，任一失败不进入扫描循环。
- **统一模块计时**（`crates/scanner/src/pipeline.rs`）：每轮各阶段产出 `ModuleStats`
  （耗时/输入/输出），打印"执行时间线"表 + "流水线时间线"，一眼定位数据在哪一步消失。
- **诊断模式** `cargo run -- diagnose`（`crates/scanner/src/diagnostics.rs`）：单次扫描
  输出完整诊断报告 -- 启动检查 / HTTP / JSON 首市场全字段+空字段标注 / 市场统计 /
  过滤漏斗 / 策略拒绝明细 / 随机市场快照 / 模块时间线 / 流水线 / 系统汇总 / 12 问作答。
- **HTTP/JSON 诊断**：逐页 URL/状态/字节/耗时 + 反序列化耗时 + Rate-Limit 头；
  JSON 解析失败打印响应预览。
- **过滤统计**：接收→已关闭→不活跃→缺价→数据无效→策略过滤→机会 的完整漏斗，
  Scanner 不再静默 `continue`。
- 常态结论：Gamma `outcomePrices` 为归一化中间价（YES+NO≡1.0），结构上无 SUM<阈值，
  故常态 0 机会 -> 各引擎 0 输出，**属预期行为非故障**；真实套利需后续接入 CLOB。

## V1.02 市场数据引擎（Market Data Engine）

**唯一目标：重构整个数据层，统一数据源接口。不改动任何交易/策略/Shadow/Execution 逻辑。**
Gamma 只提供市场信息，不提供真实订单簿/买卖价 -> 无法支撑真实套利；本版建立
`MarketDataProvider` Trait 抽象，未来新增一个 Provider（如 CLOB）即可接入真实行情。

- **统一数据源接口**（`crates/scanner/src/datasource/`）：`MarketDataProvider` Trait
  （`fetch_markets` / `fetch_orderbooks` / `fetch_prices` / `health_check`）。Scanner
  只依赖 Trait + `DataSourceManager`，**不再直接访问 HTTP**。
- **Provider**：`GammaProvider`（市场/流动性，无订单簿）、`MockProvider`（测试/离线演示）。
  `clob` 配置项已预留，未实现会返回明确错误。
- **DataSourceManager**：按 `config.datasource.provider`（gamma/clob/mock）选择 Provider，
  Scanner 无需感知具体 Provider；切换数据源只改配置。
- **统一市场模型** `UnifiedMarket`（`crates/models/src/datasource.rs`）：所有 Provider
  最终转换为 `UnifiedMarket`（MarketId/Question/Status/YES/NO/Volume/Liquidity/...）。
  Scanner 此后只认识 `UnifiedMarket`。
- **OrderBook 模型**：`best_bid/best_ask/spread/depth`；Provider 不支持则返回 None/空，**绝不伪造**。
- **能力声明** `ProviderCapability`：启动打印【数据源能力】（市场/订单簿/成交/买卖价/流动性/真实套利）。
- **Data Validator**（`datasource/validator.rs`）：question 非空、价格 [0,1]、volume/liquidity
  非负；非法数据统计 + tracing 打印。
- **内存缓存**（`datasource/cache.rs`）：TTL 默认 10s（`config.datasource.cache_ttl`），
  Scanner 优先读缓存、Provider 负责刷新；无 Redis / 无数据库。
- **市场快照**（`datasource/snapshot.rs`）：每轮保存到 `data/market_snapshots.csv`
  （时间/市场数/Provider/内容哈希），便于以后 Replay。
- **市场数据统计**（`datasource/statistics.rs`）：每轮打印【市场数据统计】
  （数据源/Market/OrderBook/Price/Liquidity/Invalid/Cached/更新时间）。
- **数据源诊断** `cargo run -- datasource`：输出 Provider / 能力 / 健康 / 延迟 / 市场数 /
  缓存 / 校验 / 快照，快速检查数据源（mock 可离线运行）。
- **配置**：`config.toml` 新增 `[datasource]` 段（provider / cache_ttl）。
- **行为等价**：GammaProvider 拉取与原 `fetch_active_markets` 同 URL/分页/可观测性，
  同样 ~2100 归一化市场 -> 同样 0 机会；现有 6 个 CLI 模式行为不变。

## 设计原则
- Rust Idiomatic、SOLID、高内聚低耦合、组合优于继承。
- 禁止 DDD / CQRS / Event Sourcing / 微服务 / Plugin / IoC / 复杂设计模式。
- 禁止 `unwrap/expect/panic`，全部 `Result` + `?`（`thiserror` + `anyhow`）。
- 允许 Tokio 异步；不过早优化、无 lock-free / unsafe / SIMD / rayon。
- 网络需代理时设 `HTTPS_PROXY`（见 memory）；reqwest 用 rustls-tls。

## V1.0 红线（不实现）
真实钱包 / Polygon / 签名 / 私钥 / 真实订单 / Redis / 数据库 / HTTP Server / WebSocket /
Prometheus / Grafana / Docker / Kubernetes / 消息队列 / 云部署 / AI / LLM / Agent。
