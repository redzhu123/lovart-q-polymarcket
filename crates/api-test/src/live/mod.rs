//! Live 测试模块（V1.08）。
//!
//! 所有需要真实网络的测试。
//! - REST API Live Tests（只读）
//! - WebSocket 测试
//! - DryRun 订单测试（构建→验证→打印，不发送）
//! - Live 订单测试（需 enable_live=true）
//! - RateLimit 测试
//! - 健康检查聚合

pub mod health_check;
pub mod order_dryrun;
pub mod order_live;
pub mod ratelimit;
pub mod rest;
pub mod ws;

/// Live 测试安全门。
///
/// 确保写操作只在 `enable_live=true` 时才执行。
pub struct LiveGuard {
    pub enable_live: bool,
}

impl LiveGuard {
    pub fn new(enable_live: bool) -> Self {
        Self { enable_live }
    }

    /// 检查是否允许写操作。
    pub fn guard_write(&self, operation: &str) -> Result<(), String> {
        if !self.enable_live {
            return Err(format!(
                "🔒 真实交易未启用 — 已阻止操作: '{}'。设置 enable_live=true 以允许。",
                operation
            ));
        }
        tracing::warn!("⚠️ 执行真实操作: {}", operation);
        Ok(())
    }

    /// 是否允许真实交易。
    pub fn is_live(&self) -> bool {
        self.enable_live
    }
}
