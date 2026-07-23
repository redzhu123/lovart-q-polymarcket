//! Execution Queue（V1.06 第六节）。
//!
//! 订单队列，管理待发送订单的生命周期。
//! 支持：FIFO / Priority / Delay / Retry / Pause / Resume。
//! 预留并发支持（当前单线程，未来可升级为多 Gateway 并发）。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use std::collections::VecDeque;

use chrono::{DateTime, Local};

use crate::order::{Order, OrderStatus};

// ============================================================================
// Queue Configuration
// ============================================================================

/// 队列配置（从 config.toml [execution] 段读取，禁止写死）。
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// 最大队列容量。
    pub max_size: usize,
    /// 最大重试次数。
    pub max_retries: u32,
    /// 重试延迟（毫秒）。
    pub retry_delay_ms: u64,
    /// 默认优先级。
    pub default_priority: u32,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_size: 1000,
            max_retries: 3,
            retry_delay_ms: 1000,
            default_priority: 0,
        }
    }
}

// ============================================================================
// Queue Error
// ============================================================================

/// 队列操作错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
    /// 队列已满。
    Full { current: usize, max: usize },
    /// 队列已暂停。
    Paused,
    /// 订单未找到。
    NotFound { order_id: String },
    /// 重试次数已达上限。
    MaxRetriesReached { order_id: String, retries: u32 },
    /// 订单不在可重试状态。
    NotRetryable { order_id: String, status: String },
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueError::Full { current, max } => {
                write!(f, "队列已满: {}/{}", current, max)
            }
            QueueError::Paused => write!(f, "队列已暂停"),
            QueueError::NotFound { order_id } => write!(f, "订单未找到: {}", order_id),
            QueueError::MaxRetriesReached { order_id, retries } => {
                write!(f, "订单 {} 已达最大重试次数: {}", order_id, retries)
            }
            QueueError::NotRetryable { order_id, status } => {
                write!(f, "订单 {} 状态 {} 不可重试", order_id, status)
            }
        }
    }
}

// ============================================================================
// Queue Status
// ============================================================================

/// 队列状态快照（供 CLI 展示）。
#[derive(Debug, Clone)]
pub struct QueueStatus {
    /// 当前队列长度。
    pub len: usize,
    /// 最大容量。
    pub max_size: usize,
    /// 是否暂停。
    pub paused: bool,
    /// 总入队数（累计）。
    pub total_enqueued: u64,
    /// 总出队数（累计）。
    pub total_dequeued: u64,
    /// 总重试次数（累计）。
    pub total_retries: u64,
}

// ============================================================================
// Execution Queue
// ============================================================================

/// 执行队列（V1.06 第六节）。
///
/// 使用 VecDeque 实现，按优先级排序后出队。
/// 支持暂停/恢复，重试机制。
pub struct ExecutionQueue {
    /// 订单存储（FIFO，出队前按优先级排序）。
    orders: VecDeque<Order>,
    /// 暂存的订单（重试时放回）。
    retry_buffer: VecDeque<Order>,
    /// 是否暂停。
    paused: bool,
    /// 队列配置。
    config: QueueConfig,
    /// 总入队数。
    total_enqueued: u64,
    /// 总出队数。
    total_dequeued: u64,
    /// 总重试次数。
    total_retries: u64,
}

impl ExecutionQueue {
    /// 创建新队列。
    pub fn new(config: QueueConfig) -> Self {
        Self {
            orders: VecDeque::new(),
            retry_buffer: VecDeque::new(),
            paused: false,
            config,
            total_enqueued: 0,
            total_dequeued: 0,
            total_retries: 0,
        }
    }

    /// 使用默认配置创建。
    pub fn with_defaults() -> Self {
        Self::new(QueueConfig::default())
    }

    /// 入队：将订单加入队列。
    ///
    /// 订单状态必须为 Created 或 Validated，入队后变为 Queued。
    pub fn enqueue(&mut self, mut order: Order, now: DateTime<Local>) -> Result<(), QueueError> {
        if self.paused {
            return Err(QueueError::Paused);
        }
        if self.orders.len() >= self.config.max_size {
            return Err(QueueError::Full {
                current: self.orders.len(),
                max: self.config.max_size,
            });
        }
        if order.priority == 0 {
            order.priority = self.config.default_priority;
        }
        order.transition(OrderStatus::Queued, "已进入执行队列", now);
        tracing::info!(
            order_id = %order.order_id,
            queue_len = %self.orders.len() + 1,
            "订单入队"
        );
        self.orders.push_back(order);
        self.total_enqueued += 1;
        Ok(())
    }

    /// 出队：按优先级取出下一个订单。
    ///
    /// 先检查 retry_buffer，再按优先级排序后从主队列取出。
    pub fn dequeue(&mut self) -> Option<Order> {
        if self.paused {
            return None;
        }

        // 优先处理重试缓冲
        if let Some(order) = self.retry_buffer.pop_front() {
            self.total_dequeued += 1;
            return Some(order);
        }

        if self.orders.is_empty() {
            return None;
        }

        // 按优先级排序（降序：高优先级在前）
        let orders = std::mem::take(&mut self.orders);
        let mut sorted: Vec<Order> = orders.into();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut dequeued = VecDeque::from(sorted);
        let order = dequeued.pop_front();
        self.orders = dequeued;
        self.total_dequeued += 1;
        order
    }

    /// 查看队首（不出队）。
    pub fn peek(&self) -> Option<&Order> {
        if self.paused {
            return None;
        }
        self.retry_buffer.front().or_else(|| self.orders.front())
    }

    /// 暂停队列。
    pub fn pause(&mut self) {
        self.paused = true;
        tracing::info!("执行队列已暂停");
    }

    /// 恢复队列。
    pub fn resume(&mut self) {
        self.paused = false;
        tracing::info!("执行队列已恢复");
    }

    /// 是否暂停。
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// 当前队列长度（不含 retry buffer）。
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    /// 队列是否为空。
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty() && self.retry_buffer.is_empty()
    }

    /// 总待处理数（含 retry buffer）。
    pub fn total_pending(&self) -> usize {
        self.orders.len() + self.retry_buffer.len()
    }

    /// 按 order_id 移除订单。
    pub fn remove(&mut self, order_id: &str) -> Option<Order> {
        // 先查 retry buffer
        if let Some(pos) = self.retry_buffer.iter().position(|o| o.order_id == order_id) {
            return self.retry_buffer.remove(pos);
        }
        // 再查主队列
        if let Some(pos) = self.orders.iter().position(|o| o.order_id == order_id) {
            return self.orders.remove(pos);
        }
        None
    }

    /// 重试订单：将失败/拒绝的订单重新入队。
    ///
    /// 重试次数递增，超过 max_retries 则返回错误。
    pub fn retry(&mut self, order_id: &str, now: DateTime<Local>) -> Result<(), QueueError> {
        // 先在主队列和 retry buffer 中查找
        let order = self.remove(order_id).ok_or_else(|| QueueError::NotFound {
            order_id: order_id.to_string(),
        })?;

        if order.retry_count >= self.config.max_retries {
            return Err(QueueError::MaxRetriesReached {
                order_id: order_id.to_string(),
                retries: order.retry_count,
            });
        }

        let mut retry_order = order;
        retry_order.retry_count += 1;
        retry_order.transition(
            OrderStatus::Queued,
            &format!("重试 (第 {} 次)", retry_order.retry_count),
            now,
        );
        self.total_retries += 1;
        tracing::info!(
            order_id = %retry_order.order_id,
            retry_count = %retry_order.retry_count,
            "订单重试"
        );
        self.retry_buffer.push_back(retry_order);
        Ok(())
    }

    /// 队列状态快照。
    pub fn status(&self) -> QueueStatus {
        QueueStatus {
            len: self.orders.len(),
            max_size: self.config.max_size,
            paused: self.paused,
            total_enqueued: self.total_enqueued,
            total_dequeued: self.total_dequeued,
            total_retries: self.total_retries,
        }
    }

    /// 打印队列状态（中文）。
    pub fn print_status(&self) {
        println!("【执行队列】");
        println!();
        println!("  当前队列长度 : {}", self.orders.len());
        println!("  最大容量     : {}", self.config.max_size);
        println!("  重试缓冲     : {}", self.retry_buffer.len());
        println!("  状态         : {}", if self.paused { "⏸ 已暂停" } else { "▶ 运行中" });
        println!("  累计入队     : {}", self.total_enqueued);
        println!("  累计出队     : {}", self.total_dequeued);
        println!("  累计重试     : {}", self.total_retries);
        println!();

        if !self.orders.is_empty() {
            println!("  队列内容：");
            println!("  {:<14} {:<10} {:<6} {:<8} {:<8} {:<10}",
                "订单ID", "优先级", "方向", "价格", "数量", "重试");
            println!("  {}", "-".repeat(60));
            for o in &self.orders {
                println!("  {:<14} {:<10} {:<6} {:<8.4} {:<8.2} {:<10}",
                    o.order_id, o.priority, o.direction.as_zh(),
                    o.price, o.quantity, o.retry_count);
            }
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::Direction;
    use chrono::Local;
    use pm_core::Side;

    fn make_order(id: &str, priority: u32) -> Order {
        let now = Local::now();
        let mut o = Order::new(
            id.into(), format!("CLI-{}", id), "mkt-1".into(), "mock".into(),
            Direction::Yes, Side::Buy,
            0.45, 100.0,
            "S1".into(), "R1".into(), "O1".into(), now,
        );
        o.priority = priority;
        o
    }

    #[test]
    fn enqueue_dequeue_fifo() {
        let now = Local::now();
        let mut q = ExecutionQueue::with_defaults();
        let o1 = make_order("EX-001", 0);
        let o2 = make_order("EX-002", 0);

        q.enqueue(o1, now).unwrap();
        q.enqueue(o2, now).unwrap();
        assert_eq!(q.len(), 2);

        let d1 = q.dequeue().unwrap();
        let d2 = q.dequeue().unwrap();
        assert_eq!(d1.order_id, "EX-001");
        assert_eq!(d2.order_id, "EX-002");
        assert!(q.dequeue().is_none());
    }

    #[test]
    fn priority_ordering() {
        let now = Local::now();
        let mut q = ExecutionQueue::with_defaults();

        q.enqueue(make_order("EX-LOW", 1), now).unwrap();
        q.enqueue(make_order("EX-HIGH", 10), now).unwrap();
        q.enqueue(make_order("EX-MED", 5), now).unwrap();

        // 高优先级先出
        let d1 = q.dequeue().unwrap();
        let d2 = q.dequeue().unwrap();
        let d3 = q.dequeue().unwrap();
        assert_eq!(d1.order_id, "EX-HIGH");
        assert_eq!(d2.order_id, "EX-MED");
        assert_eq!(d3.order_id, "EX-LOW");
    }

    #[test]
    fn pause_resume() {
        let now = Local::now();
        let mut q = ExecutionQueue::with_defaults();
        q.enqueue(make_order("EX-001", 0), now).unwrap();

        q.pause();
        assert!(q.is_paused());
        assert!(q.dequeue().is_none()); // 暂停时不出队

        q.resume();
        assert!(!q.is_paused());
        assert!(q.dequeue().is_some()); // 恢复后正常出队
    }

    #[test]
    fn queue_full() {
        let now = Local::now();
        let config = QueueConfig {
            max_size: 2,
            ..QueueConfig::default()
        };
        let mut q = ExecutionQueue::new(config);
        q.enqueue(make_order("EX-001", 0), now).unwrap();
        q.enqueue(make_order("EX-002", 0), now).unwrap();
        let result = q.enqueue(make_order("EX-003", 0), now);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), QueueError::Full { .. }));
    }

    #[test]
    fn retry_mechanism() {
        let now = Local::now();
        let mut q = ExecutionQueue::with_defaults();
        let o = make_order("EX-001", 0);
        q.enqueue(o, now).unwrap();

        // 出队后标记为失败（模拟 Gateway 失败）
        let mut d = q.dequeue().unwrap();
        d.transition(OrderStatus::Failed, "Gateway 超时", now);

        // 放回队列（通过 retry 方法重建）
        // 把失败的订单先放回 orders 以便 retry 找到
        q.orders.push_front(d);

        // retry
        q.retry("EX-001", now).unwrap();
        assert_eq!(q.total_retries, 1);
        assert_eq!(q.total_pending(), 1);

        let retried = q.dequeue().unwrap();
        assert_eq!(retried.retry_count, 1);
        assert_eq!(retried.status, OrderStatus::Queued);
    }

    #[test]
    fn max_retries_exceeded() {
        let now = Local::now();
        let config = QueueConfig {
            max_retries: 2,
            ..QueueConfig::default()
        };
        let mut q = ExecutionQueue::new(config);

        // 第一次：retry_count=0, 成功
        let mut o = make_order("EX-001", 0);
        o.retry_count = 2; // 已达上限
        o.transition(OrderStatus::Failed, "失败", now);
        o.transition(OrderStatus::Queued, "手动入队", now);
        q.orders.push_front(o);

        let result = q.retry("EX-001", now);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), QueueError::MaxRetriesReached { .. }));
    }

    #[test]
    fn remove_order() {
        let now = Local::now();
        let mut q = ExecutionQueue::with_defaults();
        q.enqueue(make_order("EX-001", 0), now).unwrap();
        q.enqueue(make_order("EX-002", 0), now).unwrap();
        assert_eq!(q.len(), 2);

        let removed = q.remove("EX-001");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().order_id, "EX-001");
        assert_eq!(q.len(), 1);

        assert!(q.remove("EX-NOT-EXIST").is_none());
    }

    #[test]
    fn status_snapshot() {
        let now = Local::now();
        let mut q = ExecutionQueue::with_defaults();
        q.enqueue(make_order("EX-001", 0), now).unwrap();

        let status = q.status();
        assert_eq!(status.len, 1);
        assert!(!status.paused);
        assert_eq!(status.total_enqueued, 1);
        assert_eq!(status.total_dequeued, 0);
    }
}
