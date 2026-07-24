//! Gateway Diagnostics（V1.08 第六节）。
//!
//! 诊断输出：连接状态 / API 延迟 / WebSocket / Rate Limit。
//! 全部中文。

use crate::config::GatewayConfig;
use crate::metrics::GatewayMetrics;
use crate::retry::CircuitBreaker;
use crate::traits::ExchangeGateway;

/// Gateway 诊断信息（中文）。
pub async fn diagnose_gateway(gateway: &dyn ExchangeGateway) -> String {
    let info = gateway.health().await;

    let live_status = if gateway.live_enabled() {
        "⚠️ 真实交易已启用"
    } else {
        "🔒 DryRun 模式（禁止真实下单）"
    };

    let health_status = if info.healthy {
        "✅ 健康"
    } else {
        "❌ 异常"
    };

    let ws_status = if info.ws_connected {
        "✅ 已连接"
    } else {
        "❌ 未连接"
    };

    format!(
        "══════════════════════════════════════\n\
         【Gateway 诊断】{}\n\
         ══════════════════════════════════════\n\
         \n\
          名称           : {}\n\
          类型           : {}\n\
          模式           : {}\n\
          健康状态       : {}\n\
         \n\
         ── 连接 ──\n\
          API 延迟       : {} ms\n\
          HTTP 成功率    : {:.1}%\n\
          WebSocket      : {}\n\
          Rate Limit     : {:.0}% 剩余\n\
         \n\
         ── 订单 ──\n\
          总提交         : {}\n\
          总成交         : {}\n\
         \n\
         ── 状态 ──\n\
          {}\n\
         ══════════════════════════════════════",
        "",
        info.name,
        info.gateway_type,
        live_status,
        health_status,
        info.api_latency_ms,
        info.http_success_rate * 100.0,
        ws_status,
        info.rate_limit_remaining,
        info.total_orders,
        info.total_fills,
        info.connection_status,
    )
}

/// 账户诊断（中文）。
pub async fn diagnose_account(gateway: &dyn ExchangeGateway) -> String {
    let balance = match gateway.get_balance().await {
        Ok(b) => b,
        Err(e) => {
            return format!("【账户诊断】❌ 获取余额失败: {}", e);
        }
    };

    let positions = match gateway.get_positions().await {
        Ok(p) => p,
        Err(e) => {
            return format!("【账户诊断】❌ 获取持仓失败: {}", e);
        }
    };

    let mut lines = vec![
        "══════════════════════════════════════".to_string(),
        "【账户诊断】".to_string(),
        "══════════════════════════════════════".to_string(),
        String::new(),
        format!("  账户 ID        : {}", balance.account_id),
        format!("  货币           : {}", balance.currency),
        String::new(),
        "── 余额 ──".to_string(),
        format!(
            "  可用余额       : {:.2} {}",
            balance.available, balance.currency
        ),
        format!(
            "  总余额         : {:.2} {}",
            balance.total, balance.currency
        ),
        format!(
            "  已占用         : {:.2} {}",
            balance.locked, balance.currency
        ),
        format!(
            "  未实现盈亏     : {:.2} {}",
            balance.unrealized_pnl, balance.currency
        ),
        format!(
            "  已实现盈亏     : {:.2} {}",
            balance.realized_pnl, balance.currency
        ),
        String::new(),
        format!("── 持仓（{} 个）──", positions.len()),
    ];

    if positions.is_empty() {
        lines.push("  （无持仓）".to_string());
    } else {
        for pos in &positions {
            lines.push(format!("  {}", pos.summary_zh()));
        }
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.join("\n")
}

/// 余额诊断（中文）。
pub async fn diagnose_balance(gateway: &dyn ExchangeGateway) -> String {
    let balance = match gateway.get_balance().await {
        Ok(b) => b,
        Err(e) => {
            return format!("【余额诊断】❌ 获取失败: {}", e);
        }
    };

    format!(
        "══════════════════════════════════════\n\
         【余额诊断】\n\
         ══════════════════════════════════════\n\
         \n\
          账户 ID        : {}\n\
          货币           : {}\n\
         \n\
          可用余额       : {:.2} {}\n\
          总余额         : {:.2} {}\n\
          已占用         : {:.2} {}\n\
          未实现盈亏     : {:.2} {}\n\
          已实现盈亏     : {:.2} {}\n\
         \n\
          更新时间       : {}\n\
         ══════════════════════════════════════",
        balance.account_id,
        balance.currency,
        balance.available,
        balance.currency,
        balance.total,
        balance.currency,
        balance.locked,
        balance.currency,
        balance.unrealized_pnl,
        balance.currency,
        balance.realized_pnl,
        balance.currency,
        balance
            .updated_at
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "未知".to_string()),
    )
}

/// 订单诊断（中文）。
pub async fn diagnose_orders(gateway: &dyn ExchangeGateway) -> String {
    let orders = gateway.list_orders().await;

    let mut lines = vec![
        "══════════════════════════════════════".to_string(),
        "【订单诊断】".to_string(),
        "══════════════════════════════════════".to_string(),
        String::new(),
        format!("  活跃订单数     : {}", orders.len()),
    ];

    if orders.is_empty() {
        lines.push(String::new());
        lines.push("  （无活跃订单）".to_string());
    } else {
        lines.push(String::new());
        for (i, result) in orders.iter().enumerate() {
            lines.push(format!(
                "  {}. {} | 状态: {} | 成交: {:.2} | 消息: {}",
                i + 1,
                result.gateway_order_id,
                result.status.as_zh(),
                result.filled,
                result.message,
            ));
        }
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.join("\n")
}

/// 配置诊断（中文）。
pub fn diagnose_config(cfg: &GatewayConfig) -> String {
    cfg.summary_zh()
}

/// 指标诊断（中文）。
pub fn diagnose_metrics(metrics: &GatewayMetrics) -> String {
    metrics.report_zh()
}

/// Prometheus 指标诊断（P2-03）。
///
/// 显示 Prometheus 风格的指标输出（文本格式）。
pub fn diagnose_prometheus(
    metrics: &crate::metrics::prometheus::GatewayPrometheusMetrics,
) -> String {
    metrics.to_prometheus_text()
}

/// 扩展健康诊断（P2-03）。
///
/// 显示完整的 Gateway 健康状态，包括：
/// - 综合状态
/// - REST 状态
/// - WebSocket 状态
/// - API 延迟（平均/最小/最大/最近）
/// - Rate Limit 状态
pub async fn diagnose_health_extended(gateway: &dyn crate::traits::ExchangeGateway) -> String {
    let info = gateway.health().await;

    let live_status = if info.live_enabled {
        "⚠️ 真实交易已启用"
    } else {
        "🔒 DryRun 模式（禁止真实下单）"
    };

    let health_status = if info.healthy {
        "✅ 健康"
    } else {
        "❌ 异常"
    };

    let ws_status = if info.ws_connected {
        "✅ 已连接"
    } else {
        "❌ 未连接（占位实现）"
    };

    format!(
        "═══════════════════════════════════════════════════════════════\n\
          【Gateway 健康状态】{} ({})\n\
         ═══════════════════════════════════════════════════════════════\n\
         \n\
          综合状态       : {}\n\
          模式           : {}\n\
         \n\
          ── REST 状态 ──\n\
            延迟         : {} ms\n\
            HTTP 成功率  : {:.1}%\n\
         \n\
          ── WebSocket 状态 ──\n\
            状态         : {}\n\
         \n\
          ── 速率限制 ──\n\
            剩余比例     : {:.0}%\n\
         \n\
          ── 累计统计 ──\n\
            订单总数     : {}（提交 / {} 成交）\n\
         \n\
          ── 连接状态 ──\n\
            {}\n\
         ═══════════════════════════════════════════════════════════════",
        info.name,
        info.gateway_type,
        health_status,
        live_status,
        info.api_latency_ms,
        info.http_success_rate * 100.0,
        ws_status,
        info.rate_limit_remaining,
        info.total_orders,
        info.total_fills,
        info.connection_status,
    )
}

/// 断路器诊断（中文）。
pub fn diagnose_circuit_breaker(breaker: &CircuitBreaker) -> String {
    format!(
        "══════════════════════════════════════\n\
         【断路器诊断】\n\
         ══════════════════════════════════════\n\
         \n\
          {}\n\
         ══════════════════════════════════════",
        breaker.stats_zh(),
    )
}
