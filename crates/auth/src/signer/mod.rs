//! 统一签名接口（P2-06 第五节）。
//!
//! Signer Trait 提供统一的签名/验证接口。
//! 支持：Polymarket (EIP-712) / EVM / Ed25519 / 未来链签名。

use anyhow::Result;
use async_trait::async_trait;

pub mod ed25519;
pub mod evm;
pub mod polymarket;

// ============================================================================
// SignRequest / SignResponse — 签名输入/输出
// ============================================================================

/// 签名请求。
#[derive(Debug, Clone)]
pub struct SignRequest {
    /// 待签名数据（字节）。
    pub payload: Vec<u8>,
    /// 签名类型（如 "eip712", "ed25519", "evm"）。
    pub sign_type: String,
    /// 链 ID。
    pub chain_id: Option<u64>,
    /// 额外元数据。
    pub metadata: std::collections::HashMap<String, String>,
}

impl SignRequest {
    /// 创建签名请求。
    pub fn new(payload: Vec<u8>, sign_type: &str) -> Self {
        Self {
            payload,
            sign_type: sign_type.to_string(),
            chain_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// 设置链 ID。
    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = Some(chain_id);
        self
    }
}

/// 签名响应。
#[derive(Debug, Clone)]
pub struct SignResponse {
    /// 签名结果（字节）。
    pub signature: Vec<u8>,
    /// 签名十六进制表示。
    pub signature_hex: String,
    /// 签名类型。
    pub sign_type: String,
    /// 签名算法。
    pub algorithm: String,
}

impl SignResponse {
    /// 创建签名响应。
    pub fn new(signature: Vec<u8>, sign_type: &str, algorithm: &str) -> Self {
        let signature_hex = hex::encode(&signature);
        Self {
            signature,
            signature_hex,
            sign_type: sign_type.to_string(),
            algorithm: algorithm.to_string(),
        }
    }
}

// ============================================================================
// Signer Trait
// ============================================================================

/// 统一签名接口（P2-06 第五节）。
///
/// 所有签名实现必须实现此 trait。
/// 支持同步签名（sign_request）和异步签名（sign_request_async）。
#[async_trait]
pub trait Signer: Send + Sync {
    /// 签名算法名称。
    fn algorithm(&self) -> &str;

    /// 签名类型（如 "eip712", "ed25519"）。
    fn sign_type(&self) -> &str;

    /// 同步签名。
    fn sign_request(&self, request: &SignRequest) -> Result<SignResponse>;

    /// 验证签名。
    fn verify_signature(&self, payload: &[u8], signature: &[u8]) -> Result<bool>;

    /// 加载私钥（接口预留，当前仅日志）。
    fn load_private_key(&mut self, _key_path: &str) -> Result<()> {
        tracing::warn!("load_private_key 接口预留，未实现");
        Ok(())
    }

    /// 是否可用于真实签名。
    fn can_sign_real(&self) -> bool {
        false
    }

    /// 健康检查。
    fn health(&self) -> SignerHealth {
        SignerHealth {
            algorithm: self.algorithm().to_string(),
            sign_type: self.sign_type().to_string(),
            can_sign: self.can_sign_real(),
            ready: true,
        }
    }
}

// ============================================================================
// SignerHealth
// ============================================================================

/// 签名器健康状态。
#[derive(Debug, Clone)]
pub struct SignerHealth {
    pub algorithm: String,
    pub sign_type: String,
    pub can_sign: bool,
    pub ready: bool,
}

impl SignerHealth {
    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "算法: {} | 类型: {} | 可签名: {} | 就绪: {}",
            self.algorithm,
            self.sign_type,
            if self.can_sign {
                "✅ 是"
            } else {
                "❌ 否（模拟）"
            },
            if self.ready { "✅ 是" } else { "❌ 否" },
        )
    }
}

// ============================================================================
// NoopSigner — 空签名器（Mock 模式）
// ============================================================================

/// 空签名器（Mock / 测试模式）。
pub struct NoopSigner;

impl NoopSigner {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoopSigner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Signer for NoopSigner {
    fn algorithm(&self) -> &str {
        "noop"
    }

    fn sign_type(&self) -> &str {
        "noop"
    }

    fn sign_request(&self, _request: &SignRequest) -> Result<SignResponse> {
        tracing::debug!("NoopSigner: 模拟签名请求");
        Ok(SignResponse::new(vec![0u8; 32], "noop", "noop"))
    }

    fn verify_signature(&self, _payload: &[u8], _signature: &[u8]) -> Result<bool> {
        tracing::debug!("NoopSigner: 模拟签名验证（始终返回 true）");
        Ok(true)
    }

    fn can_sign_real(&self) -> bool {
        false
    }
}

// ============================================================================
// hex 辅助（无外部依赖）
// ============================================================================

mod hex {
    /// 字节数组转十六进制字符串。
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_request_creation() {
        let req = SignRequest::new(b"hello".to_vec(), "eip712");
        assert_eq!(req.sign_type, "eip712");
        assert_eq!(req.payload, b"hello");
        assert!(req.chain_id.is_none());
    }

    #[test]
    fn sign_request_with_chain_id() {
        let req = SignRequest::new(b"test".to_vec(), "evm").with_chain_id(137);
        assert_eq!(req.chain_id, Some(137));
    }

    #[test]
    fn sign_response_hex_encoding() {
        let resp = SignResponse::new(vec![0xab, 0xcd, 0xef], "eip712", "secp256k1");
        assert_eq!(resp.signature_hex, "abcdef");
        assert_eq!(resp.algorithm, "secp256k1");
    }

    #[test]
    fn noop_signer_basics() {
        let signer = NoopSigner::new();
        assert_eq!(signer.algorithm(), "noop");
        assert_eq!(signer.sign_type(), "noop");
        assert!(!signer.can_sign_real());
    }

    #[test]
    fn noop_signer_signs() {
        let signer = NoopSigner::new();
        let req = SignRequest::new(b"data".to_vec(), "test");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.algorithm, "noop");
        assert_eq!(resp.signature.len(), 32);
    }

    #[test]
    fn noop_signer_verifies() {
        let signer = NoopSigner::new();
        assert!(signer.verify_signature(b"data", &[0u8; 32]).unwrap());
    }

    #[test]
    fn signer_health_summary_chinese() {
        let health = SignerHealth {
            algorithm: "secp256k1".into(),
            sign_type: "eip712".into(),
            can_sign: false,
            ready: true,
        };
        let summary = health.summary_zh();
        assert!(summary.contains("算法"));
        assert!(summary.contains("模拟"));
    }

    #[test]
    fn signer_trait_object_safe() {
        let signer: Box<dyn Signer> = Box::new(NoopSigner::new());
        assert_eq!(signer.algorithm(), "noop");
    }
}
