//! Wallet Signer Trait（P2-06 第五节）。
//!
//! 统一钱包签名接口：
//! - sign_request()：签名请求
//! - verify_signature()：验证签名
//! - load_private_key()：加载私钥
//!
//! 实现：EvmSigner / Ed25519Signer

use anyhow::Result;
use async_trait::async_trait;

pub mod ed25519;
pub mod evm;

// ============================================================================
// WalletSignRequest / WalletSignResponse
// ============================================================================

/// 钱包签名请求。
#[derive(Debug, Clone)]
pub struct WalletSignRequest {
    /// 待签名数据。
    pub payload: Vec<u8>,
    /// 链 ID。
    pub chain_id: u64,
    /// 签名类型。
    pub sign_type: String,
}

impl WalletSignRequest {
    pub fn new(payload: Vec<u8>, chain_id: u64, sign_type: &str) -> Self {
        Self {
            payload,
            chain_id,
            sign_type: sign_type.to_string(),
        }
    }
}

/// 钱包签名响应。
#[derive(Debug, Clone)]
pub struct WalletSignResponse {
    pub signature: Vec<u8>,
    pub signature_hex: String,
    pub algorithm: String,
}

impl WalletSignResponse {
    pub fn new(signature: Vec<u8>, algorithm: &str) -> Self {
        let signature_hex: String = signature.iter().map(|b| format!("{:02x}", b)).collect();
        Self {
            signature,
            signature_hex,
            algorithm: algorithm.to_string(),
        }
    }
}

// ============================================================================
// WalletSigner Trait
// ============================================================================

/// 钱包签名接口（P2-06 第五节）。
#[async_trait]
pub trait WalletSigner: Send + Sync {
    /// 签名算法。
    fn algorithm(&self) -> &str;

    /// 签名请求。
    fn sign_request(&self, request: &WalletSignRequest) -> Result<WalletSignResponse>;

    /// 验证签名。
    fn verify_signature(&self, payload: &[u8], signature: &[u8]) -> Result<bool>;

    /// 加载私钥（接口预留）。
    fn load_private_key(&mut self) -> Result<()>;

    /// 是否可用真实签名。
    fn can_sign_real(&self) -> bool {
        false
    }

    /// 健康检查。
    fn health(&self) -> WalletSignerHealth {
        WalletSignerHealth {
            algorithm: self.algorithm().to_string(),
            ready: true,
            can_sign: self.can_sign_real(),
        }
    }
}

// ============================================================================
// WalletSignerHealth
// ============================================================================

#[derive(Debug, Clone)]
pub struct WalletSignerHealth {
    pub algorithm: String,
    pub ready: bool,
    pub can_sign: bool,
}

impl WalletSignerHealth {
    pub fn summary_zh(&self) -> String {
        format!(
            "算法: {} | 就绪: {} | 可签名: {}",
            self.algorithm,
            if self.ready { "✅" } else { "❌" },
            if self.can_sign {
                "⚠️ 是"
            } else {
                "🔒 否（模拟）"
            },
        )
    }
}

// ============================================================================
// NoopWalletSigner
// ============================================================================

/// 空钱包签名器（Mock 模式）。
pub struct NoopWalletSigner;

impl NoopWalletSigner {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoopWalletSigner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WalletSigner for NoopWalletSigner {
    fn algorithm(&self) -> &str {
        "noop"
    }

    fn sign_request(&self, _request: &WalletSignRequest) -> Result<WalletSignResponse> {
        tracing::debug!("NoopWalletSigner: 模拟签名");
        Ok(WalletSignResponse::new(vec![0u8; 32], "noop"))
    }

    fn verify_signature(&self, _payload: &[u8], _signature: &[u8]) -> Result<bool> {
        Ok(true)
    }

    fn load_private_key(&mut self) -> Result<()> {
        tracing::warn!("NoopWalletSigner: 接口预留");
        Ok(())
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_sign_request_creation() {
        let req = WalletSignRequest::new(b"data".to_vec(), 137, "evm");
        assert_eq!(req.chain_id, 137);
        assert_eq!(req.sign_type, "evm");
    }

    #[test]
    fn wallet_sign_response_hex() {
        let resp = WalletSignResponse::new(vec![0xab, 0xcd], "ecdsa");
        assert_eq!(resp.signature_hex, "abcd");
    }

    #[test]
    fn noop_signer_works() {
        let signer = NoopWalletSigner::new();
        assert_eq!(signer.algorithm(), "noop");
        assert!(!signer.can_sign_real());

        let req = WalletSignRequest::new(b"test".to_vec(), 137, "evm");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.signature.len(), 32);
    }

    #[test]
    fn wallet_signer_health() {
        let signer = NoopWalletSigner::new();
        let health = signer.health();
        assert!(health.ready);
        assert!(!health.can_sign);
        assert!(health.summary_zh().contains("模拟"));
    }
}
