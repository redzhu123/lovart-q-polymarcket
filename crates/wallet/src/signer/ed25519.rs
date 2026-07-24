//! Ed25519 签名器（接口预留，P2-06 第五节）。
//!
//! 为 Solana/Aptos 等 Ed25519 链预留。
//! 当前 Simulation Only。

use anyhow::Result;
use async_trait::async_trait;

use super::{WalletSignRequest, WalletSignResponse, WalletSigner};

/// Ed25519 签名器（接口预留）。
pub struct Ed25519Signer;

impl Ed25519Signer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Ed25519Signer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WalletSigner for Ed25519Signer {
    fn algorithm(&self) -> &str {
        "ed25519"
    }

    fn sign_request(&self, request: &WalletSignRequest) -> Result<WalletSignResponse> {
        tracing::debug!(
            payload_len = request.payload.len(),
            "Ed25519Signer 接口预留，返回模拟签名"
        );
        Ok(WalletSignResponse::new(vec![0u8; 64], "ed25519"))
    }

    fn verify_signature(&self, _payload: &[u8], _signature: &[u8]) -> Result<bool> {
        Ok(true)
    }

    fn load_private_key(&mut self) -> Result<()> {
        tracing::warn!("Ed25519Signer 接口预留，未加载真实私钥");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ed25519_signer_defaults() {
        let signer = Ed25519Signer::new();
        assert_eq!(signer.algorithm(), "ed25519");
        assert!(!signer.can_sign_real());
    }

    #[test]
    fn ed25519_signer_signs_simulated() {
        let signer = Ed25519Signer::new();
        let req = WalletSignRequest::new(b"test".to_vec(), 0, "ed25519");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.signature.len(), 64);
    }
}
