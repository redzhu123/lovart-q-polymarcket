//! pm-pms：Portfolio Management System（P2-05）。
//!
//! 企业级投资组合管理系统，是系统唯一的资金/持仓/盈亏/估值/风险敞口管理中心。
//!
//! # 架构
//!
//! ```text
//! OMS EventBus
//!       │
//!       ▼
//! ┌──────────────────────────┐
//! │   PMS  (P2-05)           │
//! │  ┌────────────────────┐  │
//! │  │ PmsEventSubscriber │  │  ← 实现 OMS Subscriber trait
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  PortfolioManager  │  │  资金/权益/盈亏/收益率
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  PositionManager   │  │  统一持仓模型
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  PnLEngine         │  │  已实现/未实现/日盈亏/胜率/盈亏比
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  ValuationEngine   │  │  NAV/市值/估值
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  ExposureEngine    │  │  多空/市场类型/单市场敞口/资产配置
//! │  └─────────┬──────────┘  │
//! │            │             │
//! │  ┌─────────▼──────────┐  │
//! │  │  Repository        │  │  Memory / CSV
//! │  └────────────────────┘  │
//! └──────────────────────────┘
//! ```
//!
//! # 模块
//!
//! - [`domain`]：统一领域对象（Portfolio / Account / Position / PnLReport / ExposureReport）。
//! - [`portfolio`]：PortfolioManager — 资金/权益管理。
//! - [`position`]：PositionManager — 持仓增删改查/mark-to-market。
//! - [`account`]：AccountManager — 多账户管理。
//! - [`pnl`]：PnLEngine — 盈亏统一计算。
//! - [`valuation`]：ValuationEngine — 统一估值。
//! - [`exposure`]：ExposureEngine — 风险敞口计算。
//! - [`events`]：PmsEventSubscriber — 实现 OMS Subscriber trait，事件驱动更新。
//! - [`repository`]：PortfolioRepository trait + Memory / CSV 实现。
//! - [`metrics`]：PmsMetrics — 统计指标。
//!
//! # 业务约束
//!
//! - 禁止真实交易 / 真实资金 / Wallet / 签名。
//! - 禁止修改 OMS / Gateway / Execution。
//! - 所有日志使用 tracing，中文输出。
//! - PMS 仅负责资产管理，不调用 OMS 主动操作。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易 / 不签名 / 不上链。

pub mod account;
pub mod domain;
pub mod events;
pub mod exposure;
pub mod metrics;
pub mod pnl;
pub mod portfolio;
pub mod position;
pub mod repository;
pub mod valuation;

use chrono::Local;
use domain::{
    Account, AssetType, Direction, ExposureReport, PnLReport, Portfolio, Position, PositionStatus,
    ValuationReport,
};
use pm_core::Side;

use account::AccountManager;
use exposure::ExposureEngine;
use metrics::MetricsCalculator;
use pnl::PnLEngine;
use portfolio::PortfolioManager;
use position::PositionManager;
use valuation::ValuationEngine;

// ---- 常用导出 ----
pub mod prelude {
    pub use crate::Pms;
    pub use crate::account::AccountManager;
    pub use crate::create_csv_pms;
    pub use crate::create_default_pms;
    pub use crate::domain::{
        Account, AssetAllocation, AssetType, Balance, Currency, Direction, ExposureReport, Holding,
        MarketExposure, PmsMetrics, PnLReport, Portfolio, Position, PositionStatus,
        ValuationReport,
    };
    pub use crate::events::PmsEventSubscriber;
    pub use crate::exposure::ExposureEngine;
    pub use crate::metrics::MetricsCalculator;
    pub use crate::pnl::PnLEngine;
    pub use crate::portfolio::PortfolioManager;
    pub use crate::position::PositionManager;
    pub use crate::repository::csv::CsvPortfolioRepository;
    pub use crate::repository::memory::InMemoryPortfolioRepository;
    pub use crate::repository::{
        PortfolioRepository, RepositoryHealth, RepositoryType, create_repository,
    };
    pub use crate::valuation::ValuationEngine;
}

// ============================================================================
// PmsConfig — PMS 配置
// ============================================================================

/// PMS 配置。
#[derive(Debug, Clone)]
pub struct PmsConfig {
    /// Repository 类型（默认 Memory）。
    pub repository_type: repository::RepositoryType,
    /// portfolio.csv 路径。
    pub portfolio_csv: Option<std::path::PathBuf>,
    /// positions.csv 路径。
    pub positions_csv: Option<std::path::PathBuf>,
    /// pnl.csv 路径。
    pub pnl_csv: Option<std::path::PathBuf>,
    /// 初始资金（USDC）。
    pub initial_capital: f64,
    /// 是否自动订阅 OMS EventBus。
    pub subscribe_to_oms: bool,
}

impl Default for PmsConfig {
    fn default() -> Self {
        Self {
            repository_type: repository::RepositoryType::Memory,
            portfolio_csv: None,
            positions_csv: None,
            pnl_csv: None,
            initial_capital: 10_000.0,
            subscribe_to_oms: true,
        }
    }
}

// ============================================================================
// Pms — 顶层 PMS 对象
// ============================================================================

/// PMS 顶层对象：统一管理 Portfolio / Account / Position / PnL / Valuation / Exposure / Metrics。
pub struct Pms {
    config: PmsConfig,
    /// 投资组合管理器。
    pub portfolio_mgr: PortfolioManager,
    /// 持仓管理器。
    pub position_mgr: PositionManager,
    /// 账户管理器。
    pub account_mgr: AccountManager,
    /// 盈亏引擎。
    pub pnl_engine: PnLEngine,
    /// 估值引擎。
    pub valuation_engine: ValuationEngine,
    /// 风险敞口引擎。
    pub exposure_engine: ExposureEngine,
    /// 指标计算器。
    pub metrics_calc: MetricsCalculator,
    /// 持久化仓库。
    pub repository: Box<dyn repository::PortfolioRepository>,
}

impl Pms {
    /// 创建新 PMS 实例。
    pub fn new(
        config: PmsConfig,
        repository: Box<dyn repository::PortfolioRepository>,
    ) -> anyhow::Result<Self> {
        let now = Local::now();
        let initial_capital = config.initial_capital;

        let portfolio = Portfolio::new(
            "PF-MAIN-001".to_string(),
            "主投资组合".to_string(),
            initial_capital,
            now,
        );

        let main_account = Account::default_main(now);

        Ok(Self {
            portfolio_mgr: PortfolioManager::new(portfolio),
            position_mgr: PositionManager::new(),
            account_mgr: AccountManager::new(vec![main_account]),
            pnl_engine: PnLEngine::new(initial_capital),
            valuation_engine: ValuationEngine::new(),
            exposure_engine: ExposureEngine::new(),
            metrics_calc: MetricsCalculator::new(),
            config,
            repository,
        })
    }

    /// 获取当前投资组合快照。
    pub fn portfolio(&self) -> &Portfolio {
        self.portfolio_mgr.portfolio()
    }

    /// 获取全部持仓。
    pub fn positions(&self) -> &[Position] {
        self.position_mgr.positions()
    }

    /// 获取全部账户。
    pub fn accounts(&self) -> &[Account] {
        self.account_mgr.accounts()
    }

    /// 获取初始资金。
    pub fn initial_capital(&self) -> f64 {
        self.config.initial_capital
    }

    // ---- 事件处理 ----

    /// 处理订单成交：开仓/加仓 + 扣款。
    pub fn handle_order_filled(
        &mut self,
        order_id: &str,
        market_id: &str,
        direction: Direction,
        side: Side,
        price: f64,
        quantity: f64,
    ) -> anyhow::Result<()> {
        let now = Local::now();

        // 确定资产类型（当前所有市场均为 Prediction）
        let asset_type = AssetType::Prediction;

        let cost = price * quantity;

        tracing::info!(
            order_id = %order_id,
            market_id = %market_id,
            price = %price,
            quantity = %quantity,
            cost = %cost,
            "PMS 处理订单成交"
        );

        // 1. 投资组合扣款
        self.portfolio_mgr.freeze_and_debit(cost);
        tracing::info!(
            available_cash = %self.portfolio_mgr.portfolio().available_cash,
            frozen_cash = %self.portfolio_mgr.portfolio().frozen_cash,
            "组合资金更新：扣款完成"
        );

        // 2. 查找或创建持仓
        let existing_idx = self.position_mgr.find_open_by_market(market_id, direction);

        if let Some(idx) = existing_idx {
            // 加仓
            self.position_mgr
                .add_to_position(idx, quantity, price, order_id, now);
            tracing::info!(
                market_id = %market_id,
                direction = %direction.as_zh(),
                "持仓加仓完成"
            );
        } else {
            // 新建持仓
            let pos_id = self.position_mgr.next_position_id(now);
            let pos = Position::open(
                pos_id,
                market_id.to_string(),
                asset_type,
                direction,
                side,
                quantity,
                price,
                order_id.to_string(),
                now,
            );
            self.position_mgr.add_position(pos);
            tracing::info!(
                market_id = %market_id,
                direction = %direction.as_zh(),
                "新持仓创建完成"
            );
        }

        // 3. 重算组合指标
        self.revalue_all(now);

        // 4. 持久化
        self.repository
            .save_portfolio(self.portfolio_mgr.portfolio())?;
        self.repository
            .save_positions(self.position_mgr.positions())?;

        Ok(())
    }

    /// 处理订单取消/拒绝：释放冻结资金。
    pub fn handle_order_cancelled(&mut self, order_id: &str, reason: &str) {
        let now = Local::now();
        tracing::info!(
            order_id = %order_id,
            reason = %reason,
            "PMS 处理订单取消：释放冻结资金"
        );
        // 注意：暂时无法精确知道该订单冻结了多少资金，
        // 这里做简化处理：不做细粒度释放（实际生产需维护订单-资金映射）。
        // 重新计算时冻结资金会被自然核销。
        self.revalue_all(now);
        let _ = self
            .repository
            .save_portfolio(self.portfolio_mgr.portfolio());
    }

    /// 重算所有指标（Portfolio / PnL / Valuation / Exposure / Metrics）。
    pub fn revalue_all(&mut self, now: chrono::DateTime<Local>) {
        let positions = self.position_mgr.positions();
        let portfolio = self.portfolio_mgr.portfolio_mut();

        // 1. Portfolio 重估值
        portfolio.revalue(positions, now);

        // 2. PnL 重算
        let pnl_report = self.pnl_engine.calculate(positions, portfolio);

        // 3. 估值
        let valuation = self.valuation_engine.calculate(positions, portfolio, now);

        // 4. 风险敞口
        let exposure = self.exposure_engine.calculate(positions, portfolio, now);

        tracing::debug!(
            total_assets = %portfolio.total_assets,
            total_equity = %portfolio.total_equity,
            total_pnl = %pnl_report.total_pnl,
            nav = %valuation.nav,
            net_exposure = %exposure.net_exposure,
            "PMS 全部指标重算完成"
        );
    }

    /// 生成 PnL 报告。
    pub fn generate_pnl_report(&self) -> PnLReport {
        self.pnl_engine.calculate(
            self.position_mgr.positions(),
            self.portfolio_mgr.portfolio(),
        )
    }

    /// 生成估值报告。
    pub fn generate_valuation_report(&self) -> ValuationReport {
        let now = Local::now();
        self.valuation_engine.calculate(
            self.position_mgr.positions(),
            self.portfolio_mgr.portfolio(),
            now,
        )
    }

    /// 生成风险敞口报告。
    pub fn generate_exposure_report(&self) -> ExposureReport {
        let now = Local::now();
        self.exposure_engine.calculate(
            self.position_mgr.positions(),
            self.portfolio_mgr.portfolio(),
            now,
        )
    }

    /// 生成 PMS 指标。
    pub fn generate_metrics(&self) -> domain::PmsMetrics {
        let now = Local::now();
        self.metrics_calc.calculate(
            self.position_mgr.positions(),
            self.portfolio_mgr.portfolio(),
            now,
        )
    }

    /// 打印投资组合（中文 CLI 输出）。
    pub fn print_portfolio(&self) {
        self.portfolio_mgr.print_zh();
    }

    /// 打印全部持仓（中文 CLI 输出）。
    pub fn print_positions(&self) {
        self.position_mgr.print_zh();
    }

    /// 打印盈亏报告（中文 CLI 输出）。
    pub fn print_pnl(&self) {
        let report = self.generate_pnl_report();
        self.pnl_engine.print_zh(&report);
    }

    /// 打印风险敞口（中文 CLI 输出）。
    pub fn print_exposure(&self) {
        let report = self.generate_exposure_report();
        self.exposure_engine.print_zh(&report);
    }

    /// 打印完整 PMS 仪表盘（中文 CLI 输出）。
    pub fn print_dashboard(&self) {
        let portfolio = self.portfolio_mgr.portfolio();
        let positions = self.position_mgr.positions();
        let pnl_report = self.generate_pnl_report();
        let valuation = self.generate_valuation_report();
        let exposure = self.generate_exposure_report();
        let metrics = self.generate_metrics();

        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  PMS 投资组合管理系统 — 仪表盘");
        println!("═══════════════════════════════════════════════════════════");
        println!();

        // 投资组合概览
        println!("── 投资组合 ──");
        println!("  组合名称    : {}", portfolio.name);
        println!("  初始资金    : {:.2} USDC", portfolio.initial_capital);
        println!("  可用资金    : {:.2} USDC", portfolio.available_cash);
        println!("  冻结资金    : {:.2} USDC", portfolio.frozen_cash);
        println!("  持仓价值    : {:.2} USDC", portfolio.position_value);
        println!("  总资产      : {:.2} USDC", portfolio.total_assets);
        println!("  总权益      : {:.2} USDC", portfolio.total_equity);
        println!();

        // 盈亏
        println!("── 盈亏 ──");
        println!("  已实现盈亏  : {:.2} USDC", pnl_report.realized_pnl);
        println!("  未实现盈亏  : {:.2} USDC", pnl_report.unrealized_pnl);
        println!("  总盈亏      : {:.2} USDC", pnl_report.total_pnl);
        println!("  收益率      : {:.2}%", pnl_report.roi * 100.0);
        println!("  胜率        : {:.1}%", pnl_report.win_rate * 100.0);
        println!("  盈亏比      : {:.2}", pnl_report.profit_factor);
        println!();

        // 估值
        println!("── 估值 ──");
        println!("  NAV         : {:.2} USDC", valuation.nav);
        println!("  总市值      : {:.2} USDC", valuation.market_value);
        println!();

        // 风险敞口
        println!("── 风险敞口 ──");
        println!("  多头敞口    : {:.2} USDC", exposure.long_exposure);
        println!("  空头敞口    : {:.2} USDC", exposure.short_exposure);
        println!("  净敞口      : {:.2} USDC", exposure.net_exposure);
        println!();

        // 持仓
        let open_count = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .count();
        let closed_count = positions.len() - open_count;
        println!("── 持仓 ──");
        println!("  当前持仓    : {} 个", open_count);
        println!("  已平仓      : {} 个", closed_count);
        println!();

        // 指标
        println!("── 指标 ──");
        println!("  盈利率      : {:.1}%", metrics.win_rate * 100.0);
        println!("  累计收益率  : {:.2}%", metrics.return_rate * 100.0);
        println!("  平均每笔收益: {:.2} USDC", metrics.avg_profit_per_trade);
        println!("  总交易笔数  : {}", metrics.total_closed_trades);
        println!();

        println!("═══════════════════════════════════════════════════════════");
        println!("  Simulation Only -- 仅模拟，非真实资金");
        println!("═══════════════════════════════════════════════════════════");
        println!();
    }
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建默认 PMS（Memory 仓库）。
pub fn create_default_pms() -> anyhow::Result<Pms> {
    let config = PmsConfig::default();
    let repo = repository::create_repository(
        config.repository_type,
        config.portfolio_csv.clone(),
        config.positions_csv.clone(),
        config.pnl_csv.clone(),
    )?;
    Pms::new(config, repo)
}

/// 创建带 CSV 持久化的 PMS。
pub fn create_csv_pms(
    portfolio_csv: std::path::PathBuf,
    positions_csv: std::path::PathBuf,
    pnl_csv: std::path::PathBuf,
) -> anyhow::Result<Pms> {
    let config = PmsConfig {
        repository_type: repository::RepositoryType::Csv,
        portfolio_csv: Some(portfolio_csv),
        positions_csv: Some(positions_csv),
        pnl_csv: Some(pnl_csv),
        initial_capital: 10_000.0,
        subscribe_to_oms: true,
    };
    let repo = repository::create_repository(
        config.repository_type,
        config.portfolio_csv.clone(),
        config.positions_csv.clone(),
        config.pnl_csv.clone(),
    )?;
    Pms::new(config, repo)
}

// ============================================================================
// 中文 tracing 初始化
// ============================================================================

/// 初始化 PMS 中文 tracing。
pub fn init_pms_logging(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("PM_PMS_LOG").unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .try_init();
}

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{AssetType, Direction, PositionStatus};
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn prelude_exports_compile() {
        let _pf = Portfolio::default_portfolio(Local::now());
        let _pos_status = PositionStatus::Open;
        let _at = AssetType::Prediction;
        let _dir = Direction::Yes;
        let _ = PnLReport::default();
        let _ = ValuationReport {
            position_value: 0.0,
            portfolio_value: 0.0,
            cash_value: 0.0,
            total_exposure: 0.0,
            nav: 0.0,
            market_value: 0.0,
            valued_at: Local::now(),
        };
    }

    #[test]
    fn default_factory_works() {
        let pms = create_default_pms().unwrap();
        assert!(pms.positions().is_empty());
        assert!(approx(pms.portfolio().initial_capital, 10_000.0));
    }

    #[test]
    fn handle_order_filled_creates_position() {
        let mut pms = create_default_pms().unwrap();
        pms.handle_order_filled("OMS-001", "mkt-btc", Direction::Yes, Side::Buy, 0.50, 200.0)
            .unwrap();

        assert_eq!(pms.positions().len(), 1);
        assert_eq!(pms.positions()[0].market_id, "mkt-btc");
        assert!(approx(pms.positions()[0].quantity, 200.0));
        // 组合扣款：10000 - 100 = 9900
        assert!(approx(pms.portfolio().available_cash, 10000.0 - 100.0));
    }

    #[test]
    fn handle_order_filled_adds_to_existing() {
        let mut pms = create_default_pms().unwrap();
        // 第一笔
        pms.handle_order_filled("OMS-001", "mkt-btc", Direction::Yes, Side::Buy, 0.50, 100.0)
            .unwrap();
        // 第二笔：同一市场 + 同一方向
        pms.handle_order_filled("OMS-002", "mkt-btc", Direction::Yes, Side::Buy, 0.60, 100.0)
            .unwrap();

        assert_eq!(pms.positions().len(), 1);
        let pos = &pms.positions()[0];
        assert!(approx(pos.quantity, 200.0));
        assert!(approx(pos.average_price, 0.55));
        assert_eq!(pos.order_ids.len(), 2);
    }

    #[test]
    fn handle_order_filled_different_direction_new_position() {
        let mut pms = create_default_pms().unwrap();
        pms.handle_order_filled("OMS-001", "mkt-btc", Direction::Yes, Side::Buy, 0.50, 100.0)
            .unwrap();
        pms.handle_order_filled("OMS-002", "mkt-btc", Direction::No, Side::Buy, 0.40, 100.0)
            .unwrap();
        // Yes 和 No 是不同的持仓
        assert_eq!(pms.positions().len(), 2);
    }

    #[test]
    fn revalue_updates_portfolio_indicators() {
        let mut pms = create_default_pms().unwrap();
        let now = Local::now();
        // 开仓
        pms.handle_order_filled("OMS-001", "mkt-btc", Direction::Yes, Side::Buy, 0.50, 200.0)
            .unwrap();

        // mark-to-market：价格上涨
        pms.position_mgr
            .mark_position("mkt-btc", Direction::Yes, 0.60, now);

        pms.revalue_all(now);
        let pf = pms.portfolio();
        // 未实现盈亏 = 200 * (0.60 - 0.50) = 20
        assert!(approx(pf.unrealized_pnl, 20.0));
        assert!(approx(pf.total_pnl, 20.0));
    }

    #[test]
    fn handle_order_cancelled_does_not_panic() {
        let mut pms = create_default_pms().unwrap();
        pms.handle_order_filled("OMS-001", "mkt-btc", Direction::Yes, Side::Buy, 0.50, 100.0)
            .unwrap();
        pms.handle_order_cancelled("OMS-002", "用户取消");
        // 不应 panic
    }

    #[test]
    fn generate_reports_all_work() {
        let mut pms = create_default_pms().unwrap();
        pms.handle_order_filled("OMS-001", "mkt-btc", Direction::Yes, Side::Buy, 0.50, 100.0)
            .unwrap();

        let pnl = pms.generate_pnl_report();
        assert!(pnl.total_trades == 0); // 未平仓

        let val = pms.generate_valuation_report();
        assert!(val.nav > 0.0);

        let exp = pms.generate_exposure_report();
        assert!(exp.long_exposure > 0.0);

        let metrics = pms.generate_metrics();
        assert_eq!(metrics.position_count, 1);
    }

    #[test]
    fn print_functions_do_not_panic() {
        let mut pms = create_default_pms().unwrap();
        pms.handle_order_filled("OMS-001", "mkt-btc", Direction::Yes, Side::Buy, 0.50, 100.0)
            .unwrap();
        // 验证 print_* 不 panic
        pms.print_portfolio();
        pms.print_positions();
        pms.print_pnl();
        pms.print_exposure();
        pms.print_dashboard();
    }

    #[test]
    fn csv_factory_with_temp_paths() {
        let dir = std::env::temp_dir();
        let pf = dir.join("test_pms_portfolio.csv");
        let pos = dir.join("test_pms_positions.csv");
        let pnl = dir.join("test_pms_pnl.csv");
        let pms = create_csv_pms(pf, pos, pnl).unwrap();
        assert!(pms.positions().is_empty());
    }
}
