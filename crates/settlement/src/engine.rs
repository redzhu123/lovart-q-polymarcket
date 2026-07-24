//! Settlement Engine（成交结算引擎 — P2-06 第二节）。
//!
//! 统一入口：`process_fill()`。
//!
//! Settlement Workflow：
//! ```text
//! Trade Fill Event
//!       │
//!       ▼
//! Validation ──────► 失败 → 终止
//!       │
//!       ▼
//! Fee Calculation
//!       │
//!       ▼
//! Position Update
//!       │
//!       ▼
//! Balance Update
//!       │
//!       ▼
//! PnL Update
//!       │
//!       ▼
//! Ledger Entry
//!       │
//!       ▼
//! Settlement Completed
//! ```
//!
//! 约束：
//! - 禁止 OMS 修改资金。
//! - 禁止 PMS 直接处理成交。
//! - 禁止 Gateway 更新持仓。
//! - 所有成交必须经过 Settlement Engine。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::Local;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::balance::BalanceManager;
use crate::events::{SettlementEvent, SettlementEventBus};
use crate::fee::FeeEngine;
use crate::ledger::Ledger;
use crate::metrics::SettlementMetrics;
use crate::pnl::PnLEngine;
use crate::position::PositionManager;
use crate::repository::SettlementRepository;
use crate::types::{FeeRule, SettlementResult, SettlementStatus, TradeFillEvent};
use crate::validator::SettlementValidator;

// ============================================================================
// SettlementEngine — 统一结算引擎
// ============================================================================

/// Settlement Engine 配置。
#[derive(Debug, Clone)]
pub struct SettlementConfig {
    /// 初始资金（默认 10,000 USDC）。
    pub initial_capital: f64,
    /// 默认账户 ID。
    pub default_account_id: String,
    /// 手续费规则。
    pub fee_rule: FeeRule,
    /// 是否启用事件总线。
    pub enable_event_bus: bool,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            initial_capital: 10_000.0,
            default_account_id: "ACCT-MAIN-001".to_string(),
            fee_rule: FeeRule::zero_fee(),
            enable_event_bus: true,
        }
    }
}

/// Settlement Engine。
///
/// 成交事件唯一处理中心。
/// 所有成交必须经过 `process_fill()`。
pub struct SettlementEngine {
    /// 配置。
    pub config: SettlementConfig,
    /// 手续费引擎。
    pub fee_engine: FeeEngine,
    /// 持仓管理器。
    pub position_mgr: PositionManager,
    /// 余额管理器。
    pub balance_mgr: BalanceManager,
    /// 盈亏引擎。
    pub pnl_engine: PnLEngine,
    /// 资金流水记录器。
    pub ledger: Ledger,
    /// 校验器。
    pub validator: SettlementValidator,
    /// 事件总线。
    pub event_bus: SettlementEventBus,
    /// 指标收集器。
    pub metrics: SettlementMetrics,
    /// 持久化仓库。
    pub repository: Box<dyn SettlementRepository>,
    /// 结算 ID 序列号。
    seq: AtomicU64,
}

impl SettlementEngine {
    /// 创建新结算引擎。
    pub fn new(
        config: SettlementConfig,
        repository: Box<dyn SettlementRepository>,
    ) -> anyhow::Result<Self> {
        let now = Local::now();
        let mut balance_mgr = BalanceManager::new();
        balance_mgr.init_account(
            config.default_account_id.clone(),
            config.initial_capital,
            now,
        );

        let engine = Self {
            fee_engine: FeeEngine::new(config.fee_rule.clone()),
            position_mgr: PositionManager::new(),
            balance_mgr,
            pnl_engine: PnLEngine::new(config.initial_capital),
            ledger: Ledger::new(),
            validator: SettlementValidator::with_default_rules(),
            event_bus: SettlementEventBus::new(),
            metrics: SettlementMetrics::new(),
            config,
            repository,
            seq: AtomicU64::new(0),
        };

        tracing::info!(
            initial_capital = %engine.config.initial_capital,
            account_id = %engine.config.default_account_id,
            "结算引擎已初始化"
        );

        Ok(engine)
    }

    /// 生成结算 ID。
    fn next_settlement_id(&self) -> String {
        let n = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let now = Local::now();
        format!("SETTLE-{}-{:06}", now.format("%Y%m%d"), n)
    }

    // ============================================================================
    // process_fill — 统一结算入口
    // ============================================================================

    /// 处理成交事件。
    ///
    /// 这是 Settlement Engine 的唯一对外入口。
    /// 所有成交必须通过此方法处理。
    ///
    /// # 参数
    ///
    /// - `event`：成交事件。
    ///
    /// # 返回
    ///
    /// `SettlementResult`：结算结果（成功或失败）。
    pub fn process_fill(&mut self, event: &TradeFillEvent) -> SettlementResult {
        let start = Instant::now();
        let settlement_id = self.next_settlement_id();
        let now = Local::now();

        tracing::info!(
            trade_id = %event.trade_id,
            order_id = %event.order_id,
            market_id = %event.market_id,
            direction = %event.direction.as_zh(),
            side = %event.side.as_str(),
            price = %event.fill_price,
            quantity = %event.fill_quantity,
            notional = %event.fill_notional(),
            is_taker = %event.is_taker,
            "══════ 开始结算 ══════"
        );

        // 记录成交事件
        self.metrics.record_fill();
        self.event_bus.publish(SettlementEvent::FillReceived {
            trade_id: event.trade_id.clone(),
            order_id: event.order_id.clone(),
            market_id: event.market_id.clone(),
            notional: event.fill_notional(),
            timestamp: now,
        });

        // 保存成交事件到仓库
        if let Err(e) = self.repository.save_fill_event(event) {
            tracing::warn!(error = %e, "保存成交事件失败");
        }

        // ── Step 1: Validation ──
        let is_sell = matches!(event.side, pm_core::Side::Sell);
        let position = if is_sell {
            self.position_mgr
                .find_open(&event.market_id, event.direction)
        } else {
            None
        };
        let balance = self.balance_mgr.get(&event.account_id);

        let fee_breakdown = self.fee_engine.calculate(event);

        let validation = self
            .validator
            .validate(event, position, balance, &fee_breakdown);
        if !validation.all_passed {
            let error_msg = validation.summary_zh();
            tracing::error!(
                trade_id = %event.trade_id,
                order_id = %event.order_id,
                error = %error_msg,
                "结算校验失败，终止结算"
            );
            self.metrics.record_failure(true);
            self.event_bus.publish(SettlementEvent::ValidationFailed {
                trade_id: event.trade_id.clone(),
                order_id: event.order_id.clone(),
                reason: error_msg.clone(),
                timestamp: now,
            });

            let elapsed = start.elapsed().as_millis() as u64;
            let result = SettlementResult::failed(
                settlement_id,
                event.trade_id.clone(),
                event.order_id.clone(),
                SettlementStatus::ValidationFailed,
                error_msg,
                elapsed,
                now,
            );
            let _ = self.repository.save_settlement(&result);
            return result;
        }

        self.event_bus.publish(SettlementEvent::ValidationPassed {
            trade_id: event.trade_id.clone(),
            order_id: event.order_id.clone(),
            timestamp: now,
        });

        tracing::info!(
            trade_id = %event.trade_id,
            "校验通过"
        );

        // ── Step 2: Fee Calculation ──
        self.event_bus.publish(SettlementEvent::FeeCalculated {
            trade_id: event.trade_id.clone(),
            order_id: event.order_id.clone(),
            total_fee: fee_breakdown.total_fee,
            timestamp: now,
        });

        tracing::info!(
            trade_id = %event.trade_id,
            fee = %fee_breakdown.display_zh(),
            "手续费已计算"
        );

        // ── Step 3 & 4: Position + Balance ──
        let is_buy = matches!(event.side, pm_core::Side::Buy);
        let cost = event.fill_notional();
        let (position_summary, realized_pnl, unrealized_pnl) =
            self.position_mgr.apply_fill(event, now);

        let (balance_before, balance_after) = if is_buy {
            // 买：先冻结，再扣款
            let freeze_result = self
                .balance_mgr
                .freeze_for_open(event, cost, &fee_breakdown, now);
            if freeze_result.is_none() {
                let error_msg = "余额不足，无法冻结".to_string();
                tracing::error!(trade_id = %event.trade_id, error = %error_msg);
                self.metrics.record_failure(false);
                let elapsed = start.elapsed().as_millis() as u64;
                let result = SettlementResult::failed(
                    settlement_id,
                    event.trade_id.clone(),
                    event.order_id.clone(),
                    SettlementStatus::BalanceFailed,
                    error_msg,
                    elapsed,
                    now,
                );
                let _ = self.repository.save_settlement(&result);
                return result;
            }
            self.balance_mgr
                .debit_fill(event, cost, &fee_breakdown, now)
        } else {
            // 卖：平仓入账
            let proceeds = event.fill_notional();
            self.balance_mgr
                .credit_close(event, proceeds, &fee_breakdown, now)
        };

        self.event_bus.publish(SettlementEvent::PositionUpdated {
            trade_id: event.trade_id.clone(),
            order_id: event.order_id.clone(),
            market_id: event.market_id.clone(),
            summary: position_summary.clone(),
            timestamp: now,
        });

        self.event_bus.publish(SettlementEvent::BalanceUpdated {
            trade_id: event.trade_id.clone(),
            order_id: event.order_id.clone(),
            account_id: event.account_id.clone(),
            before: balance_before,
            after: balance_after,
            timestamp: now,
        });

        tracing::info!(
            trade_id = %event.trade_id,
            position = %position_summary,
            balance_before = %balance_before,
            balance_after = %balance_after,
            "持仓/余额更新完成"
        );

        // ── Step 5: PnL Update ──
        if realized_pnl != 0.0 {
            self.pnl_engine.record_realized(realized_pnl);
        }

        self.event_bus.publish(SettlementEvent::PnLUpdated {
            trade_id: event.trade_id.clone(),
            order_id: event.order_id.clone(),
            realized_pnl,
            unrealized_pnl,
            timestamp: now,
        });

        tracing::info!(
            trade_id = %event.trade_id,
            realized_pnl = %realized_pnl,
            unrealized_pnl = %unrealized_pnl,
            "盈亏已更新"
        );

        // ── Step 6: Ledger Entry ──
        let ledger_desc = if is_buy {
            format!(
                "成交扣款: {} {} {} @ {:.4} × {:.2}",
                event.market_id,
                event.direction.as_zh(),
                event.side.as_str(),
                event.fill_price,
                event.fill_quantity
            )
        } else {
            format!(
                "平仓入账: {} {} {} @ {:.4} × {:.2}",
                event.market_id,
                event.direction.as_zh(),
                event.side.as_str(),
                event.fill_price,
                event.fill_quantity
            )
        };

        let _entry = self.ledger.record_debit(
            event,
            cost,
            fee_breakdown.total_fee,
            balance_before,
            balance_after,
            &ledger_desc,
            now,
        );

        // 如果有手续费，单独记录一条手续费流水
        let total_fee = fee_breakdown.total_fee;
        if total_fee > 0.0 {
            self.ledger.record_fee(
                event,
                total_fee,
                balance_before,
                balance_after - total_fee,
                now,
            );
        }

        let ledger_count = if total_fee > 0.0 { 2 } else { 1 };

        self.event_bus.publish(SettlementEvent::LedgerRecorded {
            trade_id: event.trade_id.clone(),
            order_id: event.order_id.clone(),
            ledger_count,
            timestamp: now,
        });

        // 持久化流水
        for entry in self.ledger.recent(2) {
            let _ = self.repository.save_ledger_entry(entry);
        }

        // ── Step 7: Settlement Completed ──
        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.record_success(
            fee_breakdown.total_fee,
            realized_pnl,
            if fee_breakdown.total_fee > 0.0 { 2 } else { 1 },
            elapsed * 1000, // ms → us
        );

        self.event_bus
            .publish(SettlementEvent::SettlementCompleted {
                trade_id: event.trade_id.clone(),
                order_id: event.order_id.clone(),
                settlement_id: settlement_id.clone(),
                status: SettlementStatus::Success,
                elapsed_ms: elapsed,
                timestamp: now,
            });

        tracing::info!(
            trade_id = %event.trade_id,
            settlement_id = %settlement_id,
            elapsed_ms = %elapsed,
            fee = %fee_breakdown.total_fee,
            realized_pnl = %realized_pnl,
            "══════ 结算完成 ══════"
        );

        let result = SettlementResult::success(
            settlement_id,
            event.trade_id.clone(),
            event.order_id.clone(),
            fee_breakdown,
            Some(position_summary),
            balance_before,
            balance_after,
            realized_pnl,
            unrealized_pnl,
            ledger_count,
            elapsed,
            now,
        );

        let _ = self.repository.save_settlement(&result);
        result
    }

    // ============================================================================
    // 查询方法
    // ============================================================================

    /// 获取最近结算结果（从仓库）。
    pub fn recent_settlements(&self, n: usize) -> anyhow::Result<Vec<SettlementResult>> {
        let mut all = self.repository.list_settlements()?;
        let start = all.len().saturating_sub(n);
        Ok(all.split_off(start))
    }

    /// 获取全部结算结果。
    pub fn all_settlements(&self) -> anyhow::Result<Vec<SettlementResult>> {
        self.repository.list_settlements()
    }

    /// 打印仪表盘（中文 CLI 输出）。
    pub fn print_dashboard(&self) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  Settlement Engine — 成交结算引擎");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("  配置:");
        println!("    账户       : {}", self.config.default_account_id);
        println!("    初始资金   : {:.2} USDC", self.config.initial_capital);
        println!("    费率       : {}", self.fee_engine.active_rule.name);
        println!();

        // 指标
        self.metrics.print_zh();

        // 最近结算
        match self.recent_settlements(5) {
            Ok(settlements) if !settlements.is_empty() => {
                println!("── 最近结算（{} 条）──", settlements.len());
                println!();
                for s in &settlements {
                    println!("  {}", s.summary_zh());
                    println!();
                }
            }
            _ => {
                println!("── 最近结算 ──");
                println!();
                println!("  （暂无结算记录）");
                println!();
            }
        }

        println!("═══════════════════════════════════════════════════════════");
        println!("  Simulation Only -- 仅模拟，非真实资金");
        println!("═══════════════════════════════════════════════════════════");
        println!();
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{RepositoryType, create_repository};
    use crate::types::Direction;
    use chrono::Local;
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn sample_fill(side: Side) -> TradeFillEvent {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        TradeFillEvent {
            trade_id: format!("T-{:04}", n),
            order_id: "OMS-001".into(),
            client_order_id: "CLI-001".into(),
            exchange_order_id: None,
            market_id: "mkt-btc".into(),
            account_id: "ACCT-MAIN-001".into(),
            direction: Direction::Yes,
            side,
            fill_price: 0.55,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        }
    }

    fn create_test_engine() -> SettlementEngine {
        let config = SettlementConfig::default();
        let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
        SettlementEngine::new(config, repo).unwrap()
    }

    #[test]
    fn process_buy_fill_successfully() {
        let mut engine = create_test_engine();
        let fill = sample_fill(Side::Buy);
        let result = engine.process_fill(&fill);

        assert!(result.status.is_success());
        assert!(!result.settlement_id.is_empty());

        // 验证持仓
        assert_eq!(engine.position_mgr.open_count(), 1);
        let pos = engine
            .position_mgr
            .find_open("mkt-btc", Direction::Yes)
            .unwrap();
        assert!(approx(pos.quantity, 100.0));
        assert!(approx(pos.average_price, 0.55));

        // 验证余额（10000 - 55 = 9945）
        let bal = engine.balance_mgr.get("ACCT-MAIN-001").unwrap();
        assert!(approx(bal.available, 10000.0 - 55.0));

        // 验证流水
        assert!(engine.ledger.count() > 0);
    }

    #[test]
    fn process_sell_fill_successfully() {
        let mut engine = create_test_engine();

        // 先买后卖
        let buy = sample_fill(Side::Buy);
        engine.process_fill(&buy);

        let mut sell = sample_fill(Side::Sell);
        sell.trade_id = "T-SELL-001".into();
        sell.fill_price = 0.65; // 盈利
        let result = engine.process_fill(&sell);

        assert!(result.status.is_success());
        assert!(approx(result.realized_pnl, 10.0)); // 100 * (0.65 - 0.55)
        assert_eq!(engine.position_mgr.open_count(), 0); // 已平仓
        assert_eq!(engine.position_mgr.closed_count(), 1);

        // 验证余额增加了
        let bal = engine.balance_mgr.get("ACCT-MAIN-001").unwrap();
        // 初始 10000, buy 花了 55, sell 收回 65 → 10010
        assert!(approx(bal.available, 10010.0));
    }

    #[test]
    fn process_fill_validation_fails_on_bad_price() {
        let mut engine = create_test_engine();
        let mut fill = sample_fill(Side::Buy);
        fill.fill_price = -0.5;
        let result = engine.process_fill(&fill);

        assert!(!result.status.is_success());
        assert_eq!(result.status, SettlementStatus::ValidationFailed);
    }

    #[test]
    fn process_fill_insufficient_balance() {
        let mut engine = create_test_engine();
        // 耗尽余额
        engine.balance_mgr = BalanceManager::new();
        engine
            .balance_mgr
            .init_account("ACCT-MAIN-001".into(), 10.0, Local::now());

        let fill = sample_fill(Side::Buy); // cost = 55
        let result = engine.process_fill(&fill);

        assert!(!result.status.is_success());
    }

    #[test]
    fn multiple_fills_add_to_position() {
        let mut engine = create_test_engine();

        engine.process_fill(&sample_fill(Side::Buy));
        let mut fill2 = sample_fill(Side::Buy);
        fill2.trade_id = "T-002".into();
        fill2.fill_price = 0.60;
        engine.process_fill(&fill2);

        assert_eq!(engine.position_mgr.open_count(), 1);
        let pos = engine
            .position_mgr
            .find_open("mkt-btc", Direction::Yes)
            .unwrap();
        assert!(approx(pos.quantity, 200.0));
        assert!(approx(pos.average_price, 0.575)); // (55 + 60) / 200
    }

    #[test]
    fn metrics_updated_on_success() {
        let mut engine = create_test_engine();
        engine.process_fill(&sample_fill(Side::Buy));
        engine.process_fill(&sample_fill(Side::Buy));

        let snap = engine.metrics.snapshot();
        assert_eq!(snap.total_fills, 2);
        assert_eq!(snap.successful_settlements, 2);
        assert!(approx(snap.success_rate, 1.0));
    }

    #[test]
    fn repository_stores_results() {
        let mut engine = create_test_engine();
        engine.process_fill(&sample_fill(Side::Buy));

        let settlements = engine.repository.list_settlements().unwrap();
        assert_eq!(settlements.len(), 1);
        assert_eq!(settlements[0].trade_id, settlements[0].trade_id);
    }

    #[test]
    fn print_dashboard_does_not_panic() {
        let mut engine = create_test_engine();
        engine.process_fill(&sample_fill(Side::Buy));
        engine.print_dashboard();
    }
}
