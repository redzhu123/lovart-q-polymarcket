# EVM V2 循环套利

`pm-arbitrage::dex_v2` 子系统支持有界的同链 Uniswap V2 兼容循环套利：

- V1：两个不同池组成的 `A -> B -> A` 两跳循环。
- V2：三个不同池、三个不同路径代币组成的 `A -> B -> C -> A` 三角循环。

系统不会执行任意深度的全图搜索。`max_route_hops` 只能设置为 `2` 或 `3`，真实交易签名和广播仍然关闭。

## 处理流水线

```text
eth_getLogs(Sync) / 定期 getReserves
  -> PoolStateCache 池状态缓存
  -> RouteIndex.routes_by_pool 路由反向索引
  -> 一致性 StateSnapshot
  -> 整数边际汇率过滤
  -> 整数种子金额过滤
  -> 带预算的粗搜与局部精搜
  -> 按跳数估算 Gas
  -> ProfitEngine 和 RiskGuard
  -> 带逐跳 minAmountOut 的 ExecutionRequest
  -> 本地 Shadow 模拟或原子 eth_call
  -> OpportunityRepository 和结构化日志
```

所有池储备、交易金额、成本、利润和 ROI 计算均使用 `U256`/`I256`。代币 decimals 只作为元数据使用，不会通过浮点数换算链上金额。

## 路由生成

路由在启动时沿图的邻接边预生成，不会对全部池做排列组合。路由必须满足以下条件：

- anchor 和中间代币在允许列表中。
- 所有跳都位于同一条链并且首尾连续。
- 最后一跳回到 anchor。
- 每个池都不相同。
- 中间代币不重复。
- 跳数严格等于 2 或 3。

RouteId 使用 `chain_id`、anchor 以及每一跳的池地址、输入代币和输出代币计算确定性 Keccak 哈希。同一 anchor 下的相同路径会合并，不同 anchor 的旋转路径会按配置保留。达到边数量或路由数量上限时，系统会记录裁剪数量并输出警告。

## 配置

可以从 [`dex-arbitrage.toml`](../dex-arbitrage.toml) 开始配置。V2 核心配置如下：

```toml
confirmation_mode = "latest"
# 公共 RPC 的 eth_getLogs 索引可能落后链头，按节点情况设置为 3 到 10。
log_query_delay_blocks = 0

[market_data]
# 从 RPC 的 eth_gasPrice 获取实时建议价。
use_realtime_gas_price = true
# 在实时建议价上增加 10%，防止下一块 Gas Price 上涨造成低估。
gas_price_buffer_bps = 1000

[market_data.native_price_pool]
# QuickSwap V2 WPOL/USDC，按指定区块的 getReserves 计算 POL/USDC 现货价格。
name = "quickswap_v2_wpol_usdc_oracle"
address = "0x6e7a5FAFcec6BB1e78bAe2A1F0B612012BF14827"
factory = "0x5757371414417b8C6CAad45bAeF941aBc7d3Ab32"
wrapped_native_address = "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270"
anchor_token = "USDC"
native_symbol = "POL"
native_is_token0 = true

[routes]
enable_two_hop = true
enable_three_hop = true
max_route_hops = 3
max_routes_total = 100000
max_routes_per_anchor = 20000
max_edges_per_token_pair = 10
allowed_anchor_tokens = ["USDC"]
allowed_intermediate_tokens = ["WETH", "USDT"]

[optimizer]
min_input = "1000000"
max_input = "10000000000"
seed_reserve_bps = [1, 3, 10, 30, 100]
max_quote_evaluations = 128
local_search_iterations = 24

[gas]
two_hop_fallback_gas = 180000
three_hop_fallback_gas = 260000
gas_units_buffer_bps = 1500

[risk]
max_leg_price_impact_bps = 100
max_total_price_impact_bps = 200
minimum_pool_liquidity = "1"
min_three_hop_net_profit = "1000000"
min_three_hop_roi_bps = 8

[execution]
max_steps = 3
default_leg_slippage_bps = 10
three_hop_leg_slippage_bps = 15
deadline_seconds = 30
```

所有代币和池都必须显式配置。当前不支持转账税代币、Rebase 代币、动态手续费池和未登记代币。

## 运行

替换示例中的 RPC、代币、Factory 和 Pair 地址，将 `enabled` 设置为 `true`，然后运行：

```powershell
cargo run -p pm-cli-app -- dex-arb dex-arbitrage.toml
```

`shadow` 是默认模式，不会发送交易。`simulate_only` 还需要配置 `execution.executor_address`，CLI 会编码完整原子调用，并执行 `eth_call` 和 `eth_estimateGas`。配置为 `live` 时，启动校验会直接拒绝运行。

启动时 CLI 会打印本次评估实际使用的 Gas Price、POL/USDC 价格及数据来源。开启上述实时配置后，Gas Price 来自 `eth_gasPrice`，POL 价格来自 WPOL/USDC Pair 在扫描区块的储备比。所有价格转换都使用整数运算。

储备比是该区块真实可观察的 AMM 现货价，但不具备 TWAP 或 Chainlink 预言机的抗操纵能力。V1 用它估算很小的 Gas 成本换汇；在开放真实交易前，应增加 TWAP/多源价格偏差校验。

Shadow 模式没有已部署执行合约，因此 Gas units 使用两跳/三跳基础估算并增加配置缓冲；这不是实际交易回执。只有 `simulate_only` 模式配置真实 `executor_address` 后，Gas units 才来自 `eth_estimateGas`。系统当前不会签名或广播交易，也不会真实消耗 Gas。

如果公共 RPC 返回 `eth_getLogs: invalid block range params`，说明节点的日志索引高度落后于它报告的链头高度。优先使用 `confirmation_mode = "finalized"`，并适当增大 `log_query_delay_blocks`；这只会让扫描落后少量区块，不影响 Shadow 模式的安全性。

## 原子执行合约

[`contracts/src/V2ArbitrageExecutor.sol`](../contracts/src/V2ArbitrageExecutor.sol) 只接受 2 或 3 个步骤，并校验执行者、代币和 Pair 白名单、路径连续性、Pair 不重复、截止时间、逐跳最小输出和最终最小利润。

合约不支持任意 target、任意 calldata 或 `delegatecall`。当前执行合约固定使用标准 `997/1000` 手续费公式。采用其他静态手续费的池可以参与 Shadow 分析，但在合约接口经过扩展和审计前，不得交给该执行合约运行。

## 验证命令

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy -p pm-arbitrage --all-targets --all-features --no-deps -- -D warnings
cargo test -p pm-arbitrage --all-features
cargo bench -p pm-arbitrage --bench dex_v2
```

安装 Foundry 后，在 `contracts/` 目录执行：

```powershell
forge fmt --check
forge build
forge test -vvv
```

## 当前限制

当前不支持 Uniswap V3 Tick、Curve、Balancer、四跳及以上路径、跨链套利、闪电贷、Mempool Backrun、Sandwich 和真实交易广播。

本地 Shadow 模式只验证确定性的路由数学。只有配置已部署执行合约的 `simulate_only` 模式会执行完整原子 EVM 模拟。
