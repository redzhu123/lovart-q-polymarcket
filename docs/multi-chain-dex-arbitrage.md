# 多池、多协议与多链套利设计

## 当前能力

系统仍坚持“只模拟、无钱包、无签名、无广播”的安全边界。目前由三层组成：

1. `dex_v2` 是可运行的同链扫描器，支持多个 Uniswap V2 兼容 Factory 自动发现池，并生成二至四跳循环路由。
2. `dex_router` 是多协议候选与精确报价层，支持 V2、Uniswap V3 Quoter、Curve `get_dy` 和 0x Swap API。Balancer 路由在 V1 通过 0x 聚合报价覆盖，尚未直接编码 Vault `queryBatchSwap`。
3. `multi_chain` 为每条链创建独立运行时并并发同步。不同链之间不共享区块高度、Gas、池缓存或最终性设置。

## 同链搜索与报价

候选发现把代币和流动性池建成有向图，最多搜索四跳简单循环，并限制候选数量和每个币对的边数。边际价格仅用于快速筛选；最终收益必须按整数最小单位逐跳调用协议报价器，不能用浮点数决定盈利。

0x 返回的 firm quote 可通过 `RpcQuoteVerifier` 再执行 `eth_call` 和 `eth_estimateGas`。API Key 只能从环境变量注入，不应写入 TOML 或提交到 Git。

## 多链 Shadow 扫描

启动命令：

```powershell
cargo run -p pm-cli-app -- dex-multi multi-chain-arbitrage.toml
```

`multi-chain-arbitrage.toml` 引用每条链自己的 DEX 配置。当前启用 Polygon、Base 和 Arbitrum One；各链使用独立代币、Factory、核心价格池和 RPC。继续增加链时应先准备独立配置，并核对：

- `chain_id`、RPC 与确认模式；
- 代币地址、小数位和锚定币；
- Factory、池手续费与原生币价格池；
- 各跳 Gas 基准和风控阈值。

Base 与 Arbitrum 属于 L2，最终费用还包含 L1 数据费。当前 Shadow 模式通过更高的 Gas units 与缓冲保守估计，但不能替代执行器级模拟和链专用 L1 fee oracle，因此机会结果仍应视为候选评估。

## 跨链纸面套利

跨链交易无法与普通同链交易一样获得全局原子性。V1 采用预置库存：源链持有锚定币，目标链预先持有待卖资产，两边分别模拟成交；桥只用于之后的库存再平衡，并计入成本与额外风险缓冲。

### 跨链最短路径

`dex-multi` 会在三条链同步完成后构建跨链超级图：

- 节点：链 ID、统一资产 ID、链上代币地址和小数位；
- Swap 边：实时 V2 储备、协议手续费与该链 Gas 估值；
- Bridge 边：资产小数位转换、桥费、固定成本和预计时间；
- 起终点：同一条链上的 USDC，路径至少包含两条桥边并回到起点。

候选阶段采用受限 Bellman-Ford。将扣费后边际汇率转换为 `-ln(rate)`，乘法收益就变为可累加权重；总权重为负代表理论乘积大于 1。算法同时限制 `max_steps` 和 `max_bridges`，避免无界负环和不可执行长路径。

候选通过后，扫描器不会沿用浮点结果，而是使用每个池的 `U256` 储备、V2 常数乘积公式、桥费、小数位、Swap Gas、桥固定成本和风险缓冲逐腿复算。只有净利润和 ROI 同时通过门槛才输出。

当前桥费来自 `cross-chain-routing.toml` 的保守估值，并非实时成交报价。`LiFiQuoteProvider` 和 `AcrossQuoteProvider` 已作为复核接口保留，后续执行前必须获取实时报价。即使路径盈利，它仍是非原子跨链策略，需要预置库存并承担桥延迟和两端价格变化风险。

```powershell
cargo run -p pm-cli-app -- cross-chain-paper cross-chain-paper.toml
```

纸面检测依次检查报价时效、两端库存、数量匹配、双链 Gas、再平衡成本、净利润和 ROI。`LiFiQuoteProvider` 与 `AcrossQuoteProvider` 可获取桥报价，但桥报价不是成交保证，也不能消除目标链价格变化、填充延迟、重组、桥故障或库存失衡风险。

## 本地分叉验证

安装 Foundry 后，可分别启动各链 Anvil 分叉来复现固定区块状态：

```powershell
anvil --fork-url $env:POLYGON_RPC_URL --port 8545
anvil --fork-url $env:BASE_RPC_URL --port 8546
anvil --fork-url $env:ARBITRUM_RPC_URL --port 8547
```

把对应配置的 RPC 改为本地端口后运行 Shadow 或 `simulate_only`。只有 `simulate_only` 且配置了已部署执行器时，系统才会使用 `eth_estimateGas`；Shadow 仍使用按跳数配置的 Gas units 基准，再乘实时 Gas Price 和原生币价格。

## 明确不支持

- 不执行真实授权、签名或交易广播；
- 不承诺跨链原子套利；
- 不把不同链的区块号或报价时间视为同步；
- 不把聚合器或桥 API 返回值直接视为可成交利润；
- 当前没有 Balancer Vault 直连报价器和跨链自动再平衡执行器。
