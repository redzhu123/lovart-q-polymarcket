//! 历史回放：读取 opportunities.csv 全部已结束机会，按 start_time 排序后逐时间步回放。
//!
//! Simulation Only -- 仅展示历史机会的开/闭事件，不做交易计算（交易计算见 [`crate::backtest`]）。
//!
//! 数据局限：opportunities.csv 只保存机会生命周期汇总信息（start/end/best_sum/last_yes/last_no/scan_count），
//! 未保存逐轮扫描快照，因此回放按"机会在 [start_time, end_time] 区间内存活"近似还原，
//! 精度为 `Config.replay.step_secs` 秒一步。

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;

use pm_models::Config;

/// replay 模式入口：按时间步进回放历史机会。
pub async fn run_replay(cfg: &Config) -> Result<()> {
    let opps = pm_storage::load_sorted_opportunities(&cfg.paths.opportunities_csv)?;
    if opps.is_empty() {
        println!();
        println!("No replay data found in {}", cfg.paths.opportunities_csv);
        println!("Run `cargo run -- scan` first to collect opportunities.");
        return Ok(());
    }

    let step_secs = cfg.replay.step_secs.max(1);
    let speed = cfg.replay.speed.max(1);

    println!();
    println!("Loaded {} opportunities from {}", opps.len(), cfg.paths.opportunities_csv);
    println!("Replay speed: {}x", speed);
    println!();

    // 数据非空，first / max 必有值；用 context 兜底避免 unwrap
    let t_start = opps
        .first()
        .map(|o| o.start_time)
        .context("回放数据为空")?;
    let t_end = opps
        .iter()
        .map(|o| o.end_time)
        .max()
        .context("回放数据为空")?;

    let mut idx: usize = 0;
    let mut active: Vec<&pm_models::ReplayOpportunity> = Vec::new();
    let mut step: u64 = 0;
    let mut t = t_start;

    while t <= t_end {
        // 开放：start_time <= t 的机会进入活跃
        while idx < opps.len() && opps[idx].start_time <= t {
            println!("======================================");
            println!();
            println!("Opportunity Opened");
            println!();
            println!("Time");
            println!();
            println!("{}", opps[idx].start_time.format("%Y-%m-%d %H:%M:%S"));
            println!();
            println!("Question");
            println!();
            println!("{}", opps[idx].question);
            println!();
            println!("Best SUM");
            println!();
            println!("{:.2}", opps[idx].best_sum);
            println!();
            active.push(&opps[idx]);
            idx += 1;
        }

        // 关闭：end_time < t 的机会移出活跃
        let mut closed_now: Vec<&pm_models::ReplayOpportunity> = Vec::new();
        active.retain(|o| {
            if o.end_time < t {
                closed_now.push(*o);
                false
            } else {
                true
            }
        });
        for o in &closed_now {
            println!("======================================");
            println!();
            println!("Opportunity Closed");
            println!();
            println!("Time");
            println!();
            println!("{}", o.end_time.format("%Y-%m-%d %H:%M:%S"));
            println!();
            println!("Question");
            println!();
            println!("{}", o.question);
            println!();
            println!("Duration");
            println!();
            println!("{} sec", o.duration_sec);
            println!();
            println!("Last SUM");
            println!();
            println!("{:.2}", o.last_yes + o.last_no);
            println!();
        }

        // 当前 tick 快照
        println!("======================================");
        println!();
        println!("Replay Tick");
        println!();
        println!("Time");
        println!();
        println!("{}", t.format("%Y-%m-%d %H:%M:%S"));
        println!();
        println!("Step");
        println!();
        println!("{}", step);
        println!();
        println!("Active");
        println!();
        println!("{}", active.len());
        println!();

        // 步进（历史时间）
        step += 1;
        t += chrono::Duration::seconds(step_secs as i64);

        // 按倍率 sleep：1x 睡 step_secs，Nx 睡 step_secs/N
        let sleep_dur =
            Duration::from_secs_f64(step_secs as f64 / speed as f64);
        tokio::time::sleep(sleep_dur).await;
    }

    println!();
    println!("======================================");
    println!();
    println!("Replay Finished");
    println!();
    println!("Total Opportunities");
    println!();
    println!("{}", opps.len());
    println!();
    println!("Steps");
    println!();
    println!("{}", step);
    // 引用 Local 避免未使用告警（保留与原版一致的本地时间约定）
    let _ = Local::now();
    Ok(())
}
