//! CsvPortfolioRepository — CSV 文件持久化仓库。

use crate::domain::{PnLReport, Portfolio, Position};
use crate::repository::{PortfolioRepository, RepositoryHealth};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortfolioRecord {
    pub portfolio_id: String,
    pub name: String,
    pub initial_capital: f64,
    pub available_cash: f64,
    pub frozen_cash: f64,
    pub position_value: f64,
    pub total_assets: f64,
    pub total_equity: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub roi: f64,
    pub total_pnl: f64,
    pub updated_at: String,
}

impl From<&Portfolio> for PortfolioRecord {
    fn from(pf: &Portfolio) -> Self {
        Self {
            portfolio_id: pf.portfolio_id.clone(),
            name: pf.name.clone(),
            initial_capital: pf.initial_capital,
            available_cash: pf.available_cash,
            frozen_cash: pf.frozen_cash,
            position_value: pf.position_value,
            total_assets: pf.total_assets,
            total_equity: pf.total_equity,
            unrealized_pnl: pf.unrealized_pnl,
            realized_pnl: pf.realized_pnl,
            roi: pf.roi,
            total_pnl: pf.total_pnl,
            updated_at: pf.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PositionRecord {
    pub position_id: String,
    pub market_id: String,
    pub asset_type: String,
    pub direction: String,
    pub side: String,
    pub quantity: f64,
    pub average_price: f64,
    pub current_price: f64,
    pub market_value: f64,
    pub cost_basis: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub roi: f64,
    pub status: String,
    pub order_ids: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Position> for PositionRecord {
    fn from(pos: &Position) -> Self {
        Self {
            position_id: pos.position_id.clone(),
            market_id: pos.market_id.clone(),
            asset_type: pos.asset_type.as_str().to_string(),
            direction: pos.direction.as_zh().to_string(),
            side: pos.side.as_str().to_string(),
            quantity: pos.quantity,
            average_price: pos.average_price,
            current_price: pos.current_price,
            market_value: pos.market_value,
            cost_basis: pos.cost_basis,
            unrealized_pnl: pos.unrealized_pnl,
            realized_pnl: pos.realized_pnl,
            roi: pos.roi,
            status: pos.status.as_zh().to_string(),
            order_ids: pos.order_ids.join("|"),
            created_at: pos.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            updated_at: pos.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PnLRecord {
    pub timestamp: String,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub daily_pnl: f64,
    pub total_pnl: f64,
    pub roi: f64,
    pub win_rate: f64,
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub profit_factor: f64,
}

impl From<&PnLReport> for PnLRecord {
    fn from(r: &PnLReport) -> Self {
        Self {
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            realized_pnl: r.realized_pnl,
            unrealized_pnl: r.unrealized_pnl,
            daily_pnl: r.daily_pnl,
            total_pnl: r.total_pnl,
            roi: r.roi,
            win_rate: r.win_rate,
            total_trades: r.total_trades as u64,
            winning_trades: r.winning_trades as u64,
            losing_trades: r.losing_trades as u64,
            profit_factor: r.profit_factor,
        }
    }
}

pub struct CsvPortfolioRepository {
    portfolio_path: PathBuf,
    positions_path: PathBuf,
    pnl_path: PathBuf,
    portfolio_cache: Mutex<Option<Portfolio>>,
    positions_cache: Mutex<Vec<Position>>,
}

impl CsvPortfolioRepository {
    pub fn new(portfolio_path: PathBuf, positions_path: PathBuf, pnl_path: PathBuf) -> Self {
        if let Some(parent) = portfolio_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self {
            portfolio_path,
            positions_path,
            pnl_path,
            portfolio_cache: Mutex::new(None),
            positions_cache: Mutex::new(Vec::new()),
        }
    }

    fn write_portfolio_csv(&self, pf: &Portfolio) -> anyhow::Result<()> {
        let record = PortfolioRecord::from(pf);
        let mut wtr = csv::Writer::from_path(&self.portfolio_path)?;
        wtr.serialize(&record)?;
        wtr.flush()?;
        Ok(())
    }

    fn write_positions_csv(&self, positions: &[Position]) -> anyhow::Result<()> {
        let mut wtr = csv::Writer::from_path(&self.positions_path)?;
        for pos in positions {
            wtr.serialize(PositionRecord::from(pos))?;
        }
        wtr.flush()?;
        Ok(())
    }
}

impl PortfolioRepository for CsvPortfolioRepository {
    fn save_portfolio(&self, portfolio: &Portfolio) -> anyhow::Result<()> {
        self.write_portfolio_csv(portfolio)?;
        *self.portfolio_cache.lock().unwrap() = Some(portfolio.clone());
        tracing::debug!("投资组合已保存至 CSV: {}", self.portfolio_path.display());
        Ok(())
    }

    fn load_portfolio(&self) -> anyhow::Result<Option<Portfolio>> {
        if let Some(cached) = self.portfolio_cache.lock().unwrap().clone() {
            return Ok(Some(cached));
        }
        Ok(self.portfolio_cache.lock().unwrap().clone())
    }

    fn save_positions(&self, positions: &[Position]) -> anyhow::Result<()> {
        self.write_positions_csv(positions)?;
        *self.positions_cache.lock().unwrap() = positions.to_vec();
        tracing::debug!(
            "持仓已保存至 CSV: {} ({} 条)",
            self.positions_path.display(),
            positions.len()
        );
        Ok(())
    }

    fn load_positions(&self) -> anyhow::Result<Vec<Position>> {
        Ok(self.positions_cache.lock().unwrap().clone())
    }

    fn append_position(&self, position: &Position) -> anyhow::Result<()> {
        let mut pos = self.positions_cache.lock().unwrap();
        pos.push(position.clone());
        self.write_positions_csv(&pos)?;
        Ok(())
    }

    fn save_pnl_report(&self, report: &PnLReport) -> anyhow::Result<()> {
        let file_exists = self.pnl_path.exists();
        let mut wtr = if file_exists {
            csv::WriterBuilder::new()
                .has_headers(false)
                .from_path(&self.pnl_path)?
        } else {
            csv::Writer::from_path(&self.pnl_path)?
        };
        wtr.serialize(PnLRecord::from(report))?;
        wtr.flush()?;
        Ok(())
    }

    fn load_pnl_reports(&self) -> anyhow::Result<Vec<PnLReport>> {
        Ok(Vec::new()) // 简化：从 CSV 重建需要更多序列化逻辑
    }

    fn name(&self) -> &str {
        "Csv"
    }

    fn storage_path(&self) -> Option<PathBuf> {
        Some(self.portfolio_path.clone())
    }

    fn health(&self) -> RepositoryHealth {
        RepositoryHealth {
            healthy: true,
            error: None,
            portfolio_count: if self.portfolio_cache.lock().unwrap().is_some() {
                1
            } else {
                0
            },
            position_count: self.positions_cache.lock().unwrap().len() as u64,
        }
    }

    fn clear(&self) -> anyhow::Result<()> {
        *self.portfolio_cache.lock().unwrap() = None;
        self.positions_cache.lock().unwrap().clear();
        let _ = std::fs::remove_file(&self.portfolio_path);
        let _ = std::fs::remove_file(&self.positions_path);
        let _ = std::fs::remove_file(&self.pnl_path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AssetType, Direction};
    use chrono::Local;
    use pm_core::Side;

    fn temp_paths() -> (PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir();
        (
            dir.join("test_pms_pf.csv"),
            dir.join("test_pms_pos.csv"),
            dir.join("test_pms_pnl.csv"),
        )
    }

    #[test]
    fn save_and_load_portfolio_csv() {
        let (pf_path, pos_path, pnl_path) = temp_paths();
        let repo = CsvPortfolioRepository::new(pf_path.clone(), pos_path, pnl_path);
        let pf = Portfolio::default_portfolio(Local::now());
        repo.save_portfolio(&pf).unwrap();
        assert!(pf_path.exists());
        repo.clear().unwrap();
    }

    #[test]
    fn save_positions_csv() {
        let (pf_path, pos_path, pnl_path) = temp_paths();
        let repo = CsvPortfolioRepository::new(pf_path, pos_path, pnl_path);
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
        repo.clear().unwrap();
    }
}
