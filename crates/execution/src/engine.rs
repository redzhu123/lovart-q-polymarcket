//! Execution Simulator 引擎：维护待处理订单、持仓、现金，驱动成交生命周期。
//!
//! Simulation Only -- 绝不连接钱包 / 发送订单 / 签名 / 上链 / 连接 Polygon。
//! 在 Paper "立即成交" 基础上改为模拟真实成交过程：
//!   订单提交 -> Pending ->（随机延迟后）部分成交 / 完全成交 / 超时取消 / 过期。
//!
//! 风控：待处理订单上限、现金不足、价格非法 -> Rejected（引擎内部硬限制，作为兜底）。
//! 配置经 [`ExecParams`] 注入（由 driver 从 `Config.execution` 组装）。

use chrono::{DateTime, Local};
use pm_core::Side;

use crate::fill::FillEngine;
use crate::state::{OrderStatus, TerminalReason};

/// 浮点比较容差。
const EPS: f64 = 1e-9;

/// Execution 引擎参数（由 driver 从 `Config.execution` 组装）。
#[derive(Debug, Clone, Copy)]
pub struct ExecParams {
    pub capital: f64,
    pub max_pending_orders: usize,
    pub order_notional: f64,
    /// 最大等待扫描周期数，到达仍未完全成交 -> Cancelled / Expired。
    pub max_wait_scans: u32,
    /// 最大成交延迟（扫描周期数，0..=N 随机）。
    pub max_fill_delay: u32,
}

impl ExecParams {
    /// 默认参数（与 v0.9 常量对齐：10000 / 20 / 100 / 5 / 3）。
    pub fn default_for_scan() -> Self {
        Self {
            capital: 10000.0,
            max_pending_orders: 20,
            order_notional: 100.0,
            max_wait_scans: 5,
            max_fill_delay: 3,
        }
    }

    /// 压测参数：大额资金避免现金耗尽干扰成交统计，其余同默认。
    pub fn default_for_stress() -> Self {
        Self {
            capital: 1_000_000.0,
            max_pending_orders: 20,
            order_notional: 100.0,
            max_wait_scans: 5,
            max_fill_delay: 3,
        }
    }
}

/// 订单提交结果。
pub enum SubmitOutcome {
    /// 已进入 Pending（返回分配到的 order_id）。
    Accepted(String),
    /// 被风控拒绝（未进入 Pending）。
    Rejected(TerminalReason),
}

/// tick 产生的事件（供控制台展示）。Simulation Only。
#[derive(Debug, Clone)]
pub enum ExecEvent {
    /// 新订单进入 Pending。
    NewOrder {
        order_id: String,
        question: String,
        side: Side,
        quantity: f64,
        notional: f64,
    },
    /// 完全成交。
    Filled {
        order_id: String,
        question: String,
        side: Side,
        fill_time_scans: u32,
        slippage: f64,
        filled_quantity: f64,
    },
    /// 部分成交（非终态）。
    PartiallyFilled {
        order_id: String,
        question: String,
        filled_quantity: f64,
        slippage: f64,
    },
    /// 取消（部分成交后超时，保留已成交部分）。
    Cancelled {
        order_id: String,
        question: String,
        filled_quantity: f64,
    },
    /// 过期（零成交超时，整单作废）。
    Expired { order_id: String, question: String },
    /// 提交即被风控拒绝。
    Rejected {
        question: String,
        side: Side,
        reason: TerminalReason,
    },
    /// SELL 完全成交 -> 持仓平仓。
    PositionClosed {
        order_id: String,
        question: String,
        realized_pnl: f64,
    },
}

/// 模拟持仓（Execution Simulator 内部）。Simulation Only。
#[derive(Debug, Clone)]
pub struct ExecPosition {
    pub question: String,
    pub side: Side,
    pub quantity: f64,
    pub avg_price: f64,
    pub cost_basis: f64,
    pub open_time: DateTime<Local>,
    /// 已实现盈亏（开仓时为 0，平仓时计算）。
    pub realized_pnl: f64,
    pub exit_price: Option<f64>,
    pub exit_time: Option<DateTime<Local>>,
}

/// 组合概览（控制台仪表盘用）。
#[derive(Debug, Clone)]
pub struct PortfolioSummary {
    pub available_cash: f64,
    pub pending_cash: f64,
    pub pending_orders: usize,
    pub open_positions: usize,
    pub closed_positions: usize,
}

/// 模拟订单（含完整生命周期字段）。Simulation Only -- simulation_only 恒为 true。
#[derive(Debug, Clone)]
pub struct ExecutionOrder {
    pub order_id: String,
    pub question: String,
    pub side: Side,
    pub quantity: f64,
    pub base_price: f64,
    pub filled_quantity: f64,
    /// 加权平均成交价（含滑点）。
    pub avg_fill_price: f64,
    pub status: OrderStatus,
    pub create_time: DateTime<Local>,
    pub fill_time: Option<DateTime<Local>>,
    /// 实际成交耗时（扫描周期数；从创建到完全成交）。
    pub fill_time_scans: u32,
    /// 已分配的成交延迟（扫描周期数）。
    pub assigned_delay: u32,
    /// 综合滑点（加权平均，小数形式）。
    pub slippage: f64,
    pub cancel_reason: TerminalReason,
    /// 永远为 true：标记本订单仅为模拟。
    pub simulation_only: bool,
    // ---- 内部成交进度（不写入 CSV）----
    /// 剩余等待周期数（>0 时不成交）。
    wait_remaining: u32,
    /// 分批成交计划（剩余批次的比例）。
    schedule: Vec<f64>,
    /// 已存活的扫描周期数。
    scans_alive: u32,
    /// 是否流动性失败（零成交 -> Expired）。
    liquidity_fail: bool,
    /// 是否经历过部分成交（多批次）。
    partial_occurred: bool,
    /// 关联持仓（SELL 平仓时引用；BUY 为 None）。
    target_position: Option<ExecPosition>,
}

impl ExecutionOrder {
    /// 单笔订单的成交率 = filled_quantity / quantity。
    pub fn fill_rate(&self) -> f64 {
        if self.quantity.abs() > f64::EPSILON {
            self.filled_quantity / self.quantity
        } else {
            0.0
        }
    }
}

/// 执行统计。Simulation Only。
#[derive(Debug, Clone)]
pub struct ExecutionStats {
    pub total: u64,
    pub filled: u64,
    pub cancelled: u64,
    pub expired: u64,
    pub rejected: u64,
    /// 有任意成交的订单数（Filled + 部分成交后 Cancelled）。
    any_fill: u64,
    /// 经历过部分成交（多批次）的订单数。
    partial_occurred: u64,
    /// fill_time 求和（扫描周期数，仅 Filled）。
    fill_time_sum: f64,
    /// assigned_delay 求和（扫描周期数，仅 Filled）。
    assigned_delay_sum: f64,
    /// 滑点求和（仅 Filled）。
    slippage_sum: f64,
    /// 参与 Filled 统计的订单数。
    filled_count: u64,
}

impl ExecutionStats {
    pub fn new() -> Self {
        Self {
            total: 0,
            filled: 0,
            cancelled: 0,
            expired: 0,
            rejected: 0,
            any_fill: 0,
            partial_occurred: 0,
            fill_time_sum: 0.0,
            assigned_delay_sum: 0.0,
            slippage_sum: 0.0,
            filled_count: 0,
        }
    }

    /// 记录一笔终态订单，更新聚合统计。
    fn record_terminal(&mut self, order: &ExecutionOrder) {
        match order.status {
            OrderStatus::Filled => {
                self.filled += 1;
                self.filled_count += 1;
                self.fill_time_sum += order.fill_time_scans as f64;
                self.assigned_delay_sum += order.assigned_delay as f64;
                if order.slippage.is_finite() {
                    self.slippage_sum += order.slippage;
                }
                if order.filled_quantity > EPS {
                    self.any_fill += 1;
                }
                if order.partial_occurred {
                    self.partial_occurred += 1;
                }
            }
            OrderStatus::Cancelled => {
                self.cancelled += 1;
                if order.filled_quantity > EPS {
                    self.any_fill += 1;
                }
                if order.partial_occurred {
                    self.partial_occurred += 1;
                }
            }
            OrderStatus::Expired => {
                self.expired += 1;
            }
            OrderStatus::Rejected => {
                self.rejected += 1;
            }
            _ => {}
        }
    }

    /// Fill Rate = 完全成交数 / 总订单数。
    pub fn fill_rate(&self) -> f64 {
        pm_utils::ratio(self.filled, self.total)
    }
    /// Execution Success Rate = 有任意成交的订单数 / 总订单数。
    pub fn execution_success_rate(&self) -> f64 {
        pm_utils::ratio(self.any_fill, self.total)
    }
    /// Partial Fill Rate = 经历部分成交的订单数 / 总订单数。
    pub fn partial_fill_rate(&self) -> f64 {
        pm_utils::ratio(self.partial_occurred, self.total)
    }
    /// Average Fill Time（扫描周期数，仅 Filled）。
    pub fn average_fill_time(&self) -> f64 {
        if self.filled_count > 0 {
            self.fill_time_sum / self.filled_count as f64
        } else {
            0.0
        }
    }
    /// Average Delay（已分配延迟的均值，扫描周期数，仅 Filled）。
    pub fn average_delay(&self) -> f64 {
        if self.filled_count > 0 {
            self.assigned_delay_sum / self.filled_count as f64
        } else {
            0.0
        }
    }
    /// Average Slippage（小数形式，仅 Filled）。
    pub fn average_slippage(&self) -> f64 {
        if self.filled_count > 0 {
            self.slippage_sum / self.filled_count as f64
        } else {
            0.0
        }
    }
}

impl Default for ExecutionStats {
    fn default() -> Self {
        Self::new()
    }
}

/// 执行模拟引擎。Simulation Only -- 不持有任何钱包 / 私钥 / 签名能力。
pub struct ExecutionEngine {
    /// 待处理订单（Pending / PartiallyFilled）。
    pending: Vec<ExecutionOrder>,
    /// 终态订单（待写入 CSV）。
    terminal: Vec<ExecutionOrder>,
    /// 开仓持仓：question -> position。
    open_positions: std::collections::HashMap<String, ExecPosition>,
    /// 已平仓持仓。
    closed_positions: Vec<ExecPosition>,
    available_cash: f64,
    pending_cash: f64,
    counter: u64,
    fill_engine: FillEngine,
    stats: ExecutionStats,
    initial_capital: f64,
    max_pending_orders: usize,
    order_notional: f64,
    max_wait_scans: u32,
}

impl ExecutionEngine {
    pub fn new(params: ExecParams) -> Self {
        Self {
            pending: Vec::new(),
            terminal: Vec::new(),
            open_positions: std::collections::HashMap::new(),
            closed_positions: Vec::new(),
            available_cash: params.capital,
            pending_cash: 0.0,
            counter: 0,
            fill_engine: FillEngine::new(params.max_fill_delay),
            stats: ExecutionStats::new(),
            initial_capital: params.capital,
            max_pending_orders: params.max_pending_orders,
            order_notional: params.order_notional,
            max_wait_scans: params.max_wait_scans,
        }
    }

    /// 注入 order_id 计数基线（启动时调用，值 = 历史 execution_orders.csv 数据行数）。
    pub fn load_order_base(&mut self, base: u64) {
        self.counter = base;
    }

    /// 初始资金。
    pub fn initial_capital(&self) -> f64 {
        self.initial_capital
    }

    /// 单笔订单固定成本（USDC）。
    pub fn order_notional(&self) -> f64 {
        self.order_notional
    }

    /// 生成下一个 order_id。
    fn next_order_id(&mut self) -> String {
        self.counter += 1;
        format!("EX-{:06}", self.counter)
    }

    /// 当前待处理订单数。
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// 当前开仓数。
    pub fn open_position_count(&self) -> usize {
        self.open_positions.len()
    }

    /// 已平仓数。
    pub fn closed_position_count(&self) -> usize {
        self.closed_positions.len()
    }

    /// 开仓持仓成本之和（占用资金）。
    pub fn open_positions_cost(&self) -> f64 {
        self.open_positions.values().map(|p| p.cost_basis).sum()
    }

    /// 已平仓持仓的已实现盈亏之和。
    pub fn closed_realized_pnl(&self) -> f64 {
        self.closed_positions.iter().map(|p| p.realized_pnl).sum()
    }

    /// 可用现金。
    pub fn available_cash(&self) -> f64 {
        self.available_cash
    }

    /// 待处理现金（已锁定）。
    pub fn pending_cash(&self) -> f64 {
        self.pending_cash
    }

    /// 统计快照。
    pub fn stats(&self) -> &ExecutionStats {
        &self.stats
    }

    /// 组合概览。
    pub fn portfolio_summary(&self) -> PortfolioSummary {
        PortfolioSummary {
            available_cash: self.available_cash,
            pending_cash: self.pending_cash,
            pending_orders: self.pending.len(),
            open_positions: self.open_positions.len(),
            closed_positions: self.closed_positions.len(),
        }
    }

    /// 取出终态订单（供调用方写 CSV）。统计已在 tick / submit 时更新。
    pub fn drain_terminal(&mut self) -> Vec<ExecutionOrder> {
        std::mem::take(&mut self.terminal)
    }

    /// 构造一笔 Rejected 订单，入 terminal 并更新统计，返回拒绝结果。
    fn reject(
        &mut self,
        question: &str,
        side: Side,
        reason: TerminalReason,
        now: DateTime<Local>,
    ) -> SubmitOutcome {
        self.stats.total += 1;
        self.stats.rejected += 1;
        let order = ExecutionOrder {
            order_id: self.next_order_id(),
            question: question.to_string(),
            side,
            quantity: 0.0,
            base_price: 0.0,
            filled_quantity: 0.0,
            avg_fill_price: 0.0,
            status: OrderStatus::Rejected,
            create_time: now,
            fill_time: None,
            fill_time_scans: 0,
            assigned_delay: 0,
            slippage: 0.0,
            cancel_reason: reason,
            simulation_only: true,
            wait_remaining: 0,
            schedule: Vec::new(),
            scans_alive: 0,
            liquidity_fail: false,
            partial_occurred: false,
            target_position: None,
        };
        self.terminal.push(order);
        SubmitOutcome::Rejected(reason)
    }

    /// 提交 BUY 订单。Simulation Only。
    /// 风控顺序：价格合法性 -> 待处理上限 -> 现金充足。
    pub fn submit_buy(
        &mut self,
        question: &str,
        price: f64,
        now: DateTime<Local>,
    ) -> SubmitOutcome {
        if !price.is_finite() || price <= 0.0 {
            return self.reject(question, Side::Buy, TerminalReason::InvalidPrice, now);
        }
        if self.pending.len() >= self.max_pending_orders {
            return self.reject(question, Side::Buy, TerminalReason::MaxPending, now);
        }
        let notional = self.order_notional;
        if self.available_cash + EPS < notional {
            return self.reject(question, Side::Buy, TerminalReason::InsufficientCash, now);
        }
        let quantity = notional / price;
        let assigned_delay = self.fill_engine.assign_delay();
        let liquidity_fail = self.fill_engine.liquidity_fail();
        let schedule = self.fill_engine.partial_schedule();
        let order_id = self.next_order_id();
        let order = ExecutionOrder {
            order_id: order_id.clone(),
            question: question.to_string(),
            side: Side::Buy,
            quantity,
            base_price: price,
            filled_quantity: 0.0,
            avg_fill_price: 0.0,
            status: OrderStatus::Pending,
            create_time: now,
            fill_time: None,
            fill_time_scans: 0,
            assigned_delay,
            slippage: 0.0,
            cancel_reason: TerminalReason::None,
            simulation_only: true,
            wait_remaining: assigned_delay,
            schedule,
            scans_alive: 0,
            liquidity_fail,
            partial_occurred: false,
            target_position: None,
        };
        // 锁定资金
        self.available_cash -= notional;
        self.pending_cash += notional;
        self.stats.total += 1;
        self.pending.push(order);
        SubmitOutcome::Accepted(order_id)
    }

    /// 提交 SELL 订单（平仓）。Simulation Only。
    /// 风控顺序：价格合法性 -> 待处理上限 -> 存在持仓。
    /// SELL 采用单批成交、不流动性失败模型，避免持仓孤儿。
    pub fn submit_sell(
        &mut self,
        question: &str,
        exit_price: f64,
        now: DateTime<Local>,
    ) -> SubmitOutcome {
        if !exit_price.is_finite() || exit_price <= 0.0 {
            return self.reject(question, Side::Sell, TerminalReason::InvalidPrice, now);
        }
        if self.pending.len() >= self.max_pending_orders {
            return self.reject(question, Side::Sell, TerminalReason::MaxPending, now);
        }
        let Some(pos) = self.open_positions.remove(question) else {
            return self.reject(question, Side::Sell, TerminalReason::NoPosition, now);
        };
        let delay = self.fill_engine.assign_delay();
        let order_id = self.next_order_id();
        let order = ExecutionOrder {
            order_id: order_id.clone(),
            question: question.to_string(),
            side: Side::Sell,
            quantity: pos.quantity,
            base_price: exit_price,
            filled_quantity: 0.0,
            avg_fill_price: 0.0,
            status: OrderStatus::Pending,
            create_time: now,
            fill_time: None,
            fill_time_scans: 0,
            assigned_delay: delay,
            slippage: 0.0,
            cancel_reason: TerminalReason::None,
            simulation_only: true,
            wait_remaining: delay,
            schedule: vec![1.0],
            scans_alive: 0,
            liquidity_fail: false,
            partial_occurred: false,
            target_position: Some(pos),
        };
        self.stats.total += 1;
        self.pending.push(order);
        SubmitOutcome::Accepted(order_id)
    }

    /// 推进一个扫描周期：对每笔 pending 订单尝试成交 / 超时。
    /// 返回本周期产生的事件（供控制台展示）。Simulation Only。
    pub fn tick(&mut self, now: DateTime<Local>) -> Vec<ExecEvent> {
        let mut events: Vec<ExecEvent> = Vec::new();
        // 把 pending 取出，避免在遍历时与 self 其它字段借用冲突。
        let mut pending = std::mem::take(&mut self.pending);
        let max_wait = self.max_wait_scans;

        for order in pending.iter_mut() {
            if order.status.is_terminal() {
                continue;
            }
            order.scans_alive += 1;

            // 流动性失败：等到 MAX_WAIT 才判 Expired（零成交）
            if order.liquidity_fail {
                if order.scans_alive >= max_wait {
                    self.expire_order(order, now);
                    events.push(ExecEvent::Expired {
                        order_id: order.order_id.clone(),
                        question: order.question.clone(),
                    });
                }
                continue;
            }

            // 非流动性失败：先扣等待，再尝试成交
            if order.wait_remaining > 0 {
                order.wait_remaining -= 1;
            } else if !order.schedule.is_empty() {
                let frac = order.schedule.remove(0);
                let chunk_qty = frac * order.quantity;
                if chunk_qty > EPS {
                    self.apply_fill(order, chunk_qty, now, &mut events);
                }
            }

            // 超时检查（仍非终态时）
            if !order.status.is_terminal() && order.scans_alive >= max_wait {
                if order.filled_quantity <= EPS {
                    self.expire_order(order, now);
                    events.push(ExecEvent::Expired {
                        order_id: order.order_id.clone(),
                        question: order.question.clone(),
                    });
                } else {
                    self.cancel_order(order, now);
                    events.push(ExecEvent::Cancelled {
                        order_id: order.order_id.clone(),
                        question: order.question.clone(),
                        filled_quantity: order.filled_quantity,
                    });
                }
            }
        }

        // 终态订单移入 terminal 并更新统计；其余放回 pending
        let mut still: Vec<ExecutionOrder> = Vec::new();
        for order in pending {
            if order.status.is_terminal() {
                self.stats.record_terminal(&order);
                self.terminal.push(order);
            } else {
                still.push(order);
            }
        }
        self.pending = still;
        events
    }

    /// 对订单应用一批成交（chunk_qty 份额），更新成交价 / 滑点 / 记账 / 持仓。
    fn apply_fill(
        &mut self,
        order: &mut ExecutionOrder,
        chunk_qty: f64,
        now: DateTime<Local>,
        events: &mut Vec<ExecEvent>,
    ) {
        let slip = self.fill_engine.slippage(order.quantity);
        let fill_price = match order.side {
            Side::Buy => order.base_price * (1.0 + slip),
            Side::Sell => order.base_price * (1.0 - slip),
        };

        // 加权平均成交价与滑点
        let prev_filled = order.filled_quantity;
        let prev_slip_sum = order.slippage * prev_filled;
        let prev_cost = order.avg_fill_price * prev_filled;
        order.filled_quantity = prev_filled + chunk_qty;
        if order.filled_quantity > EPS {
            order.slippage = (prev_slip_sum + slip * chunk_qty) / order.filled_quantity;
            order.avg_fill_price = (prev_cost + chunk_qty * fill_price) / order.filled_quantity;
        }

        // 记账
        match order.side {
            Side::Buy => {
                let base_chunk = chunk_qty * order.base_price;
                let actual_chunk = chunk_qty * fill_price;
                self.pending_cash -= base_chunk;
                // 滑点额外成本从可用现金扣除
                self.available_cash -= actual_chunk - base_chunk;
                self.merge_buy_fill(&order.question, chunk_qty, fill_price, now);
            }
            Side::Sell => {
                let proceeds = chunk_qty * fill_price;
                self.available_cash += proceeds;
            }
        }

        // 是否完全成交
        if order.filled_quantity >= order.quantity - EPS {
            order.status = OrderStatus::Filled;
            order.fill_time = Some(now);
            order.fill_time_scans = order.scans_alive;
            if order.side == Side::Sell {
                if let Some(pos) = order.target_position.take() {
                    let proceeds = order.filled_quantity * order.avg_fill_price;
                    let realized = proceeds - pos.cost_basis;
                    let q = pos.question.clone();
                    self.closed_positions.push(ExecPosition {
                        realized_pnl: realized,
                        exit_price: Some(order.avg_fill_price),
                        exit_time: Some(now),
                        ..pos
                    });
                    events.push(ExecEvent::PositionClosed {
                        order_id: order.order_id.clone(),
                        question: q,
                        realized_pnl: realized,
                    });
                }
            }
            events.push(ExecEvent::Filled {
                order_id: order.order_id.clone(),
                question: order.question.clone(),
                side: order.side,
                fill_time_scans: order.fill_time_scans,
                slippage: order.slippage,
                filled_quantity: order.filled_quantity,
            });
        } else {
            order.status = OrderStatus::PartiallyFilled;
            order.partial_occurred = true;
            events.push(ExecEvent::PartiallyFilled {
                order_id: order.order_id.clone(),
                question: order.question.clone(),
                filled_quantity: order.filled_quantity,
                slippage: order.slippage,
            });
        }
    }

    /// 把 BUY 成交份额并入持仓（已存在则累加）。
    fn merge_buy_fill(&mut self, question: &str, qty: f64, price: f64, now: DateTime<Local>) {
        match self.open_positions.get_mut(question) {
            Some(pos) => {
                let new_cost = pos.cost_basis + qty * price;
                pos.quantity += qty;
                pos.avg_price = new_cost / pos.quantity;
                pos.cost_basis = new_cost;
            }
            None => {
                self.open_positions.insert(
                    question.to_string(),
                    ExecPosition {
                        question: question.to_string(),
                        side: Side::Buy,
                        quantity: qty,
                        avg_price: price,
                        cost_basis: qty * price,
                        open_time: now,
                        realized_pnl: 0.0,
                        exit_price: None,
                        exit_time: None,
                    },
                );
            }
        }
    }

    /// 过期：零成交整单作废，释放全部锁定资金（BUY）/ 持仓放回（SELL）。
    fn expire_order(&mut self, order: &mut ExecutionOrder, _now: DateTime<Local>) {
        order.status = OrderStatus::Expired;
        order.cancel_reason = TerminalReason::Timeout;
        match order.side {
            Side::Buy => {
                let release = (order.quantity - order.filled_quantity) * order.base_price;
                self.pending_cash -= release;
                self.available_cash += release;
            }
            Side::Sell => {
                if let Some(pos) = order.target_position.take() {
                    self.open_positions.insert(pos.question.clone(), pos);
                }
            }
        }
    }

    /// 取消：保留已成交部分，释放未成交部分的锁定资金（BUY）/ 持仓放回（SELL）。
    fn cancel_order(&mut self, order: &mut ExecutionOrder, now: DateTime<Local>) {
        order.status = OrderStatus::Cancelled;
        order.cancel_reason = TerminalReason::Timeout;
        order.fill_time = Some(now);
        match order.side {
            Side::Buy => {
                let release = (order.quantity - order.filled_quantity) * order.base_price;
                self.pending_cash -= release;
                self.available_cash += release;
            }
            Side::Sell => {
                // SELL 单批模型下不会触发；兜底：把持仓放回。
                if let Some(pos) = order.target_position.take() {
                    self.open_positions.insert(pos.question.clone(), pos);
                }
            }
        }
    }
}
