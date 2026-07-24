//! EVM 签名器（接口预留，P2-06 第五节）。
//!
//! 为 EVM 链交易签名预留。
//! 当前 Simulation Only。

use anyhow::Result;
use async_trait::async_trait;

use super::{WalletSignRequest, WalletSignResponse, WalletSigner};

/// EVM 签名器（接口预留）。
pub struct EvmSigner {
    chain_id: u64,
}

impl EvmSigner {
    pub fn new(chain_id: u64) -> Self {
        Self { chain_id }
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
}

impl Default for EvmSigner {
    fn default() -> Self {
        Self::new(137)
    }
}

#[async_trait]
impl WalletSigner for EvmSigner {
    fn algorithm(&self) -> &str {
        "ecdsa"
    }

    fn sign_request(&self, request: &WalletSignRequest) -> Result<WalletSignResponse> {
        tracing::debug!(
            chain_id = %self.chain_id,
            payload_len = request.payload.len(),
            "EvmSigner 接口预留，返回模拟签名"
        );
        Ok(WalletSignResponse::new(vec![0u8; 65], "ecdsa"))
    }

    fn verify_signature(&self, _payload: &[u8], _signature: &[u8]) -> Result<bool> {
        Ok(true)
    }

    fn load_private_key(&mut self) -> Result<()> {
        tracing::warn!("EvmSigner 接口预留，未加载真实私钥");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_signer_defaults() {
        let signer = EvmSigner::default();
        assert_eq!(signer.chain_id(), 137);
        assert_eq!(signer.algorithm(), "ecdsa");
        assert!(!signer.can_sign_real());
    }

    #[test]
    fn evm_signer_signs_simulated() {
        let signer = EvmSigner::new(1);
        let req = WalletSignRequest::new(b"test".to_vec(), 1, "evm");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.algorithm, "ecdsa");
    }
}
