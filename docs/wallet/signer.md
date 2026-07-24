# Signer (P2-06)

## WalletSigner Trait

```rust
#[async_trait]
pub trait WalletSigner: Send + Sync {
    fn algorithm(&self) -> &str;
    fn sign_request(&self, request: &WalletSignRequest) -> Result<WalletSignResponse>;
    fn verify_signature(&self, payload: &[u8], signature: &[u8]) -> Result<bool>;
    fn load_private_key(&mut self) -> Result<()>;
    fn can_sign_real(&self) -> bool;
    fn health(&self) -> WalletSignerHealth;
}
```

## 实现

### EvmSigner

EVM 链（Ethereum/Polygon/Arbitrum）签名器：
- 算法：ECDSA (secp256k1)
- Simulation Only：返回模拟占位签名
- 支持自定义链 ID

### Ed25519Signer

Ed25519 链（Solana/Aptos/Sui）签名器：
- 算法：Ed25519
- Simulation Only：返回模拟占位签名
- 接口预留

### NoopWalletSigner

空签名器（Mock 模式）：
- 算法：noop
- 始终返回成功
- 测试/演示用

## 安全

- 所有实现为 Simulation Only
- `can_sign_real()` 始终返回 `false`
- 私钥加载接口预留但未实现
- 签名输出为确定性占位字节（非随机）
