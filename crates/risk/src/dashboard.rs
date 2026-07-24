//! Risk Dashboard（V1.05 第十节）。
//!
//! CLI `cargo run -- risk` 输出当前风险状态。
//! 全部中文。

use crate::context::RiskContext;
use crate::engine::RiskEngine;
use crate::exposure::ExposureReport;

/// 风险仪表盘。
pub struct RiskDashboard;

impl RiskDashboard {
    /// 渲染完整风险仪表盘。
    pub fn render(engine: &RiskEngine, ctx: &RiskContext, exposure: &ExposureReport) -> String {
        let mut lines = Vec::new();

        // 标题
        lines.push("╔══════════════════════════════════════════════╗".to_string());
        lines.push("║          【风险仪表盘】V1.05                ║".to_string());
        lines.push("╚══════════════════════════════════════════════╝".to_string());
        lines.push(String::new());

        // 风险等级
        let report = engine.portfolio_risk_report(ctx);
        lines.push(format!(
            "  {} 风险等级：{}",
            report.risk.risk_level.as_color(),
            report.risk.risk_level.as_zh()
        ));
        lines.push(String::new());

        // 组合风险
        lines.push("  ── 组合风险 ──".to_string());
        lines.push(format!(
            "  资金利用率：  {:.0}%",
            report.risk.capital_usage * 100.0
        ));
        lines.push(format!(
            "  现金比例：    {:.0}%",
            report.risk.cash_ratio * 100.0
        ));
        lines.push(format!(
            "  风险暴露：    {:.0}%",
            report.risk.exposure_ratio * 100.0
        ));
        lines.push(format!(
            "  最大回撤：    {:.1}%（当前 {:.1}%）",
            report.risk.max_drawdown * 100.0,
            report.risk.current_drawdown * 100.0
        ));
        lines.push(format!("  总 ROI：      {:.2}%", report.risk.roi * 100.0));
        lines.push(format!("  当日盈亏：    {:.0} USDC", report.risk.daily_pnl));
        lines.push(String::new());

        // 交易统计
        lines.push("  ── 交易统计 ──".to_string());
        lines.push(format!("  持仓数量：    {}", ctx.open_position_count));
        lines.push(format!("  待处理订单：  {}", ctx.pending_order_count));
        lines.push(format!(
            "  连续亏损：    {} 次",
            report.risk.consecutive_losses
        ));
        lines.push(String::new());

        // 风险审核统计
        let stats = engine.stats();
        lines.push("  ── 风险审核 ──".to_string());
        lines.push(format!("  总评估：      {} 次", stats.total_evaluations));
        lines.push(format!("  接受：        {} 次", stats.accepted));
        lines.push(format!("  需审核：      {} 次", stats.reviewed));
        lines.push(format!("  拒绝：        {} 次", stats.rejected));
        lines.push(format!("  风险事件：    {} 个", engine.events().total()));
        lines.push(String::new());

        // 暴露
        lines.push("  ── 暴露 ──".to_string());
        lines.push(format!(
            "  YES 暴露：    {:.0} USDC（{:.1}%）",
            exposure.by_side.yes_exposure,
            if ctx.initial_capital > 0.0 {
                exposure.by_side.yes_exposure / ctx.initial_capital * 100.0
            } else {
                0.0
            }
        ));
        lines.push(format!(
            "  NO  暴露：    {:.0} USDC（{:.1}%）",
            exposure.by_side.no_exposure,
            if ctx.initial_capital > 0.0 {
                exposure.by_side.no_exposure / ctx.initial_capital * 100.0
            } else {
                0.0
            }
        ));
        lines.push(format!(
            "  总  暴露：    {:.0} USDC（{:.1}%）",
            exposure.by_side.total_exposure,
            exposure.total_exposure_ratio() * 100.0
        ));
        lines.push(String::new());

        // 配置摘要
        lines.push("  ── 风险配置 ──".to_string());
        lines.push(format!(
            "  仓位策略：    {}",
            engine.config().position_sizer.as_zh()
        ));
        lines.push(format!(
            "  固定仓位：    {:.0} USDC",
            engine.config().fixed_size
        ));
        lines.push(format!(
            "  最大持仓：    {} 个",
            engine.config().max_positions
        ));
        lines.push(format!(
            "  日亏损上限：  {:.0} USDC",
            engine.config().max_daily_loss
        ));
        lines.push(format!(
            "  连续亏损上限：{} 次",
            engine.config().max_consecutive_losses
        ));
        lines.push(format!(
            "  最大回撤：    {:.0}%",
            engine.config().max_drawdown * 100.0
        ));
        lines.push(String::new());

        // 活跃警告
        if !report.details.is_empty() {
            lines.push("  ── ⚠️ 活跃警告 ──".to_string());
            for d in &report.details {
                lines.push(format!("  - {}", d));
            }
            lines.push(String::new());
        }

        // 风险事件摘要
        if engine.events().total() > 0 {
            lines.push("  ── 风险事件 ──".to_string());
            lines.push(engine.events().summary_zh());
            lines.push(String::new());
        }

        lines.push("══════════════════════════════════════════════".to_string());
        lines.push("  仅模拟 -- 无钱包 / 无下单 / 无签名".to_string());

        lines.join("\n")
    }

    /// 简短摘要（每轮扫描后显示）。
    pub fn summary_line(engine: &RiskEngine, ctx: &RiskContext) -> String {
        let report = engine.portfolio_risk_report(ctx);
        format!(
            "{} 风险:{} | 资金:{:.0}% | 回撤:{:.1}% | 暴露:{:.0}% | 连续亏损:{} | 拒绝:{}",
            report.risk.risk_level.as_color(),
            report.risk.risk_level.as_zh(),
            report.risk.capital_usage * 100.0,
            report.risk.current_drawdown * 100.0,
            report.risk.exposure_ratio * 100.0,
            report.risk.consecutive_losses,
            engine.total_rejections(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::RiskContext;
    use chrono::Local;

    #[test]
    fn dashboard_renders_all_sections() {
        let ctx = RiskContext::minimal(10000.0, 9000.0, Local::now());
        let engine = RiskEngine::with_defaults();
        let exposure = ExposureReport::new(10000.0);
        let dash = RiskDashboard::render(&engine, &ctx, &exposure);
        assert!(dash.contains("风险仪表盘"));
        assert!(dash.contains("组合风险"));
        assert!(dash.contains("交易统计"));
        assert!(dash.contains("风险审核"));
        assert!(dash.contains("暴露"));
        assert!(dash.contains("风险配置"));
        assert!(dash.contains("仅模拟"));
    }

    #[test]
    fn summary_line_contains_key_info() {
        let ctx = RiskContext::minimal(10000.0, 9000.0, Local::now());
        let engine = RiskEngine::with_defaults();
        let line = RiskDashboard::summary_line(&engine, &ctx);
        assert!(line.contains("风险:"));
        assert!(line.contains("资金:"));
        assert!(line.contains("回撤:"));
        assert!(line.contains("拒绝:"));
    }
}
