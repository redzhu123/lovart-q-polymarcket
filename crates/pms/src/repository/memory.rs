//! InMemoryPortfolioRepository — 内存仓库实现。

use crate::domain::{PnLReport, Portfolio, Position};
use crate::repository::{PortfolioRepository, RepositoryHealth};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct InMemoryPortfolioRepository {
    portfolio: Mutex<Option<Portfolio>>,
    positions: Mutex<Vec<Position>>,
    pnl_reports: Mutex<Vec<PnLReport>>,
}

impl InMemoryPortfolioRepository {
    pub fn new() -> Self {
        Self {
            portfolio: Mutex::new(None),
            positions: Mutex::new(Vec::new()),
            pnl_reports: Mutex::new(Vec::new()),
        }
    }
}

impl Default for InMemoryPortfolioRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl PortfolioRepository for InMemoryPortfolioRepository {
    fn save_portfolio(&self, portfolio: &Portfolio) -> anyhow::Result<()> {
        *self.portfolio.lock().unwrap() = Some(portfolio.clone());
        Ok(())
    }

    fn load_portfolio(&self) -> anyhow::Result<Option<Portfolio>> {
        Ok(self.portfolio.lock().unwrap().clone())
    }

    fn save_positions(&self, positions: &[Position]) -> anyhow::Result<()> {
        *self.positions.lock().unwrap() = positions.to_vec();
        Ok(())
    }

    fn load_positions(&self) -> anyhow::Result<Vec<Position>> {
        Ok(self.positions.lock().unwrap().clone())
    }

    fn append_position(&self, position: &Position) -> anyhow::Result<()> {
        self.positions.lock().unwrap().push(position.clone());
        Ok(())
    }

    fn save_pnl_report(&self, report: &PnLReport) -> anyhow::Result<()> {
        self.pnl_reports.lock().unwrap().push(report.clone());
        Ok(())
    }

    fn load_pnl_reports(&self) -> anyhow::Result<Vec<PnLReport>> {
        Ok(self.pnl_reports.lock().unwrap().clone())
    }

    fn name(&self) -> &str {
        "InMemory"
    }

    fn storage_path(&self) -> Option<PathBuf> {
        None
    }

    fn health(&self) -> RepositoryHealth {
        RepositoryHealth {
            healthy: true,
            error: None,
            portfolio_count: if self.portfolio.lock().unwrap().is_some() {
                1
            } else {
                0
            },
            position_count: self.positions.lock().unwrap().len() as u64,
        }
    }

    fn clear(&self) -> anyhow::Result<()> {
        *self.portfolio.lock().unwrap() = None;
        self.positions.lock().unwrap().clear();
        self.pnl_reports.lock().unwrap().clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AssetType, Direction};
    use chrono::Local;
    use pm_core::Side;

    #[test]
    fn save_and_load_portfolio() {
        let repo = InMemoryPortfolioRepository::new();
        let pf = Portfolio::default_portfolio(Local::now());
        repo.save_portfolio(&pf).unwrap();
        assert!(repo.load_portfolio().unwrap().is_some());
    }

    #[test]
    fn save_and_load_positions() {
        let repo = InMemoryPortfolioRepository::new();
        let now = Local::now();
        let pos = Position::open(
            "POS-001".into(),
            "mkt-1".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        repo.save_positions(&[pos]).unwrap();
        assert_eq!(repo.load_positions().unwrap().len(), 1);
    }

    #[test]
    fn clear_removes_all() {
        let repo = InMemoryPortfolioRepository::new();
        repo.save_portfolio(&Portfolio::default_portfolio(Local::now()))
            .unwrap();
        repo.clear().unwrap();
        assert!(repo.load_portfolio().unwrap().is_none());
        assert!(repo.load_positions().unwrap().is_empty());
    }
}
