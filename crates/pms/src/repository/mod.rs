//! PortfolioRepository — 持久化仓库 trait（P2-05 第九节）。
//!
//! 支持 Memory / CSV 实现，SQLite 接口预留。

pub mod csv;
pub mod memory;

use crate::domain::{PnLReport, Portfolio, Position};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryType {
    Memory,
    Csv,
    Sqlite,
}

#[derive(Debug, Clone)]
pub struct RepositoryHealth {
    pub healthy: bool,
    pub error: Option<String>,
    pub portfolio_count: u64,
    pub position_count: u64,
}

impl RepositoryHealth {
    pub fn summary_zh(&self) -> String {
        format!(
            "健康={} 组合={} 持仓={} {}",
            if self.healthy { "✅" } else { "❌" },
            self.portfolio_count,
            self.position_count,
            self.error.as_deref().unwrap_or(""),
        )
    }
}

/// 投资组合持久化仓库 trait。
pub trait PortfolioRepository: Send + Sync {
    fn save_portfolio(&self, portfolio: &Portfolio) -> anyhow::Result<()>;
    fn load_portfolio(&self) -> anyhow::Result<Option<Portfolio>>;
    fn save_positions(&self, positions: &[Position]) -> anyhow::Result<()>;
    fn load_positions(&self) -> anyhow::Result<Vec<Position>>;
    fn append_position(&self, position: &Position) -> anyhow::Result<()>;
    fn save_pnl_report(&self, report: &PnLReport) -> anyhow::Result<()>;
    fn load_pnl_reports(&self) -> anyhow::Result<Vec<PnLReport>>;
    fn name(&self) -> &str;
    fn storage_path(&self) -> Option<PathBuf>;
    fn health(&self) -> RepositoryHealth;
    fn clear(&self) -> anyhow::Result<()>;
}

pub fn create_repository(
    repo_type: RepositoryType,
    portfolio_csv: Option<PathBuf>,
    positions_csv: Option<PathBuf>,
    pnl_csv: Option<PathBuf>,
) -> anyhow::Result<Box<dyn PortfolioRepository>> {
    match repo_type {
        RepositoryType::Memory => {
            tracing::info!("创建 Memory 仓库");
            Ok(Box::new(memory::InMemoryPortfolioRepository::new()))
        }
        RepositoryType::Csv => {
            let pf_path = portfolio_csv.unwrap_or_else(|| PathBuf::from("data/pms_portfolio.csv"));
            let pos_path = positions_csv.unwrap_or_else(|| PathBuf::from("data/pms_positions.csv"));
            let pnl_path = pnl_csv.unwrap_or_else(|| PathBuf::from("data/pms_pnl.csv"));
            tracing::info!(
                portfolio_csv = %pf_path.display(),
                positions_csv = %pos_path.display(),
                "创建 CSV 仓库"
            );
            Ok(Box::new(csv::CsvPortfolioRepository::new(
                pf_path, pos_path, pnl_path,
            )))
        }
        RepositoryType::Sqlite => {
            anyhow::bail!("SQLite 仓库尚未实现（接口预留）")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_memory_repository() {
        let repo = create_repository(RepositoryType::Memory, None, None, None).unwrap();
        assert_eq!(repo.name(), "InMemory");
        assert!(repo.health().healthy);
    }

    #[test]
    fn sqlite_not_implemented() {
        assert!(create_repository(RepositoryType::Sqlite, None, None, None).is_err());
    }
}
