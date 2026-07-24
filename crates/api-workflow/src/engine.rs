//! Workflow Engine（P2-02）。
//!
//! 驱动状态机 + 录制器 + 校验器，执行 API 调用流程。
//! - `run_full_lifecycle`：DryRun / Replay 完整生命周期。
//! - `run_readonly_lifecycle`：Live ReadOnly 只读子集。
//!
//! **安全**：提交订单步骤仅构建请求并校验参数，不发送（dry_run=true）。

use chrono::Utc;
use serde_json::{Value, json};

use pm_api_test::client::http::ApiClient;
use pm_api_test::validator::response::ResponseValidator;

use crate::config::{WorkflowConfig, WorkflowMode};
use crate::recorder::{ApiCallRecord, StepRecord, WorkflowRecorder};
use crate::report::types::WorkflowReport;
use crate::state_machine::{StateMachine, WorkflowState};
use crate::validator::WorkflowValidator;

/// Workflow Engine。
pub struct WorkflowEngine {
    /// 配置。
    config: WorkflowConfig,
    /// API 客户端（Mock / Live）。
    client: ApiClient,
    /// 响应校验器（复用 P2-01）。
    #[allow(dead_code)]
    validator: ResponseValidator,
    /// 状态机。
    sm: StateMachine,
    /// 录制器。
    recorder: WorkflowRecorder,
}

impl WorkflowEngine {
    /// 创建新的 Workflow Engine。
    pub fn new(config: WorkflowConfig) -> Self {
        let api_cfg = config.to_api_test_config();
        let client = ApiClient::new(api_cfg);
        let validator = ResponseValidator::new();
        let run_id = format!("wf-{}", Utc::now().format("%Y%m%d-%H%M%S%3f"));

        tracing::info!("═══════════════════════════════════════════════════════════");
        tracing::info!(
            "  Workflow Engine 启动 | run_id={} | 模式={}",
            run_id,
            config.mode.as_zh()
        );
        tracing::info!("═══════════════════════════════════════════════════════════");

        Self {
            config,
            client,
            validator,
            sm: StateMachine::new(),
            recorder: WorkflowRecorder::new(&run_id),
        }
    }

    /// 配置引用。
    pub fn config(&self) -> &WorkflowConfig {
        &self.config
    }

    /// 录制器引用。
    pub fn recorder(&self) -> &WorkflowRecorder {
        &self.recorder
    }

    /// 当前状态。
    pub fn current_state(&self) -> WorkflowState {
        self.sm.current()
    }

    /// 是否已认证（用于 LiveReadOnly 决定是否读取 Balance/Position）。
    fn is_authed(&self) -> bool {
        self.client.config().api_key.is_some()
    }

    /// LiveReadOnly 未认证时跳过认证读取。
    fn should_skip_authed_read(&self) -> bool {
        self.config.mode == WorkflowMode::LiveReadOnly && !self.is_authed()
    }

    // ----------------------------------------------------------------
    // 生命周期编排
    // ----------------------------------------------------------------

    /// 运行完整生命周期（DryRun / Replay）。
    pub async fn run_full_lifecycle(&mut self) -> WorkflowReport {
        tracing::info!("【生命周期】开始完整交易生命周期（DryRun）");

        if !self.step_loading_market().await.success {
            return self.finalize();
        }
        if !self.step_loading_orderbook().await.success {
            return self.finalize();
        }
        if !self.step_checking_balance().await.success {
            return self.finalize();
        }
        if !self.step_building_order().await.success {
            return self.finalize();
        }
        if !self.step_submitting_order_dryrun().await.success {
            return self.finalize();
        }
        if !self.step_waiting_result().await.success {
            return self.finalize();
        }
        if !self.step_sync_order().await.success {
            return self.finalize();
        }
        if !self.step_sync_trade().await.success {
            return self.finalize();
        }
        if !self.step_sync_position().await.success {
            return self.finalize();
        }
        if !self.step_sync_balance().await.success {
            return self.finalize();
        }
        self.step_complete().await;
        self.finalize()
    }

    /// 运行只读生命周期（LiveReadOnly）。
    pub async fn run_readonly_lifecycle(&mut self) -> WorkflowReport {
        tracing::info!("【生命周期】开始只读生命周期（Live ReadOnly）");

        if !self.step_loading_market().await.success {
            return self.finalize();
        }
        if !self.step_loading_orderbook().await.success {
            return self.finalize();
        }
        if !self.step_checking_balance().await.success {
            return self.finalize();
        }
        if !self.step_sync_position().await.success {
            return self.finalize();
        }
        if !self.step_sync_balance().await.success {
            return self.finalize();
        }
        self.step_complete().await;
        self.finalize()
    }

    /// 终化：生成 Trace + 校验 + 报告。
    fn finalize(&self) -> WorkflowReport {
        let trace = self.recorder.trace(self.config.mode.as_zh());
        let validation = WorkflowValidator::validate(&trace, self.config.mode);
        let report = WorkflowReport::from_trace(trace, validation);

        tracing::info!("═══════════════════════════════════════════════════════════");
        tracing::info!("{}", report.summary_zh());
        tracing::info!("{}", report.validation.summary_zh());
        if !report.validation.failures.is_empty() {
            for f in &report.validation.failures {
                tracing::warn!("  ❌ {}", f);
            }
        }
        tracing::info!("═══════════════════════════════════════════════════════════");

        report
    }

    // ----------------------------------------------------------------
    // 步骤实现
    // ----------------------------------------------------------------

    /// 加载市场列表：GET /markets。
    async fn step_loading_market(&mut self) -> StepRecord {
        let _ = self
            .sm
            .transition(WorkflowState::LoadingMarket, "开始加载市场列表");
        let mut step = StepRecord::start(WorkflowState::LoadingMarket);

        match self.client.get("/markets").await {
            Ok(resp) => {
                step.add_api_call(ApiCallRecord::from_response(
                    "GET", "/markets", None, &resp, false,
                ));
                if resp.is_success() {
                    step.add_note(&format!("市场列表加载成功（HTTP {}）", resp.status));
                } else {
                    step.fail(&format!("市场列表加载失败: HTTP {}", resp.status));
                }
            }
            Err(e) => step.fail(&format!("市场列表请求失败: {}", e)),
        }

        self.finish_step(&mut step);
        step
    }

    /// 加载订单簿：GET /book?token_id=。
    async fn step_loading_orderbook(&mut self) -> StepRecord {
        let path = format!("/book?token_id={}", self.config.target_token_id);
        let _ = self
            .sm
            .transition(WorkflowState::LoadingOrderBook, "开始加载订单簿");
        let mut step = StepRecord::start(WorkflowState::LoadingOrderBook);

        match self.client.get(&path).await {
            Ok(resp) => {
                step.add_api_call(ApiCallRecord::from_response(
                    "GET", &path, None, &resp, false,
                ));
                if resp.is_success() {
                    step.add_note(&format!("订单簿加载成功（HTTP {}）", resp.status));
                } else {
                    step.fail(&format!("订单簿加载失败: HTTP {}", resp.status));
                }
            }
            Err(e) => step.fail(&format!("订单簿请求失败: {}", e)),
        }

        self.finish_step(&mut step);
        step
    }

    /// 检查余额：GET /balances。
    async fn step_checking_balance(&mut self) -> StepRecord {
        let _ = self
            .sm
            .transition(WorkflowState::CheckingBalance, "开始检查余额");
        let mut step = StepRecord::start(WorkflowState::CheckingBalance);

        if self.should_skip_authed_read() {
            step.add_note("未认证，跳过余额读取（LiveReadOnly）");
        } else {
            match self.client.get("/balances").await {
                Ok(resp) => {
                    step.add_api_call(ApiCallRecord::from_response(
                        "GET",
                        "/balances",
                        None,
                        &resp,
                        false,
                    ));
                    if resp.is_success() {
                        step.add_note(&format!("余额查询成功（HTTP {}）", resp.status));
                    } else {
                        step.fail(&format!("余额查询失败: HTTP {}", resp.status));
                    }
                }
                Err(e) => step.fail(&format!("余额请求失败: {}", e)),
            }
        }

        self.finish_step(&mut step);
        step
    }

    /// 构建订单（本地）：构造 CLOB V2 订单 JSON，不发送。
    async fn step_building_order(&mut self) -> StepRecord {
        let _ = self
            .sm
            .transition(WorkflowState::BuildingOrder, "开始构建订单");
        let mut step = StepRecord::start(WorkflowState::BuildingOrder);

        let order_json = self.build_order_json();
        step.add_api_call(ApiCallRecord::dry_run_local(
            "POST",
            "/order",
            Some(&order_json),
        ));
        step.add_note(&format!(
            "订单已构建: {} {} {:.2} @ {:.4}（token={}）",
            self.config.order_side,
            "GTC",
            self.config.order_size,
            self.config.order_price,
            self.config.target_token_id,
        ));

        self.finish_step(&mut step);
        step
    }

    /// 提交订单（DryRun）：校验参数，不发送。
    async fn step_submitting_order_dryrun(&mut self) -> StepRecord {
        let _ = self
            .sm
            .transition(WorkflowState::SubmittingOrder, "DryRun 提交订单");
        let mut step = StepRecord::start(WorkflowState::SubmittingOrder);

        // 参数校验（本地）
        let mut errors: Vec<String> = Vec::new();
        if !matches!(self.config.order_side.as_str(), "BUY" | "SELL") {
            errors.push(format!("无效订单方向: {}", self.config.order_side));
        }
        if !(0.0..=1.0).contains(&self.config.order_price) {
            errors.push(format!("订单价格超出 [0,1]: {}", self.config.order_price));
        }
        if self.config.order_size <= 0.0 {
            errors.push(format!("订单数量需 > 0: {}", self.config.order_size));
        }
        if self.config.target_token_id.is_empty() {
            errors.push("目标 token_id 为空".to_string());
        }

        if errors.is_empty() {
            step.add_note("订单参数校验通过");
            step.add_note("🔒 DryRun - 订单未发送至交易所");
        } else {
            step.fail(&format!("订单参数校验失败: {}", errors.join("; ")));
        }

        self.finish_step(&mut step);
        step
    }

    /// 等待结果（DryRun 模拟）。
    async fn step_waiting_result(&mut self) -> StepRecord {
        let _ = self
            .sm
            .transition(WorkflowState::WaitingResult, "等待订单结果");
        let mut step = StepRecord::start(WorkflowState::WaitingResult);
        step.add_note("DryRun 模拟: 订单已接受");
        step.add_note("DryRun 模拟: 订单已成交（Filled）");
        self.finish_step(&mut step);
        step
    }

    /// 同步订单状态：GET /orders。
    async fn step_sync_order(&mut self) -> StepRecord {
        let _ = self.sm.transition(WorkflowState::SyncOrder, "同步订单状态");
        let mut step = StepRecord::start(WorkflowState::SyncOrder);

        match self.client.get("/orders").await {
            Ok(resp) => {
                step.add_api_call(ApiCallRecord::from_response(
                    "GET", "/orders", None, &resp, false,
                ));
                if resp.is_success() {
                    step.add_note(&format!("订单状态查询成功（HTTP {}）", resp.status));
                } else {
                    step.fail(&format!("订单状态查询失败: HTTP {}", resp.status));
                }
            }
            Err(e) => step.fail(&format!("订单状态请求失败: {}", e)),
        }

        self.finish_step(&mut step);
        step
    }

    /// 同步成交记录：GET /trades。
    async fn step_sync_trade(&mut self) -> StepRecord {
        let _ = self.sm.transition(WorkflowState::SyncTrade, "同步成交记录");
        let mut step = StepRecord::start(WorkflowState::SyncTrade);

        match self.client.get("/trades").await {
            Ok(resp) => {
                step.add_api_call(ApiCallRecord::from_response(
                    "GET", "/trades", None, &resp, false,
                ));
                if resp.is_success() {
                    step.add_note(&format!("成交记录查询成功（HTTP {}）", resp.status));
                } else {
                    step.fail(&format!("成交记录查询失败: HTTP {}", resp.status));
                }
            }
            Err(e) => step.fail(&format!("成交记录请求失败: {}", e)),
        }

        self.finish_step(&mut step);
        step
    }

    /// 同步持仓：GET /positions。
    async fn step_sync_position(&mut self) -> StepRecord {
        let _ = self.sm.transition(WorkflowState::SyncPosition, "同步持仓");
        let mut step = StepRecord::start(WorkflowState::SyncPosition);

        if self.should_skip_authed_read() {
            step.add_note("未认证，跳过持仓读取（LiveReadOnly）");
        } else {
            match self.client.get("/positions").await {
                Ok(resp) => {
                    step.add_api_call(ApiCallRecord::from_response(
                        "GET",
                        "/positions",
                        None,
                        &resp,
                        false,
                    ));
                    if resp.is_success() {
                        step.add_note(&format!("持仓查询成功（HTTP {}）", resp.status));
                    } else {
                        step.fail(&format!("持仓查询失败: HTTP {}", resp.status));
                    }
                }
                Err(e) => step.fail(&format!("持仓请求失败: {}", e)),
            }
        }

        self.finish_step(&mut step);
        step
    }

    /// 同步余额：GET /balances。
    async fn step_sync_balance(&mut self) -> StepRecord {
        let _ = self.sm.transition(WorkflowState::SyncBalance, "同步余额");
        let mut step = StepRecord::start(WorkflowState::SyncBalance);

        if self.should_skip_authed_read() {
            step.add_note("未认证，跳过余额同步（LiveReadOnly）");
        } else {
            match self.client.get("/balances").await {
                Ok(resp) => {
                    step.add_api_call(ApiCallRecord::from_response(
                        "GET",
                        "/balances",
                        None,
                        &resp,
                        false,
                    ));
                    if resp.is_success() {
                        step.add_note(&format!("余额同步成功（HTTP {}）", resp.status));
                    } else {
                        step.fail(&format!("余额同步失败: HTTP {}", resp.status));
                    }
                }
                Err(e) => step.fail(&format!("余额同步请求失败: {}", e)),
            }
        }

        self.finish_step(&mut step);
        step
    }

    /// 完成。
    async fn step_complete(&mut self) -> StepRecord {
        let _ = self.sm.transition(WorkflowState::Completed, "生命周期完成");
        let mut step = StepRecord::start(WorkflowState::Completed);
        step.add_note("Workflow 生命周期完成");
        self.finish_step(&mut step);
        step
    }

    // ----------------------------------------------------------------
    // 内部辅助
    // ----------------------------------------------------------------

    /// 完成步骤并记录（失败时推动状态机进入 Failed）。
    fn finish_step(&mut self, step: &mut StepRecord) {
        step.finish();
        if !step.success {
            self.sm
                .force_failed(step.failure_reason.as_deref().unwrap_or("未知原因"));
        }
        let recorded = step.clone();
        self.recorder.record(recorded);
    }

    /// 构建 CLOB V2 订单 JSON（使用配置参数）。
    fn build_order_json(&self) -> Value {
        // 金额以 6 位小数（USDC）表示
        let maker_amount = (self.config.order_size * 1_000_000.0).round() as u64;
        let taker_amount =
            (self.config.order_price * self.config.order_size * 1_000_000.0).round() as u64;

        json!({
            "order": {
                "salt": 123456789,
                "maker": "0x1234567890abcdef1234567890abcdef12345678",
                "signer": "0x1234567890abcdef1234567890abcdef12345678",
                "taker": "0x0000000000000000000000000000000000000000",
                "tokenId": self.config.target_token_id,
                "makerAmount": maker_amount.to_string(),
                "takerAmount": taker_amount.to_string(),
                "expiration": "0",
                "nonce": "0",
                "feeRateBps": "0",
                "side": self.config.order_side,
                "signatureType": 3
            },
            "signature": "0x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "orderType": "GTC"
        })
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn full_lifecycle_dryrun_completes() {
        crate::init_logging("warn");
        let cfg = WorkflowConfig::default(); // DryRun + fixtures
        let mut engine = WorkflowEngine::new(cfg);
        let report = engine.run_full_lifecycle().await;

        assert!(
            report.success,
            "DryRun 完整生命周期应成功: {}",
            report.summary_zh()
        );
        assert!(report.validation.passed);
        assert_eq!(engine.current_state(), WorkflowState::Completed);
        // 11 个步骤 + Completed = 实际步骤数 = 11（含 Completed）
        assert!(report.total_steps >= 11);
    }

    #[tokio::test]
    async fn readonly_lifecycle_completes() {
        crate::init_logging("warn");
        // LiveReadOnly 默认 enable_live_reads=false -> Mock 客户端，可离线校验只读约束
        let cfg = WorkflowConfig {
            mode: WorkflowMode::LiveReadOnly,
            ..WorkflowConfig::default()
        };
        let mut engine = WorkflowEngine::new(cfg);
        let report = engine.run_readonly_lifecycle().await;

        // Mock 模式下未认证 -> 跳过 balance/position，但 markets/orderbook 读取成功
        assert!(
            report.validation.passed,
            "只读路径应通过: {}",
            report.summary_zh()
        );
    }

    #[tokio::test]
    async fn dryrun_never_sends_real_write() {
        crate::init_logging("warn");
        let cfg = WorkflowConfig::default();
        let mut engine = WorkflowEngine::new(cfg);
        let report = engine.run_full_lifecycle().await;

        // 所有写操作必须 dry_run
        let real_writes: Vec<_> = report
            .api_sequence
            .iter()
            .filter(|c| c.is_write() && !c.dry_run)
            .collect();
        assert!(real_writes.is_empty(), "DryRun 不应发送真实写操作");
    }

    #[test]
    fn build_order_json_uses_config() {
        let cfg = WorkflowConfig::default();
        let engine = WorkflowEngine::new(cfg);
        let order = engine.build_order_json();
        assert_eq!(order["order"]["side"], "BUY");
        assert_eq!(
            order["order"]["tokenId"],
            WorkflowConfig::default().target_token_id
        );
        assert_eq!(order["orderType"], "GTC");
    }
}
