//! pm-paper：Paper Trading 引擎 + 历史回放（Simulation Only）。
//!
//! 真实行情 + 模拟订单。绝不连接钱包 / 发送订单 / 签名 / 上链。
//! - 新机会 -> 自动 BUY -> 立即模拟成交 -> 生成 Position -> 资金减少。
//! - 每轮 mark-to-market。
//! - 机会结束 -> 自动 SELL -> 计算 realized_pnl -> 更新 Portfolio。
//! - 风控委托 [`pm_portfolio::RiskManager`]。
//! - CSV：paper_orders.csv / paper_positions.csv / paper_portfolio.csv（复用 [`pm_storage`]）。
//!
//! 另提供 [`history::run_paper_history`]：回放 `opportunities.csv` 走 PaperTradingEngine（`cargo run -- paper`）。

pub mod engine;
pub mod history;
pub mod records;

pub use engine::{CloseOutcome, OpenOutcome, PaperTradingEngine};
pub use history::{paper_backtest, run_paper_history, PaperHistoryReport};
pub use records::{
    append_orders, append_portfolio, append_positions, ensure_csv, load_order_base,
    OrderRecord, PortfolioRecord, PositionRecord,
};
