# Solana / Raydium 利润扫描

## 接入位置

统一入口仍为：

```powershell
cargo run -p pm-cli-app -- dex-multi multi-chain-arbitrage.toml
```

`MultiChainSupervisor` 启动时同时创建现有 EVM V2 runtime 和 Raydium runtime。每个定时轮次通过
`JoinSet` 并发执行所有 runtime；Raydium 的单条闭环之间再按 `max_concurrency` 有界并发。
Raydium 另有 `poll_interval_ms`（默认 5000ms），不会跟随 EVM 的 500ms 主 tick 持续请求；429、
上游 5xx 或临时非 JSON 响应会按 `quote_max_retries` 退避重试。

## 扫描模型

Raydium 不能复用 EVM 的 Factory / Pair ABI，因此使用独立适配器：

1. 从 Solana JSON-RPC 读取 `confirmed` slot，作为本轮观测标记；RPC 暂时不可用时 slot 记为 0，
   报价扫描仍继续。
2. 以配置中的唯一 anchor（默认 USDC）生成二跳与三跳闭环。
3. 每一腿调用 Raydium Trade API 的只读 `GET /compute/swap-base-in`。
4. 使用 `otherAmountThreshold`（而非乐观的 `outputAmount`）作为下一腿输入，逐腿累计滑点保护。
5. 最后一腿回到 anchor 后，扣除 `network_cost_anchor` 和 `risk_buffer_bps`，再检查
   `min_net_profit_anchor` 与 `min_roi_bps`。

Trade API 可以在单腿内部选择 Raydium 的 CPMM、AMM v4 或 CLMM 路径，日志会记录返回的真实 pool ID。
当前实现是只读 Shadow 扫描，不请求交易构建、不读取私钥、不签名也不广播。

## 配置

入口配置 `multi-chain-arbitrage.toml`：

```toml
[[solana_dexes]]
name = "solana-raydium"
config_path = "dex-solana-raydium.toml"
enabled = true
```

Raydium 独立配置见 `dex-solana-raydium.toml`。金额字段使用 anchor 的最小单位；默认 USDC 为 6 位
小数，例如 `input_amount = "1000000000"` 表示 1000 USDC。

## 边界

- Trade API 报价是短时有效的链下索引报价，不等同于已成交结果。
- 多腿闭环不是原子执行证明；真实执行前仍需构建一笔原子 Solana 交易并整体验证账户、CU、优先费和
  最小输出。
- `network_cost_anchor` 当前为保守配置值，不是实时优先费预言机。
- 该 runtime 暂不加入现有 EVM 跨链超级图；Solana 桥接资产映射和非 EVM 地址模型需要独立扩展。
