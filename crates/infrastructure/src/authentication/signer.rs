//! 签名器模块。
//!
//! 从 `pm-auth::signer` 提取并统一。

/// 签名请求
#[derive(Debug, Clone)]
pub struct SignRequest {
    /// 待签名数据
    pub payload: Vec<u8>,
    /// 签名类型（"eip712", "evm", "ed25519"）
    pub sign_type: String,
}

/// 签名响应
#[derive(Debug, Clone)]
pub struct SignResponse {
    /// 签名结果
    pub signature: Vec<u8>,
    /// 签名类型
    pub sign_type: String,
}

/// 签名器 trait
///
/// 支持多种签名算法（EVM、Ed25519 等），未来可扩展。
pub trait Signer: Send + Sync {
    /// 签名算法名称
    fn algorithm(&self) -> &str;

    /// 是否可以执行真实签名
    fn can_sign_real(&self) -> bool;

    /// 执行签名
    fn sign_request(&self, req: &SignRequest) -> anyhow::Result<SignResponse>;
}

/// 空签名器（Dry Run / 测试用）
pub struct NoopSigner;

impl Signer for NoopSigner {
    fn algorithm(&self) -> &str {
        "noop"
    }

    fn can_sign_real(&self) -> bool {
        false
    }

    fn sign_request(&self, req: &SignRequest) -> anyhow::Result<SignResponse> {
        tracing::debug!("NoopSigner: 跳过签名 (type={})", req.sign_type);
        Ok(SignResponse {
            signature: vec![],
            sign_type: req.sign_type.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_signer_returns_empty_signature() {
        let signer = NoopSigner;
        assert!(!signer.can_sign_real());
        assert_eq!(signer.algorithm(), "noop");

        let req = SignRequest {
            payload: b"test-data".to_vec(),
            sign_type: "evm".to_string(),
        };
        let resp = signer.sign_request(&req).unwrap();
        assert!(resp.signature.is_empty());
        assert_eq!(resp.sign_type, "evm");
    }
}
