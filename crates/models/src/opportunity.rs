//! 机会生命周期数据模型（DTO）。
//!
//! 这些类型在 tracker / scanner / recorder / shadow / backtest / strategy 之间流转，
//! 纯数据、行为轻量，故归 models。跟踪逻辑（observe/reap）在 pm-tracker。

use chrono::{DateTime, Local};

/// 机会生命周期状态（Tracker 内部持有，跨轮维护）。
///
/// 字段：Question / StartTime / LastSeen / BestSUM / ScanCount / LastYES / LastNO / Volume / Liquidity。
#[derive(Debug, Clone)]
pub struct OpportunityState {
    pub question: String,
    pub start_time: DateTime<Local>,
    pub last_seen: DateTime<Local>,
    /// 历史最低 SUM（套利越大越好，所以取最小值）。
    pub best_sum: f64,
    pub scan_count: u64,
    pub last_yes: f64,
    pub last_no: f64,
    pub volume: f64,
    pub liquidity: f64,
}

/// `observe` 的返回：告诉调用方这是新建还是更新，并带上展示所需的快照字段。
#[derive(Debug, Clone)]
pub struct TrackUpdate {
    pub is_new: bool,
    pub question: String,
    /// 从 start_time 到当前轮的秒数。
    pub duration_sec: i64,
    pub best_sum: f64,
    pub scan_count: u64,
    /// 本轮观测到的 SUM（New 区块展示用）。
    pub sum: f64,
}

/// 生命周期结束的机会，字段对齐 CSV 列，可直接转成 LifecycleRecord。
#[derive(Debug, Clone)]
pub struct FinishedOpportunity {
    pub question: String,
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub duration_sec: i64,
    pub best_sum: f64,
    pub scan_count: u64,
    pub last_yes: f64,
    pub last_no: f64,
    pub volume: f64,
    pub liquidity: f64,
}

/// 回放 / 回测使用的历史机会（CSV 解析后的强类型结构）。
#[derive(Debug, Clone)]
pub struct ReplayOpportunity {
    pub question: String,
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub duration_sec: i64,
    pub best_sum: f64,
    pub scan_count: u64,
    pub last_yes: f64,
    pub last_no: f64,
    pub volume: f64,
    pub liquidity: f64,
}
