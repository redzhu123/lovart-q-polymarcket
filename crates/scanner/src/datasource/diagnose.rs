//! 数据源诊断模式（V1.02 第十一节）-- `cargo run -- datasource`。
//!
//! 快速检查数据源：Provider / Capability / Health / Latency / Market Count /
//! Cache / Validator / Snapshot。不进入扫描循环、不清屏。
//! 使用 `mock` Provider 时无需网络即可演示。

use std::time::Instant;

use anyhow::Result;
use chrono::Local;

use pm_models::Config;

use crate::datasource::{DataSourceManager, MarketSnapshot, Validator};
use crate::display::{DASH, SEP};

/// `datasource` 模式入口：输出数据源完整诊断。
pub async fn run_datasource_diagnose(cfg: &Config) -> Result<()> {
    println!("{}", SEP);
    println!();
    println!("🔍 数据源诊断");
    println!();
    println!("仅模拟 -- 检查数据源 Provider / 能力 / 健康 / 延迟 / 市场数 / 缓存 / 校验 / 快照");
    println!();
    println!("{}", SEP);
    println!();

    let mut manager = DataSourceManager::from_config(cfg)?;

    // ---- Provider + Capability ----
    println!("{}", DASH);
    println!();
    println!("Provider");
    println!();
    println!("名称: {}", manager.name());
    println!();
    manager.print_capability_block();

    // ---- Health + Latency ----
    let probe = match manager.health_check().await {
        Ok(p) => p,
        Err(e) => {
            println!("{}", DASH);
            println!();
            println!("🔴 健康检查失败: {:#}", e);
            println!();
            return Ok(());
        }
    };
    println!("{}", DASH);
    println!();
    println!("Health / Latency");
    println!();
    println!("健康    : {}", if probe.ok { "✅ 正常" } else { "❌ 异常" });
    println!("HTTP    : {}", probe.status);
    println!("延迟    : {} 毫秒", probe.latency_ms);
    println!("市场数  : {}", probe.market_count);
    println!("细节    : {}", probe.detail);
    println!();

    // ---- Market Count（带 fetch 计时）----
    let fetch_start = Instant::now();
    let outcome = match manager.fetch_markets().await {
        Ok(o) => o,
        Err(e) => {
            println!("{}", DASH);
            println!();
            println!("🔴 数据拉取失败: {:#}", e);
            println!();
            return Ok(());
        }
    };
    let fetch_ms = fetch_start.elapsed().as_millis();
    println!("{}", DASH);
    println!();
    println!("Market Count");
    println!();
    println!("市场数      : {}", outcome.markets.len());
    println!(
        "来源        : {}",
        if outcome.cached {
            "缓存命中"
        } else {
            "Provider 新拉取"
        }
    );
    println!("拉取耗时    : {} 毫秒", fetch_ms);
    println!();

    // ---- Cache ----
    let cache = manager.cache_info();
    println!("{}", DASH);
    println!();
    println!("Cache");
    println!();
    println!("缓存市场数  : {}", cache.size);
    println!("TTL         : {} 秒", cache.ttl_secs);
    println!(
        "是否新鲜    : {}",
        if cache.fresh { "✅ 是" } else { "❌ 否" }
    );
    println!();

    // ---- Validator ----
    let report = Validator::validate_many(&outcome.markets);
    println!("{}", DASH);
    println!();
    println!("Validator");
    println!();
    println!("校验总数    : {}", report.total);
    println!("合法        : {}", report.valid);
    println!("非法        : {}", report.invalid);
    println!("非法率      : {:.2}%", report.invalid_rate() * 100.0);
    if !report.errors.is_empty() {
        println!("非法明细（前 10）:");
        for (id, e) in report.errors.iter().take(10) {
            println!("  - {} : {} ({})", id, e.field, e.reason);
        }
    }
    println!();

    // ---- OrderBook（若 Provider 支持，取前 5 个市场演示）----
    let cap = manager.capability();
    if cap.supports_orderbook {
        let ids: Vec<String> = outcome
            .markets
            .iter()
            .take(5)
            .map(|m| m.market_id.clone())
            .collect();
        let obs = manager
            .provider()
            .fetch_orderbooks(&ids)
            .await
            .unwrap_or_default();
        let with_bid = obs.iter().filter(|o| o.best_bid.is_some()).count();
        println!("{}", DASH);
        println!();
        println!("OrderBook（前 5 个市场演示）");
        println!();
        println!("请求        : {}", ids.len());
        println!("返回        : {}", obs.len());
        println!("有买一价    : {}", with_bid);
        println!();
    } else {
        println!("{}", DASH);
        println!();
        println!("OrderBook");
        println!();
        println!("当前 Provider 不支持订单簿，跳过。");
        println!();
    }

    // ---- Snapshot ----
    let now = Local::now();
    let snapshot = MarketSnapshot::from_markets(&outcome.markets, manager.name(), now);
    snapshot.print_block();

    // 持久化快照到 <data_dir>/market_snapshots.csv
    let snap_path = format!("{}/market_snapshots.csv", cfg.paths.data_dir);
    match snapshot.save_to_csv(&snap_path) {
        Ok(()) => {
            println!("快照已保存: {}", snap_path);
            println!();
        }
        Err(e) => {
            println!("快照保存失败: {:#}", e);
            println!();
        }
    }

    println!("{}", SEP);
    println!();
    println!("数据源诊断完成 -- 仅模拟");
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn datasource_diagnose_mock_runs_without_network() {
        // mock Provider 无需网络即可完整跑通数据源诊断。
        let mut cfg = Config::default();
        cfg.datasource.provider = "mock".into();
        // 不 panic 即通过（输出写到 stdout）。
        let _ = run_datasource_diagnose(&cfg).await;
    }
}
