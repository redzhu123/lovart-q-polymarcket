//! pm-shadow：影子交易系统（Simulation Only）。
//!
//! 绝不发送任何真实交易，不接触钱包 / 私钥 / 签名 / 下单 / Polygon 链。
//! 当 Scanner 发现新 Opportunity 时自动"模拟买入"开仓，Opportunity 结束时自动"模拟平仓"，
//! 用一个简单的 mark-to-market 模型按 SUM 变化估算理论盈亏。
//!
//! 所有收益数字均为模拟估算值，不代表真实套利收益；后续接入 CLOB 真实价格后再替换本模型。
//! 代码中凡是模拟估算处均显式标注 "Shadow Model" / "Simulation Only"，不伪装成真实收益。

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Local};

use pm_models::FinishedOpportunity;

/// 每笔影子交易的固定本金（USDC）。Simulation Model 参数，不读配置文件。
pub const INITIAL_CAPITAL: f64 = 100.0;

/// CSV 表头（列顺序固定，须与 [`ShadowTradeRecord`] 字段顺序一致）。
pub const HEADER: &[&str] = &[
    "trade_id",
    "question",
    "entry_time",
    "exit_time",
    "duration",
    "capital",
    "entry_sum",
    "exit_sum",
    "estimated_pnl",
    "estimated_roi",
    "status",
];

/// 交易状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeStatus {
    Open,
    Closed,
}

impl TradeStatus {
    /// 用于 CSV 输出与控制台展示的字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeStatus::Open => "Open",
            TradeStatus::Closed => "Closed",
        }
    }
}

/// 影子交易（Simulation Only）。
#[derive(Debug, Clone)]
pub struct ShadowTrade {
    pub trade_id: String,
    pub question: String,
    pub entry_time: DateTime<Local>,
    pub exit_time: Option<DateTime<Local>>,
    pub entry_yes: f64,
    pub entry_no: f64,
    pub exit_yes: Option<f64>,
    pub exit_no: Option<f64>,
    pub duration_sec: Option<i64>,
    pub capital: f64,
    pub estimated_pnl: Option<f64>,
    pub estimated_roi: Option<f64>,
    pub status: TradeStatus,
}

impl ShadowTrade {
    /// 开仓：用第一次扫描价格建立模拟仓位。Simulation Only。
    pub fn open(
        trade_id: String,
        question: String,
        now: DateTime<Local>,
        entry_yes: f64,
        entry_no: f64,
    ) -> Self {
        Self {
            trade_id,
            question,
            entry_time: now,
            exit_time: None,
            entry_yes,
            entry_no,
            exit_yes: None,
            exit_no: None,
            duration_sec: None,
            capital: INITIAL_CAPITAL,
            estimated_pnl: None,
            estimated_roi: None,
            status: TradeStatus::Open,
        }
    }

    /// 开仓时的 SUM = YES + NO（套利价差基准）。
    pub fn entry_sum(&self) -> f64 {
        self.entry_yes + self.entry_no
    }

    /// 平仓：用最后一次扫描价格结算模拟盈亏。Simulation Only。
    ///
    /// Shadow Model（后续替换为真实模型）：
    ///   假设开仓时以 entry_sum 同时买入 YES + NO，平仓时按 exit_sum 估值。
    ///   仓位单位 = capital / entry_sum；平仓市值 = 仓位 * exit_sum。
    ///   EstimatedPnL = capital * (exit_sum - entry_sum) / entry_sum
    ///   EstimatedROI = EstimatedPnL / capital
    ///
    /// 说明：exit 价格取 Opportunity 最后一次扫描价（非市场结算价），
    /// 因此本模型为保守估算，不代表真实套利收益。
    pub fn close(&mut self, exit_yes: f64, exit_no: f64, now: DateTime<Local>) {
        let entry_sum = self.entry_sum();
        let exit_sum = exit_yes + exit_no;
        self.exit_yes = Some(exit_yes);
        self.exit_no = Some(exit_no);
        self.exit_time = Some(now);
        self.duration_sec = Some((now - self.entry_time).num_seconds());
        // 分母为 0 时无法估算，置 NaN（展示与统计时兜底为 0）
        let (pnl, roi) = if entry_sum.abs() > f64::EPSILON {
            let p = self.capital * (exit_sum - entry_sum) / entry_sum;
            (p, p / self.capital)
        } else {
            (f64::NAN, f64::NAN)
        };
        self.estimated_pnl = Some(pnl);
        self.estimated_roi = Some(roi);
        self.status = TradeStatus::Closed;
    }
}

/// 单条影子交易记录，序列化顺序由结构体字段顺序决定，须与 [`HEADER`] 对齐。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ShadowTradeRecord {
    pub trade_id: String,
    pub question: String,
    pub entry_time: String,
    pub exit_time: String,
    pub duration: i64,
    pub capital: f64,
    pub entry_sum: f64,
    pub exit_sum: f64,
    pub estimated_pnl: f64,
    pub estimated_roi: f64,
    pub status: String,
}

impl From<&ShadowTrade> for ShadowTradeRecord {
    fn from(t: &ShadowTrade) -> Self {
        ShadowTradeRecord {
            trade_id: t.trade_id.clone(),
            question: t.question.clone(),
            entry_time: t.entry_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            exit_time: t
                .exit_time
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default(),
            duration: t.duration_sec.unwrap_or(0),
            capital: t.capital,
            entry_sum: t.entry_sum(),
            exit_sum: t.exit_yes.unwrap_or(0.0) + t.exit_no.unwrap_or(0.0),
            estimated_pnl: t.estimated_pnl.unwrap_or(0.0),
            estimated_roi: t.estimated_roi.unwrap_or(0.0),
            status: t.status.as_str().to_string(),
        }
    }
}

/// 影子交易累计统计（仅统计已平仓交易）。
#[derive(Debug, Clone)]
pub struct ShadowStats {
    pub total: u64,
    pub winners: u64,
    pub losers: u64,
    /// ROI 求和（用于计算均值）。
    roi_sum: f64,
    /// 历史最大 ROI。
    roi_best: f64,
    /// 历史最小 ROI。
    roi_worst: f64,
    /// Duration 求和（秒，用于计算均值）。
    duration_sum_sec: i64,
}

impl ShadowStats {
    pub fn new() -> Self {
        Self {
            total: 0,
            winners: 0,
            losers: 0,
            roi_sum: 0.0,
            roi_best: f64::NEG_INFINITY,
            roi_worst: f64::INFINITY,
            duration_sum_sec: 0,
        }
    }

    /// 直接按字段记录一笔已平仓交易（供从 CSV 重建历史时使用）。
    pub fn record_fields(&mut self, pnl: f64, roi: f64, dur: i64) {
        self.total += 1;
        if pnl > 0.0 {
            self.winners += 1;
        } else if pnl < 0.0 {
            self.losers += 1;
        }
        if roi.is_finite() {
            self.roi_sum += roi;
            if roi > self.roi_best {
                self.roi_best = roi;
            }
            if roi < self.roi_worst {
                self.roi_worst = roi;
            }
        }
        self.duration_sum_sec += dur;
    }

    /// 记录一笔已平仓交易，更新聚合统计。
    pub fn record(&mut self, trade: &ShadowTrade) {
        let Some(pnl) = trade.estimated_pnl else {
            return;
        };
        let Some(roi) = trade.estimated_roi else {
            return;
        };
        let Some(dur) = trade.duration_sec else {
            return;
        };
        self.record_fields(pnl, roi, dur);
    }

    /// 平均 ROI（无交易或无非有限值时为 0）。
    pub fn average_roi(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.roi_sum / self.total as f64
        }
    }

    /// 最佳 ROI（无交易或无非有限值时为 0）。
    pub fn best_roi(&self) -> f64 {
        if self.total == 0 || !self.roi_best.is_finite() {
            0.0
        } else {
            self.roi_best
        }
    }

    /// 最差 ROI（无交易或无非有限值时为 0）。
    pub fn worst_roi(&self) -> f64 {
        if self.total == 0 || !self.roi_worst.is_finite() {
            0.0
        } else {
            self.roi_worst
        }
    }

    /// 平均持仓时长（秒，无交易时为 0）。
    pub fn average_duration_sec(&self) -> i64 {
        if self.total == 0 {
            0
        } else {
            self.duration_sum_sec / self.total as i64
        }
    }
}

impl Default for ShadowStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 启动时从 CSV 读取的历史结果。
pub struct ShadowHistory {
    /// 历史累计统计（仅 Closed 行）。
    pub stats: ShadowStats,
    /// 历史 CSV 数据行数（用于继续编号 trade_id，避免重复）。
    pub next_id_base: u64,
}

/// 影子交易引擎：维护未平仓交易，并在机会结束时平仓。
/// Simulation Only -- 不持有任何钱包 / 私钥 / 签名能力。
pub struct ShadowEngine {
    /// 未平仓交易：question -> trade。一个机会只允许一笔。
    open: HashMap<String, ShadowTrade>,
    /// 自增计数器，用于生成 trade_id。
    counter: u64,
    /// 累计已平仓统计（含本次运行 + 启动时从 CSV 读取的历史）。
    stats: ShadowStats,
}

impl ShadowEngine {
    pub fn new() -> Self {
        Self {
            open: HashMap::new(),
            counter: 0,
            stats: ShadowStats::new(),
        }
    }

    /// 注入历史统计与计数器基线（启动时调用）。
    pub fn load_history(&mut self, stats: ShadowStats, next_id_base: u64) {
        self.stats = stats;
        self.counter = next_id_base;
    }

    /// 生成下一个 trade_id。
    fn next_trade_id(&mut self) -> String {
        self.counter += 1;
        format!("ST-{:06}", self.counter)
    }

    /// 开仓：新机会出现时调用。每个机会只允许一笔（已存在则返回 None）。
    /// 返回新建的 ShadowTrade，供控制台展示。
    pub fn open_trade(
        &mut self,
        question: &str,
        entry_yes: f64,
        entry_no: f64,
        now: DateTime<Local>,
    ) -> Option<ShadowTrade> {
        if self.open.contains_key(question) {
            return None; // 已有未平仓交易，不重复开仓
        }
        let trade_id = self.next_trade_id();
        let trade = ShadowTrade::open(trade_id, question.to_string(), now, entry_yes, entry_no);
        self.open.insert(question.to_string(), trade.clone());
        Some(trade)
    }

    /// 平仓：机会结束时调用。从 open 移除并结算，更新累计统计。
    /// 返回已平仓的 ShadowTrade，供控制台展示与 CSV 写入。
    pub fn close_trade(
        &mut self,
        finished: &FinishedOpportunity,
        now: DateTime<Local>,
    ) -> Option<ShadowTrade> {
        let mut trade = self.open.remove(&finished.question)?;
        trade.close(finished.last_yes, finished.last_no, now);
        self.stats.record(&trade);
        Some(trade)
    }

    /// 累计统计快照。
    pub fn stats(&self) -> &ShadowStats {
        &self.stats
    }
}

impl Default for ShadowEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------- CSV 读写 -----------------------

/// 确保 `shadow_trades.csv` 就绪（委托 [`pm_storage::ensure_csv`]）。
pub fn ensure_csv(path: impl AsRef<Path>) -> anyhow::Result<()> {
    pm_storage::ensure_csv(path, HEADER)
}

/// 把一批已平仓交易追加写入 CSV（委托 [`pm_storage::append_records`]），返回写入条数。
pub fn append_records(records: &[ShadowTradeRecord], path: impl AsRef<Path>) -> usize {
    pm_storage::append_records(path, records)
}

/// 从 CSV 读取全部历史影子交易，重建累计统计与 trade_id 计数基线。
/// 文件不存在或读取失败时返回空结果（不阻断启动）。
pub fn load_history(path: impl AsRef<Path>) -> ShadowHistory {
    let mut stats = ShadowStats::new();
    let mut rows: u64 = 0;
    let path = path.as_ref();
    if !path.exists() {
        return ShadowHistory {
            stats,
            next_id_base: 0,
        };
    }
    let Ok(file) = std::fs::File::open(path) else {
        return ShadowHistory {
            stats,
            next_id_base: 0,
        };
    };
    let reader = std::io::BufReader::new(file);
    // 默认 has_headers(true)：自动跳过首行表头，records() 只产出数据行。
    let mut rdr = csv::Reader::from_reader(reader);
    for result in rdr.records() {
        let Ok(rec) = result else {
            continue;
        };
        rows += 1;
        // 列顺序：trade_id, question, entry_time, exit_time, duration, capital,
        //         entry_sum, exit_sum, estimated_pnl, estimated_roi, status
        let status = rec.get(10).unwrap_or("");
        if status != "Closed" {
            continue; // 仅统计已平仓
        }
        let pnl: f64 = rec.get(8).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let roi: f64 = rec.get(9).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let dur: i64 = rec.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
        stats.record_fields(pnl, roi, dur);
    }
    ShadowHistory {
        stats,
        next_id_base: rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(question: &str, last_yes: f64, last_no: f64) -> FinishedOpportunity {
        let now = Local::now();
        FinishedOpportunity {
            question: question.into(),
            start_time: now,
            end_time: now,
            duration_sec: 100,
            best_sum: 0.9,
            scan_count: 2,
            last_yes,
            last_no,
            volume: 0.0,
            liquidity: 0.0,
        }
    }

    #[test]
    fn open_then_close_computes_pnl() {
        let now = Local::now();
        let mut eng = ShadowEngine::new();
        let t = eng.open_trade("Q", 0.40, 0.50, now).expect("open");
        assert_eq!(t.status, TradeStatus::Open);
        assert!((t.entry_sum() - 0.90).abs() < 1e-9);

        let closed = eng
            .close_trade(&finished("Q", 0.45, 0.55), now)
            .expect("close");
        assert_eq!(closed.status, TradeStatus::Closed);
        // exit_sum=1.00, entry_sum=0.90 -> pnl = 100*(1.0-0.9)/0.9 = 11.11...
        let pnl = closed.estimated_pnl.unwrap();
        assert!(pnl > 0.0);
        assert!((closed.estimated_roi.unwrap() - pnl / 100.0).abs() < 1e-9);
    }

    #[test]
    fn duplicate_open_returns_none() {
        let now = Local::now();
        let mut eng = ShadowEngine::new();
        eng.open_trade("Q", 0.4, 0.5, now);
        assert!(eng.open_trade("Q", 0.4, 0.5, now).is_none());
    }

    #[test]
    fn close_nonexistent_is_none() {
        let now = Local::now();
        let mut eng = ShadowEngine::new();
        assert!(eng.close_trade(&finished("Ghost", 0.5, 0.5), now).is_none());
    }

    #[test]
    fn stats_aggregate_closed_only() {
        let now = Local::now();
        let mut eng = ShadowEngine::new();
        eng.open_trade("A", 0.40, 0.50, now);
        eng.open_trade("B", 0.40, 0.50, now);
        eng.close_trade(&finished("A", 0.45, 0.55), now); // 盈利
        eng.close_trade(&finished("B", 0.35, 0.45), now); // 亏损（exit_sum=0.80<0.90）
        let s = eng.stats();
        assert_eq!(s.total, 2);
        assert_eq!(s.winners, 1);
        assert_eq!(s.losers, 1);
    }

    #[test]
    fn ensure_append_load_history_roundtrip() {
        let dir = std::env::temp_dir().join("pm_shadow_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("shadow_trades.csv");

        ensure_csv(&path).expect("ensure");

        let now = Local::now();
        let mut eng = ShadowEngine::new();
        eng.open_trade("Q", 0.40, 0.50, now);
        let closed = eng
            .close_trade(&finished("Q", 0.45, 0.55), now)
            .expect("close");
        let rec = ShadowTradeRecord::from(&closed);
        assert_eq!(append_records(&[rec], &path), 1);

        // 重新加载历史
        let hist = load_history(&path);
        assert_eq!(hist.next_id_base, 1);
        assert_eq!(hist.stats.total, 1);
        assert_eq!(hist.stats.winners, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
