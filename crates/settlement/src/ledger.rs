//! Ledger（资金流水 — P2-06 第七节）。
//!
//! 统一资金流水记录。所有资金变化必须生成 Ledger。
//!
//! 约束：
//! - 只能追加（Append Only）。
//! - 禁止修改 / 删除已记录的 Ledger。
//! - 每条包含：LedgerId / TradeId / OrderId / AccountId / Asset / Amount / Fee / Direction / Before / After / Timestamp。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use chrono::{DateTime, Local};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::{LedgerEntry, TradeFillEvent};

// ============================================================================
// Ledger — 追加不可修改的资金流水
// ============================================================================

/// 资金流水记录器。
///
/// 所有资金变化通过此记录器生成 Ledger。
/// 只追加，不修改。
#[derive(Debug)]
pub struct Ledger {
    /// 流水条目列表（追加）。
    entries: Vec<LedgerEntry>,
    /// 流水 ID 序列号。
    seq: AtomicU64,
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    /// 创建新流水记录器。
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            seq: AtomicU64::new(0),
        }
    }

    /// 生成下一个流水 ID。
    fn next_ledger_id(&self, now: DateTime<Local>) -> String {
        let n = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        format!("LEDGER-{}-{:06}", now.format("%Y%m%d"), n)
    }

    /// 记录一笔出账（成交扣款）。
    ///
    /// # 参数
    ///
    /// - `event`：成交事件。
    /// - `cost`：成交金额。
    /// - `fee`：手续费。
    /// - `before_balance`：变动前余额。
    /// - `after_balance`：变动后余额。
    /// - `description`：摘要说明。
    /// - `now`：时间戳。
    ///
    /// # 返回
    ///
    /// 生成的 LedgerEntry 引用。
    pub fn record_debit(
        &mut self,
        event: &TradeFillEvent,
        cost: f64,
        fee: f64,
        before_balance: f64,
        after_balance: f64,
        description: &str,
        now: DateTime<Local>,
    ) -> &LedgerEntry {
        let id = self.next_ledger_id(now);
        let entry = LedgerEntry::debit(
            id,
            event.trade_id.clone(),
            event.order_id.clone(),
            event.account_id.clone(),
            cost,
            fee,
            before_balance,
            after_balance,
            description.to_string(),
            now,
        );
        tracing::info!(
            ledger_id = %entry.ledger_id,
            trade_id = %entry.trade_id,
            direction = %entry.direction.as_zh(),
            amount = %entry.amount,
            fee = %entry.fee,
            before = %before_balance,
            after = %after_balance,
            "资金流水已记录（出账）"
        );
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    /// 记录一笔入账（平仓收款）。
    pub fn record_credit(
        &mut self,
        event: &TradeFillEvent,
        amount: f64,
        fee: f64,
        before_balance: f64,
        after_balance: f64,
        description: &str,
        now: DateTime<Local>,
    ) -> &LedgerEntry {
        let id = self.next_ledger_id(now);
        let entry = LedgerEntry::credit(
            id,
            event.trade_id.clone(),
            event.order_id.clone(),
            event.account_id.clone(),
            amount,
            fee,
            before_balance,
            after_balance,
            description.to_string(),
            now,
        );
        tracing::info!(
            ledger_id = %entry.ledger_id,
            trade_id = %entry.trade_id,
            direction = %entry.direction.as_zh(),
            amount = %entry.amount,
            fee = %entry.fee,
            before = %before_balance,
            after = %after_balance,
            "资金流水已记录（入账）"
        );
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    /// 记录一笔手续费扣款。
    pub fn record_fee(
        &mut self,
        event: &TradeFillEvent,
        fee: f64,
        before_balance: f64,
        after_balance: f64,
        now: DateTime<Local>,
    ) -> &LedgerEntry {
        let id = self.next_ledger_id(now);
        let entry = LedgerEntry::debit(
            id,
            event.trade_id.clone(),
            event.order_id.clone(),
            event.account_id.clone(),
            fee,
            0.0,
            before_balance,
            after_balance,
            format!("手续费: {}", event.trade_id),
            now,
        );
        tracing::info!(
            ledger_id = %entry.ledger_id,
            trade_id = %entry.trade_id,
            fee = %fee,
            "手续费流水已记录"
        );
        self.entries.push(entry);
        self.entries.last().unwrap()
    }

    /// 获取所有流水条目（只读）。
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// 流水条目总数。
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// 按订单 ID 筛选流水。
    pub fn by_order(&self, order_id: &str) -> Vec<&LedgerEntry> {
        self.entries
            .iter()
            .filter(|e| e.order_id == order_id)
            .collect()
    }

    /// 按成交 ID 筛选流水。
    pub fn by_trade(&self, trade_id: &str) -> Vec<&LedgerEntry> {
        self.entries
            .iter()
            .filter(|e| e.trade_id == trade_id)
            .collect()
    }

    /// 按账户 ID 筛选流水。
    pub fn by_account(&self, account_id: &str) -> Vec<&LedgerEntry> {
        self.entries
            .iter()
            .filter(|e| e.account_id == account_id)
            .collect()
    }

    /// 最近 N 条流水。
    pub fn recent(&self, n: usize) -> &[LedgerEntry] {
        let start = self.entries.len().saturating_sub(n);
        &self.entries[start..]
    }

    /// 总入账金额。
    pub fn total_credits(&self) -> f64 {
        self.entries
            .iter()
            .filter(|e| e.amount > 0.0)
            .map(|e| e.amount)
            .sum::<f64>()
    }

    /// 总出账金额（绝对值）。
    pub fn total_debits(&self) -> f64 {
        self.entries
            .iter()
            .filter(|e| e.amount < 0.0)
            .map(|e| e.amount.abs())
            .sum::<f64>()
    }

    /// 总手续费。
    pub fn total_fees(&self) -> f64 {
        self.entries.iter().map(|e| e.fee).sum::<f64>()
    }

    /// 净流量 = 入账 - 出账 - 手续费。
    pub fn net_flow(&self) -> f64 {
        self.total_credits() - self.total_debits() - self.total_fees()
    }

    /// 打印流水报告（中文 CLI 输出，最近 N 条）。
    pub fn print_zh(&self, n: usize) {
        let entries = self.recent(n);
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!(
            "  资金流水（最近 {} 条 / 共 {} 条）",
            entries.len(),
            self.count()
        );
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!(
            "  {:<22} {:<12} {:<12} {:<6} {:<12} {:<10} {:<12} {}",
            "流水 ID", "成交 ID", "订单 ID", "方向", "金额", "手续费", "余额变化", "说明"
        );
        println!("  {}", "─".repeat(98));

        for e in entries {
            let balance_change = format!("{:.2}→{:.2}", e.before_balance, e.after_balance);
            println!(
                "  {:<22} {:<12} {:<12} {:<6} {:>12.4} {:>10.4} {:<12} {}",
                e.ledger_id,
                if e.trade_id.len() > 10 {
                    format!("{}..", &e.trade_id[..8])
                } else {
                    e.trade_id.clone()
                },
                if e.order_id.len() > 10 {
                    format!("{}..", &e.order_id[..8])
                } else {
                    e.order_id.clone()
                },
                e.direction.as_zh(),
                e.amount.abs(),
                e.fee,
                balance_change,
                e.description,
            );
        }
        println!();
        println!("── 汇总 ──");
        println!("  总入账      : {:.2} USDC", self.total_credits());
        println!("  总出账      : {:.2} USDC", self.total_debits());
        println!("  总手续费    : {:.2} USDC", self.total_fees());
        println!("  净流量      : {:+.2} USDC", self.net_flow());
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!();
    }

    /// 导出为 CSV 字符串。
    pub fn to_csv(&self) -> String {
        let mut wtr = csv::Writer::from_writer(Vec::new());
        // 写入表头
        let header = LedgerEntry::csv_header();
        let _ = wtr.write_record(&header);
        for e in &self.entries {
            let row = e.to_csv_row();
            let _ = wtr.write_record(&row);
        }
        let data = wtr.into_inner().unwrap();
        String::from_utf8(data).unwrap_or_default()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Direction;
    use chrono::Local;
    use pm_core::Side;

    fn sample_fill() -> TradeFillEvent {
        TradeFillEvent {
            trade_id: "T-001".into(),
            order_id: "OMS-001".into(),
            client_order_id: "CLI-001".into(),
            exchange_order_id: None,
            market_id: "mkt-btc".into(),
            account_id: "ACCT-MAIN".into(),
            direction: Direction::Yes,
            side: Side::Buy,
            fill_price: 0.50,
            fill_quantity: 100.0,
            filled_at: Local::now(),
            is_taker: true,
            gateway_name: "Mock".into(),
        }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn record_debit_creates_entry() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill = sample_fill();
        let entry = ledger.record_debit(&fill, 50.0, 0.02, 10000.0, 9949.98, "成交扣款", now);
        assert_eq!(entry.direction, crate::types::LedgerDirection::Debit);
        assert!(entry.amount < 0.0);
        assert_eq!(ledger.count(), 1);
    }

    #[test]
    fn record_credit_creates_entry() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill = sample_fill();
        let entry = ledger.record_credit(&fill, 60.0, 0.02, 9949.98, 10009.96, "平仓入账", now);
        assert!(entry.amount > 0.0);
        assert_eq!(ledger.count(), 1);
    }

    #[test]
    fn ledger_is_append_only() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill = sample_fill();
        ledger.record_debit(&fill, 50.0, 0.02, 10000.0, 9949.98, "第一笔", now);

        let mut fill2 = sample_fill();
        fill2.trade_id = "T-002".into();
        ledger.record_credit(&fill2, 60.0, 0.02, 9949.98, 10009.96, "第二笔", now);

        assert_eq!(ledger.count(), 2);
        // 第一条保持不变
        assert_eq!(ledger.entries()[0].trade_id, "T-001");
        assert_eq!(ledger.entries()[1].trade_id, "T-002");
    }

    #[test]
    fn filter_by_order_and_trade() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill1 = sample_fill();
        let mut fill2 = sample_fill();
        fill2.order_id = "OMS-002".into();
        fill2.trade_id = "T-002".into();

        ledger.record_debit(&fill1, 50.0, 0.0, 10000.0, 9950.0, "test", now);
        ledger.record_debit(&fill2, 30.0, 0.0, 9950.0, 9920.0, "test", now);

        assert_eq!(ledger.by_order("OMS-001").len(), 1);
        assert_eq!(ledger.by_order("OMS-002").len(), 1);
        assert_eq!(ledger.by_trade("T-001").len(), 1);
    }

    #[test]
    fn totals_calculated_correctly() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill1 = sample_fill();
        let mut fill2 = sample_fill();
        fill2.trade_id = "T-002".into();
        fill2.order_id = "OMS-002".into();

        ledger.record_debit(&fill1, 50.0, 2.0, 10000.0, 9948.0, "扣款", now);
        ledger.record_credit(&fill2, 60.0, 2.0, 9948.0, 10006.0, "入账", now);

        assert!(approx(ledger.total_credits(), 60.0));
        assert!(approx(ledger.total_debits(), 50.0));
        assert!(approx(ledger.total_fees(), 4.0));
        assert!(approx(ledger.net_flow(), 6.0)); // 60 - 50 - 4
    }

    #[test]
    fn recent_returns_last_n() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        for i in 0..10 {
            let mut fill = sample_fill();
            fill.trade_id = format!("T-{:03}", i);
            fill.order_id = format!("OMS-{:03}", i);
            ledger.record_debit(&fill, 10.0, 0.0, 10000.0, 9990.0, "test", now);
        }
        assert_eq!(ledger.recent(5).len(), 5);
        assert_eq!(ledger.recent(100).len(), 10);
    }

    #[test]
    fn to_csv_produces_valid_output() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill = sample_fill();
        ledger.record_debit(&fill, 50.0, 0.02, 10000.0, 9949.98, "测试", now);

        let csv = ledger.to_csv();
        assert!(csv.contains("ledger_id"));
        assert!(csv.contains("LEDGER-"));
        assert!(csv.contains("测试"));
    }

    #[test]
    fn ledger_id_is_unique() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill = sample_fill();
        let id1 = ledger
            .record_debit(&fill, 10.0, 0.0, 1000.0, 990.0, "a", now)
            .ledger_id
            .clone();
        let id2 = ledger
            .record_debit(&fill, 10.0, 0.0, 990.0, 980.0, "b", now)
            .ledger_id
            .clone();
        assert_ne!(id1, id2);
    }

    #[test]
    fn print_zh_does_not_panic() {
        let mut ledger = Ledger::new();
        let now = Local::now();
        let fill = sample_fill();
        ledger.record_debit(&fill, 50.0, 0.02, 10000.0, 9949.98, "测试", now);
        ledger.print_zh(10);
    }
}
