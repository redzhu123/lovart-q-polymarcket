//! Polymarket 签名器（P2-06 第五节）。
//!
//! EIP-712 类型化数据签名 + HMAC-SHA256 L2 签名。
//! Simulation Only — 所有签名均为模拟，禁止真实签名。
//!
//! 业务约束：
//! - 禁止真实私钥签名
//! - 所有签名输出均为模拟占位
//! - 日志自动脱敏

use anyhow::Result;
use async_trait::async_trait;
use chrono::Local;

use super::{SignRequest, SignResponse, Signer, SignerHealth};

// ============================================================================
// PolymarketSigner
// ============================================================================

/// Polymarket 签名器（模拟模式）。
///
/// 实现 EIP-712 类型化数据签名 + HMAC-SHA256 L2 认证。
/// 当前为 Simulation Only —— 不加载真实私钥，不产生真实签名。
pub struct PolymarketSigner {
    /// 签名器名称。
    name: String,
    /// 链 ID（默认 137 = Polygon）。
    chain_id: u64,
    /// 是否启用真实签名（始终为 false）。
    live_enabled: bool,
    /// 模拟私钥（脱敏显示，为空表示未加载）。
    private_key_loaded: bool,
}

impl PolymarketSigner {
    /// 创建模拟 Polymarket 签名器。
    pub fn new() -> Self {
        Self {
            name: "PolymarketSigner".to_string(),
            chain_id: 137,
            live_enabled: false,
            private_key_loaded: false,
        }
    }

    /// 设置链 ID。
    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }

    /// 链 ID。
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// 是否启用真实签名。
    pub fn live_enabled(&self) -> bool {
        self.live_enabled
    }

    /// 安全摘要（中文，脱敏）。
    pub fn safe_summary(&self) -> String {
        format!(
            "签名器: {} | 链ID: {} | 真实签名: {} | 私钥: {}",
            self.name,
            self.chain_id,
            if self.live_enabled {
                "⚠️ 是"
            } else {
                "🔒 否（模拟）"
            },
            if self.private_key_loaded {
                "[PRIVATE_KEY]"
            } else {
                "无"
            },
        )
    }
}

impl Default for PolymarketSigner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Signer for PolymarketSigner {
    fn algorithm(&self) -> &str {
        "secp256k1"
    }

    fn sign_type(&self) -> &str {
        "eip712"
    }

    fn sign_request(&self, request: &SignRequest) -> Result<SignResponse> {
        if self.live_enabled {
            anyhow::bail!("真实签名未实现（Simulation Only）");
        }

        tracing::debug!(
            sign_type = %request.sign_type,
            payload_len = request.payload.len(),
            chain_id = %self.chain_id,
            "PolymarketSigner 模拟签名请求"
        );

        // 模拟签名：返回确定性占位字节
        let mut signature = vec![0u8; 65]; // ECDSA 签名: r(32) + s(32) + v(1)
        // 注入 chain_id 和时间戳使签名可区分
        signature[0..8].copy_from_slice(&self.chain_id.to_be_bytes());
        let now = Local::now();
        signature[8..16].copy_from_slice(&now.timestamp().to_be_bytes());
        signature[64] = 27; // v (模拟 recovery id)

        Ok(SignResponse::new(signature, "eip712", "secp256k1"))
    }

    fn verify_signature(&self, _payload: &[u8], _signature: &[u8]) -> Result<bool> {
        if self.live_enabled {
            anyhow::bail!("真实签名验证未实现（Simulation Only）");
        }

        tracing::debug!("PolymarketSigner 模拟签名验证（始终返回 true）");
        Ok(true)
    }

    fn load_private_key(&mut self, _key_path: &str) -> Result<()> {
        tracing::warn!(
            key_path = %_key_path,
            "私钥加载接口预留，当前为模拟模式，未加载真实私钥"
        );
        self.private_key_loaded = false;
        Ok(())
    }

    fn can_sign_real(&self) -> bool {
        false
    }

    fn health(&self) -> SignerHealth {
        SignerHealth {
            algorithm: self.algorithm().to_string(),
            sign_type: self.sign_type().to_string(),
            can_sign: false,
            ready: true,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polymarket_signer_defaults() {
        let signer = PolymarketSigner::new();
        assert_eq!(signer.algorithm(), "secp256k1");
        assert_eq!(signer.sign_type(), "eip712");
        assert_eq!(signer.chain_id(), 137);
        assert!(!signer.live_enabled());
        assert!(!signer.can_sign_real());
    }

    #[test]
    fn polymarket_signer_with_chain_id() {
        let signer = PolymarketSigner::new().with_chain_id(80001);
        assert_eq!(signer.chain_id(), 80001);
    }

    #[test]
    fn polymarket_signer_signs_simulated() {
        let signer = PolymarketSigner::new();
        let req = SignRequest::new(b"test-data".to_vec(), "eip712");
        let resp = signer.sign_request(&req).unwrap();
        assert_eq!(resp.algorithm, "secp256k1");
        assert_eq!(resp.sign_type, "eip712");
        assert_eq!(resp.signature.len(), 65);
    }

    #[test]
    fn polymarket_signer_verifies_simulated() {
        let signer = PolymarketSigner::new();
        assert!(signer.verify_signature(b"data", &[0u8; 65]).unwrap());
    }

    #[test]
    fn polymarket_signer_load_private_key_is_noop() {
        let mut signer = PolymarketSigner::new();
        assert!(signer.load_private_key("/fake/path").is_ok());
        assert!(!signer.private_key_loaded);
    }

    #[test]
    fn polymarket_signer_safe_summary_chinese() {
        let signer = PolymarketSigner::new();
        let summary = signer.safe_summary();
        assert!(summary.contains("PolymarketSigner"));
        assert!(summary.contains("模拟"));
    }

    #[test]
    fn polymarket_signer_health() {
        let signer = PolymarketSigner::new();
        let health = signer.health();
        assert!(health.ready);
        assert!(!health.can_sign);
    }
}
