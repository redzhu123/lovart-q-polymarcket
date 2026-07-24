//! Workflow 配置（P2-02）。
//!
//! 独立配置文件 `workflow.toml`（仿 V1.07 `provider.toml` 范式），
//! 不侵入 `pm-models::Config` / `config.toml`，零风险。
//!
//! # 安全
//!
//! 默认 DryRun；真实读取需 `enable_live_reads=true` 且 mode=LiveReadOnly。
//! 任何模式下都禁止真实下单。

use serde::{Deserialize, Serialize};

use pm_api_test::client::config::{ApiTestConfig, ClientMode};

// ============================================================================
// WorkflowMode
// ============================================================================

/// Workflow 运行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WorkflowMode {
    /// DryRun：Mock 模式，全生命周期，下单步骤仅构建+校验，不发送。
    DryRun,
    /// Replay：从 fixtures/ 读取 Mock 数据，确定性完整回放，不访问网络。
    Replay,
    /// Live ReadOnly：真实接口，仅读取，禁止下单/撤单。
    LiveReadOnly,
}

// 接受 TOML 中的友好小写形式：dryrun / replay / live / readonly / live_readonly。
impl<'de> Deserialize<'de> for WorkflowMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(WorkflowMode::from_str(&s))
    }
}

impl WorkflowMode {
    /// 中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            WorkflowMode::DryRun => "DryRun（模拟）",
            WorkflowMode::Replay => "Replay（回放）",
            WorkflowMode::LiveReadOnly => "Live ReadOnly（真实只读）",
        }
    }

    /// 是否访问真实网络。
    pub fn is_network(&self) -> bool {
        matches!(self, WorkflowMode::LiveReadOnly)
    }

    /// 从字符串解析模式。
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "replay" => WorkflowMode::Replay,
            "live" | "live_readonly" | "readonly" => WorkflowMode::LiveReadOnly,
            _ => WorkflowMode::DryRun,
        }
    }

    /// CLI 令牌。
    pub fn as_token(&self) -> &'static str {
        match self {
            WorkflowMode::DryRun => "dryrun",
            WorkflowMode::Replay => "replay",
            WorkflowMode::LiveReadOnly => "live",
        }
    }
}

impl Default for WorkflowMode {
    fn default() -> Self {
        WorkflowMode::DryRun
    }
}

impl std::fmt::Display for WorkflowMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_zh())
    }
}

// ============================================================================
// WorkflowConfig
// ============================================================================

/// Workflow 配置。
///
/// 全部字段可配置，禁止写死。从 `workflow.toml` 的 `[workflow]` 段读取。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// 运行模式。
    #[serde(default = "default_mode")]
    pub mode: WorkflowMode,

    /// fixtures 目录（Mock 数据源，单一共享，禁止重复）。
    #[serde(default = "default_fixtures_dir")]
    pub fixtures_dir: String,

    /// 报告输出目录。
    #[serde(default = "default_report_dir")]
    pub report_dir: String,

    /// 目标市场 condition_id。
    #[serde(default = "default_market_id")]
    pub target_market_id: String,

    /// 目标 token_id（订单簿 / 下单标的）。
    #[serde(default = "default_token_id")]
    pub target_token_id: String,

    /// 订单方向（BUY / SELL）。
    #[serde(default = "default_order_side")]
    pub order_side: String,

    /// 订单价格（0.0~1.0）。
    #[serde(default = "default_order_price")]
    pub order_price: f64,

    /// 订单数量。
    #[serde(default = "default_order_size")]
    pub order_size: f64,

    /// 是否允许真实读取（LiveReadOnly 用）。
    #[serde(default)]
    pub enable_live_reads: bool,
}

// ---- 默认值函数 ----

fn default_mode() -> WorkflowMode {
    WorkflowMode::DryRun
}

fn default_fixtures_dir() -> String {
    // 工作区顶层共享 fixtures/（本 crate 目录上溯两级）。
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures").to_string()
}

fn default_report_dir() -> String {
    "reports/workflow".to_string()
}

fn default_market_id() -> String {
    "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string()
}

fn default_token_id() -> String {
    "1111111111111111111111111111111111111111111111111111111111111111".to_string()
}

fn default_order_side() -> String {
    "BUY".to_string()
}

fn default_order_price() -> f64 {
    0.45
}

fn default_order_size() -> f64 {
    100.0
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            fixtures_dir: default_fixtures_dir(),
            report_dir: default_report_dir(),
            target_market_id: default_market_id(),
            target_token_id: default_token_id(),
            order_side: default_order_side(),
            order_price: default_order_price(),
            order_size: default_order_size(),
            enable_live_reads: false,
        }
    }
}

impl WorkflowConfig {
    /// 从 `workflow.toml` 加载配置；文件缺失或字段缺失时使用安全默认。
    pub fn load_or_default(path: &str) -> Self {
        // 用包装结构反序列化整份文件；`#[serde(default)]` 让缺失的 [workflow] 段退化为默认值，
        // 字段级 `#[serde(default = "fn")]` 让段内缺失字段也退化。
        #[derive(Deserialize)]
        struct WorkflowFile {
            #[serde(default)]
            workflow: WorkflowConfig,
        }

        match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<WorkflowFile>(&content) {
                Ok(f) => {
                    tracing::info!(path, mode = %f.workflow.mode.as_zh(), "Workflow 配置已加载");
                    f.workflow
                }
                Err(e) => {
                    tracing::warn!(path, error = %e, "workflow.toml 解析失败，使用默认值");
                    Self::default()
                }
            },
            Err(_) => {
                tracing::debug!(path, "workflow.toml 不存在，使用默认 Workflow 配置");
                Self::default()
            }
        }
    }

    /// 根据模式构造 pm-api-test 的 ApiTestConfig。
    ///
    /// - DryRun / Replay：Mock 模式，mock_dir 指向 fixtures/。
    /// - LiveReadOnly：`enable_live_reads=true` 时使用真实网络（仅 GET），
    ///   否则默认 Mock（安全、可离线运行校验逻辑）。任何情况下 `enable_live=false`（永不真实下单）。
    pub fn to_api_test_config(&self) -> ApiTestConfig {
        match self.mode {
            WorkflowMode::DryRun | WorkflowMode::Replay => ApiTestConfig {
                mode: ClientMode::Mock,
                mock_dir: self.fixtures_dir.clone(),
                ..ApiTestConfig::default()
            },
            WorkflowMode::LiveReadOnly => {
                let mut c = if self.enable_live_reads {
                    ApiTestConfig::live()
                } else {
                    // 默认安全：Mock 客户端，便于离线校验只读约束
                    ApiTestConfig {
                        mode: ClientMode::Mock,
                        mock_dir: self.fixtures_dir.clone(),
                        ..ApiTestConfig::default()
                    }
                };
                c.enable_live = false; // 永不开启真实下单
                c
            }
        }
    }

    /// 安全摘要（中文）。
    pub fn safety_summary_zh(&self) -> String {
        let live_str = if self.enable_live_reads {
            "⚠️ 真实读取已启用（仅 GET）"
        } else {
            "🔒 DryRun 模式"
        };
        format!(
            "【Workflow 配置】\n\
             模式: {}\n\
             {}\n\
             fixtures 目录: {}\n\
             报告目录: {}\n\
             目标市场: {}\n\
             目标 Token: {}\n\
             订单: {} {:.4} x {:.2}",
            self.mode.as_zh(),
            live_str,
            self.fixtures_dir,
            self.report_dir,
            self.target_market_id,
            self.target_token_id,
            self.order_side,
            self.order_price,
            self.order_size,
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_dryrun() {
        let cfg = WorkflowConfig::default();
        assert_eq!(cfg.mode, WorkflowMode::DryRun);
        assert!(!cfg.enable_live_reads);
    }

    #[test]
    fn mode_from_str() {
        assert_eq!(WorkflowMode::from_str("replay"), WorkflowMode::Replay);
        assert_eq!(WorkflowMode::from_str("live"), WorkflowMode::LiveReadOnly);
        assert_eq!(WorkflowMode::from_str("dryrun"), WorkflowMode::DryRun);
        assert_eq!(WorkflowMode::from_str("unknown"), WorkflowMode::DryRun);
    }

    #[test]
    fn dryrun_uses_mock_client() {
        let cfg = WorkflowConfig::default();
        let api = cfg.to_api_test_config();
        assert!(!api.is_live_enabled());
        assert_eq!(cfg.fixtures_dir, api.mock_dir);
    }

    #[test]
    fn live_readonly_never_enables_orders() {
        let cfg = WorkflowConfig {
            mode: WorkflowMode::LiveReadOnly,
            enable_live_reads: true,
            ..WorkflowConfig::default()
        };
        let api = cfg.to_api_test_config();
        assert!(!api.enable_live); // 永不开启真实下单
    }

    #[test]
    fn live_readonly_defaults_to_mock_client() {
        // 默认 enable_live_reads=false -> Mock 客户端（可离线运行）
        let cfg = WorkflowConfig {
            mode: WorkflowMode::LiveReadOnly,
            ..WorkflowConfig::default()
        };
        let api = cfg.to_api_test_config();
        assert_eq!(api.mode, ClientMode::Mock);
        assert!(!api.enable_live);
    }

    #[test]
    fn live_readonly_with_reads_uses_live_client() {
        let cfg = WorkflowConfig {
            mode: WorkflowMode::LiveReadOnly,
            enable_live_reads: true,
            ..WorkflowConfig::default()
        };
        let api = cfg.to_api_test_config();
        assert_eq!(api.mode, ClientMode::Live);
    }

    #[test]
    fn load_missing_file_uses_defaults() {
        let cfg = WorkflowConfig::load_or_default("definitely-nonexistent-workflow.toml");
        assert_eq!(cfg.mode, WorkflowMode::DryRun);
    }

    #[test]
    fn load_lowercase_mode_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflow.toml");
        std::fs::write(
            &path,
            "[workflow]\nmode = \"replay\"\norder_price = 0.55\n",
        )
        .unwrap();
        let cfg = WorkflowConfig::load_or_default(path.to_str().unwrap());
        assert_eq!(cfg.mode, WorkflowMode::Replay);
        assert!((cfg.order_price - 0.55).abs() < 1e-9);
        // 未设置的字段使用默认值
        assert!(!cfg.enable_live_reads);
    }

    #[test]
    fn safety_summary_is_chinese() {
        let cfg = WorkflowConfig::default();
        let s = cfg.safety_summary_zh();
        assert!(s.contains("DryRun"));
        assert!(s.contains("Workflow 配置"));
    }
}
