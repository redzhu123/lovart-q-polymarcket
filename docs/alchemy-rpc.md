# Alchemy RPC 接入

扫描器将环境变量中的 Alchemy 完整 HTTP URL 作为主 RPC，原配置文件中的 PublicNode/公共
Solana RPC 始终保留为自动回退。没有设置环境变量时，运行行为与接入前一致。

## 环境变量

在 Alchemy 为 Polygon PoS、Base、Arbitrum One 和 Solana Mainnet 分别创建应用并复制 HTTP URL。
不要把 API Key 或完整 URL 提交到仓库。

```powershell
$env:ALCHEMY_POLYGON_RPC_URL="https://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY"
$env:ALCHEMY_BASE_RPC_URL="https://base-mainnet.g.alchemy.com/v2/YOUR_KEY"
$env:ALCHEMY_ARBITRUM_RPC_URL="https://arb-mainnet.g.alchemy.com/v2/YOUR_KEY"
$env:ALCHEMY_SOLANA_RPC_URL="https://solana-mainnet.g.alchemy.com/v2/YOUR_KEY"

cargo run -p pm-cli-app -- dex-multi multi-chain-arbitrage.toml
```

每个环境变量必须保存完整 URL，而不是只保存 API Key。程序依次尝试 Alchemy和原公共 RPC；
错误日志只记录 RPC 主机名，不输出 URL 路径中的 Key。

Solana的闭环报价仍来自 `https://transaction-v1.raydium.io`。Alchemy只负责 Solana slot/RPC，
不会改变或删除 Raydium Trade API 的原始报价方式。
