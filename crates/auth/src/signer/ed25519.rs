//! Ed25519 签名器（接口预留，P2-06 第五节）。
//!
//! 为未来 Ed25519 链（Solana/Aptos/Sui 等）预留。
//! 当前所有方法返回 Simulation Only 错误。

use anyhow::Result;
use async_trait::async_trait;

use super::{SignRequest, SignResponse, Signer, SignerHealth};

/// Ed25519 签名器（接口预留）。
///
/// 为未来 Ed25519 链签名预留接口。
/// 当前 Simulation Only —— 所有方法返回占位结果。
pub struct Ed25519Signer;

impl Ed25519Signer {
    /// 创建 Ed25519 签名器。
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
impl Signer for Ed25519Signer {
    fn algorithm(&self) -> &str {
        "ed25519"
    }

    fn sign_type(&self) -> &str {
        "ed25519"
    }

    fn sign_request(&self, request: &SignRequest) -> Result<SignResponse> {
        tracing::debug!(
            payload_len = request.payload.len(),
            "Ed25519Signer 接口预留，返回模拟签名"
        );
        Ok(SignResponse::new(vec![0u8; 64], "ed25519", "ed25519"))
    }

    fn verify_signature(&self, _payload: &[u8], _signature: &[u8]) -> Result<bool> {
        tracing::debug!("Ed25519Signer 接口预留，模拟验证（始终返回 true）");
        Ok(true)
    }

    fn load_private_key(&mut self, _key_path: &str) -> Result<()> {
        tracing::warn!("Ed25519Signer 接口预留，未加载真实私钥");
        Ok(())
    }

    fn can_sign_real(&self) -> bool {
        false
    }

    fn health(&self) -> SignerHealth {
        SignerHealth {
            algorithm: "ed25519".to_string(),
            sign_type: "ed25519".to_string(),
            can_sign: false,
            ready: true,
        }
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
        let req = SignRequest::new(b"test".to_vec(), "ed25519");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.algorithm, "ed25519");
        assert_eq!(resp.signature.len(), 64);
    }
}
