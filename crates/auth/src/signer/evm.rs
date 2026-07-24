//! EVM 签名器（接口预留，P2-06 第五节）。
//!
//! 为未来 EVM 链（Ethereum/Polygon/Arbitrum 等）预留。
//! 当前所有方法返回 Simulation Only 错误。

use anyhow::Result;
use async_trait::async_trait;

use super::{SignRequest, SignResponse, Signer, SignerHealth};

/// EVM 签名器（接口预留）。
///
/// 为未来 EVM 链签名预留接口。
/// 当前 Simulation Only —— 所有方法返回占位结果。
pub struct EvmSigner {
    chain_id: u64,
}

impl EvmSigner {
    /// 创建 EVM 签名器。
    pub fn new(chain_id: u64) -> Self {
        Self { chain_id }
    }

    /// 链 ID。
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
}

impl Default for EvmSigner {
    fn default() -> Self {
        Self::new(1) // Ethereum mainnet
    }
}

#[async_trait]
impl Signer for EvmSigner {
    fn algorithm(&self) -> &str {
        "ecdsa"
    }

    fn sign_type(&self) -> &str {
        "evm"
    }

    fn sign_request(&self, request: &SignRequest) -> Result<SignResponse> {
        tracing::debug!(
            chain_id = %self.chain_id,
            payload_len = request.payload.len(),
            "EvmSigner 接口预留，返回模拟签名"
        );
        Ok(SignResponse::new(vec![0u8; 65], "evm", "ecdsa"))
    }

    fn verify_signature(&self, _payload: &[u8], _signature: &[u8]) -> Result<bool> {
        tracing::debug!("EvmSigner 接口预留，模拟验证（始终返回 true）");
        Ok(true)
    }

    fn load_private_key(&mut self, _key_path: &str) -> Result<()> {
        tracing::warn!("EvmSigner 接口预留，未加载真实私钥");
        Ok(())
    }

    fn can_sign_real(&self) -> bool {
        false
    }

    fn health(&self) -> SignerHealth {
        SignerHealth {
            algorithm: "ecdsa".to_string(),
            sign_type: "evm".to_string(),
            can_sign: false,
            ready: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_signer_defaults() {
        let signer = EvmSigner::default();
        assert_eq!(signer.chain_id(), 1);
        assert_eq!(signer.algorithm(), "ecdsa");
        assert!(!signer.can_sign_real());
    }

    #[test]
    fn evm_signer_custom_chain() {
        let signer = EvmSigner::new(137);
        assert_eq!(signer.chain_id(), 137);
    }

    #[test]
    fn evm_signer_signs_simulated() {
        let signer = EvmSigner::new(137);
        let req = SignRequest::new(b"test".to_vec(), "evm");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.algorithm, "ecdsa");
    }
}
