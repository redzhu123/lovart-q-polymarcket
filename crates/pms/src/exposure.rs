//! ExposureEngine — 风险敞口引擎（P2-05 第七节）。
//!
//! 计算：
//! - 多头/空头敞口
//! - 按资产类型分类敞口（Prediction/Spot/AMM/Perpetual）
//! - 单市场敞口
//! - 资产配置

use crate::domain::{
    AssetAllocation, AssetType, ExposureReport, MarketExposure, Portfolio, Position, PositionStatus,
};
use chrono::{DateTime, Local};
use pm_core::Side;
use tracing;

/// 风险敞口引擎。
pub struct ExposureEngine {
    // Reserve for future configuration
}

impl ExposureEngine {
    pub fn new() -> Self {
        tracing::info!("风险敞口引擎初始化");
        Self {}
    }

    /// 计算风险敞口报告。
    pub fn calculate(
        &self,
        positions: &[Position],
        portfolio: &Portfolio,
        now: DateTime<Local>,
    ) -> ExposureReport {
        let active: Vec<&Position> = positions
            .iter()
            .filter(|p| p.status == PositionStatus::Open)
            .collect();

        // 多空敞口
        let long_exposure: f64 = active
            .iter()
            .filter(|p| p.side == Side::Buy)
            .map(|p| p.market_value)
            .sum();
        let short_exposure: f64 = active
            .iter()
            .filter(|p| p.side == Side::Sell)
            .map(|p| p.market_value)
            .sum();
        let net_exposure = long_exposure - short_exposure;

        // 按资产类型分类
        let prediction_exposure = type_exposure(&active, AssetType::Prediction);
        let spot_exposure = type_exposure(&active, AssetType::Spot);
        let amm_exposure = type_exposure(&active, AssetType::AMM);
        let perpetual_exposure = type_exposure(&active, AssetType::Perpetual);

        // 单市场敞口
        let mut market_map: std::collections::HashMap<String, (f64, f64)> =
            std::collections::HashMap::new();
        for pos in &active {
            let entry = market_map
                .entry(pos.market_id.clone())
                .or_insert((0.0, 0.0));
            if pos.side == Side::Buy {
                entry.0 += pos.market_value;
            } else {
                entry.1 += pos.market_value;
            }
        }
        let market_exposures: Vec<MarketExposure> = market_map
            .into_iter()
            .map(|(market_id, (long, short))| MarketExposure {
                market_id,
                long_exposure: long,
                short_exposure: short,
                net_exposure: long - short,
            })
            .collect();

        // 资产配置
        let total_value = portfolio.total_assets.max(f64::EPSILON);
        let asset_allocation = vec![
            AssetAllocation {
                asset_type: AssetType::Prediction,
                value: prediction_exposure,
                percentage: prediction_exposure / total_value,
            },
            AssetAllocation {
                asset_type: AssetType::Spot,
                value: spot_exposure,
                percentage: spot_exposure / total_value,
            },
            AssetAllocation {
                asset_type: AssetType::AMM,
                value: amm_exposure,
                percentage: amm_exposure / total_value,
            },
            AssetAllocation {
                asset_type: AssetType::Perpetual,
                value: perpetual_exposure,
                percentage: perpetual_exposure / total_value,
            },
        ];

        tracing::debug!(
            long_exposure = %long_exposure,
            short_exposure = %short_exposure,
            net_exposure = %net_exposure,
            market_count = %market_exposures.len(),
            "风险敞口计算完成"
        );

        ExposureReport {
            long_exposure,
            short_exposure,
            net_exposure,
            prediction_exposure,
            spot_exposure,
            amm_exposure,
            perpetual_exposure,
            market_exposures,
            asset_allocation,
            reported_at: now,
        }
    }

    /// 中文打印风险敞口报告。
    pub fn print_zh(&self, report: &ExposureReport) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  风险敞口报告 (Exposure Report)");
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("  ── 多空敞口 ──");
        println!("  多头敞口      : {:.2} USDC", report.long_exposure);
        println!("  空头敞口      : {:.2} USDC", report.short_exposure);
        println!("  净敞口        : {:+.2} USDC", report.net_exposure);
        println!();
        println!("  ── 资产类型敞口 ──");
        println!("  预测市场      : {:.2} USDC", report.prediction_exposure);
        println!("  现货          : {:.2} USDC", report.spot_exposure);
        println!("  AMM           : {:.2} USDC", report.amm_exposure);
        println!("  永续合约      : {:.2} USDC", report.perpetual_exposure);
        println!();

        if !report.market_exposures.is_empty() {
            println!("  ── 单市场敞口 ──");
            println!(
                "  {:<25} {:<12} {:<12} {:<12}",
                "市场", "多头", "空头", "净敞口"
            );
            println!("  {}", "─".repeat(61));
            for m in &report.market_exposures {
                println!(
                    "  {:<25} {:<12.2} {:<12.2} {:<12.2}",
                    truncate(&m.market_id, 25),
                    m.long_exposure,
                    m.short_exposure,
                    m.net_exposure,
                );
            }
            println!();
        }

        println!("  ── 资产配置 ──");
        for a in &report.asset_allocation {
            if a.value > f64::EPSILON {
                println!(
                    "  {}: {:.2} USDC ({:.1}%)",
                    a.asset_type.as_zh(),
                    a.value,
                    a.percentage * 100.0,
                );
            }
        }
        println!();
        println!(
            "  报告时间: {}",
            report.reported_at.format("%Y-%m-%d %H:%M:%S")
        );
        println!();
    }
}

/// 按资产类型汇总敞口。
fn type_exposure(positions: &[&Position], asset_type: AssetType) -> f64 {
    positions
        .iter()
        .filter(|p| p.asset_type == asset_type)
        .map(|p| p.market_value)
        .sum()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

impl Default for ExposureEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AssetType, Direction};
    use pm_core::Side;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn empty_positions_zero_exposure() {
        let engine = ExposureEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let report = engine.calculate(&[], &pf, now);
        assert!(approx(report.long_exposure, 0.0));
        assert!(approx(report.net_exposure, 0.0));
    }

    #[test]
    fn long_position_exposure() {
        let engine = ExposureEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            200.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        let report = engine.calculate(&[pos], &pf, now);
        assert!(approx(report.long_exposure, 100.0));
        assert!(approx(report.short_exposure, 0.0));
        assert!(approx(report.prediction_exposure, 100.0));
        assert_eq!(report.market_exposures.len(), 1);
    }

    #[test]
    fn multiple_positions_aggregate() {
        let engine = ExposureEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let p1 = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        let p2 = Position::open(
            "POS-002".into(),
            "mkt-eth".into(),
            AssetType::Prediction,
            Direction::No,
            Side::Buy,
            200.0,
            0.40,
            "OMS-002".into(),
            now,
        );
        let report = engine.calculate(&[p1, p2], &pf, now);
        assert!(approx(report.long_exposure, 130.0)); // 50 + 80
        assert_eq!(report.market_exposures.len(), 2);
    }

    #[test]
    fn asset_allocation_adds_up() {
        let engine = ExposureEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        let report = engine.calculate(&[pos], &pf, now);
        let total_pct: f64 = report.asset_allocation.iter().map(|a| a.percentage).sum();
        // Should be ~ 50/(10000+50)
        assert!(total_pct > 0.0);
    }

    #[test]
    fn print_zh_does_not_panic() {
        let engine = ExposureEngine::new();
        let pf = Portfolio::default_portfolio(Local::now());
        let now = Local::now();
        let pos = Position::open(
            "POS-001".into(),
            "mkt-btc".into(),
            AssetType::Prediction,
            Direction::Yes,
            Side::Buy,
            100.0,
            0.50,
            "OMS-001".into(),
            now,
        );
        let report = engine.calculate(&[pos], &pf, now);
        engine.print_zh(&report);
    }
}
