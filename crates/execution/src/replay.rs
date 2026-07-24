//! Order Replay（V1.06 第十四节）。
//!
//! 回放订单生命周期：重新播放 Created → Queued → Filled → Cancelled 等状态变化。
//! 方便调试和验证订单行为。
//!
//! Simulation Only -- 不连接钱包 / 不真实交易。

use anyhow::Result;
use chrono::Local;

use crate::events::ExecutionEvent;

// ============================================================================
// Order Replay
// ============================================================================

/// 订单回放器（V1.06 第十四节）。
///
/// 从事件日志中回放订单的完整生命周期。
pub struct OrderReplay {
    /// 所有事件。
    events: Vec<ExecutionEvent>,
}

impl OrderReplay {
    /// 从事件列表创建回放器。
    pub fn new(events: Vec<ExecutionEvent>) -> Self {
        Self { events }
    }

    /// 从 CSV 文件加载事件并创建回放器。
    pub fn from_csv(path: &str) -> Result<Self> {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_path(path)?;

        let mut events: Vec<ExecutionEvent> = Vec::new();
        for result in rdr.deserialize() {
            let record: crate::events::EventRecord = result?;
            // 从 EventRecord 重建 ExecutionEvent（简化版：仅用关键字段）
            let now = Local::now(); // CSV 回放使用当前时间
            let event = match record.event_type.as_str() {
                "OrderCreated" => ExecutionEvent::OrderCreated {
                    order_id: record.order_id.clone(),
                    timestamp: now,
                },
                "OrderValidated" => ExecutionEvent::OrderValidated {
                    order_id: record.order_id.clone(),
                    timestamp: now,
                },
                "OrderQueued" => ExecutionEvent::OrderQueued {
                    order_id: record.order_id.clone(),
                    position: 0,
                    timestamp: now,
                },
                "OrderFilled" => ExecutionEvent::OrderFilled {
                    order_id: record.order_id.clone(),
                    avg_price: 0.0,
                    slippage: 0.0,
                    timestamp: now,
                },
                "OrderRejected" => ExecutionEvent::OrderRejected {
                    order_id: record.order_id.clone(),
                    reason: record.details.clone(),
                    timestamp: now,
                },
                "OrderCancelled" => ExecutionEvent::OrderCancelled {
                    order_id: record.order_id.clone(),
                    reason: record.details.clone(),
                    timestamp: now,
                },
                "OrderExpired" => ExecutionEvent::OrderExpired {
                    order_id: record.order_id.clone(),
                    timestamp: now,
                },
                "OrderFailed" => ExecutionEvent::OrderFailed {
                    order_id: record.order_id.clone(),
                    error: record.details.clone(),
                    timestamp: now,
                },
                _ => continue, // 跳过无法重建的事件类型
            };
            events.push(event);
        }
        Ok(Self { events })
    }

    /// 获取所有事件。
    pub fn events(&self) -> &[ExecutionEvent] {
        &self.events
    }

    /// 获取某个订单的所有事件。
    pub fn events_for_order(&self, order_id: &str) -> Vec<&ExecutionEvent> {
        self.events
            .iter()
            .filter(|e| e.order_id() == order_id)
            .collect()
    }

    /// 获取所有唯一的订单 ID。
    pub fn order_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self
            .events
            .iter()
            .map(|e| e.order_id())
            .filter(|id| !id.is_empty())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// 打印某个订单的生命周期时间线（中文）。
    pub fn print_timeline(&self, order_id: &str) {
        let order_events = self.events_for_order(order_id);

        if order_events.is_empty() {
            println!("未找到订单 {} 的事件记录。", order_id);
            return;
        }

        println!("订单 {} 生命周期回放：", order_id);
        println!();
        println!("  {:<20} {:<14} {}", "时间", "事件", "详情");
        println!("  {}", "-".repeat(60));

        for event in &order_events {
            println!(
                "  {:<20} {:<14} {}",
                event.timestamp().format("%Y-%m-%d %H:%M:%S"),
                event.event_name_zh(),
                event_detail(event)
            );
        }
        println!();
    }

    /// 打印所有订单的摘要。
    pub fn print_summary(&self) {
        let ids = self.order_ids();
        println!("【订单回放摘要】");
        println!();
        println!("  订单总数 : {}", ids.len());
        println!("  事件总数 : {}", self.events.len());
        println!();

        for id in &ids {
            let events = self.events_for_order(id);
            let first = events.first().map(|e| e.event_name_zh()).unwrap_or("-");
            let last = events.last().map(|e| e.event_name_zh()).unwrap_or("-");
            println!(
                "  {:<16} {:>4} 个事件  始: {:<10}  终: {:<10}",
                id,
                events.len(),
                first,
                last
            );
        }
        println!();
    }

    /// 按顺序回放所有事件（以 speed 倍速，speed=1 为实时）。
    pub fn replay_all(&self, speed: u32) {
        println!(
            "【回放开始】共 {} 个事件，{} 倍速",
            self.events.len(),
            speed
        );
        println!();

        for (i, event) in self.events.iter().enumerate() {
            println!(
                "[{}/{}] {} - {} ({})",
                i + 1,
                self.events.len(),
                event.timestamp().format("%H:%M:%S"),
                event.event_name_zh(),
                event.order_id()
            );
            if speed > 1 {
                std::thread::sleep(std::time::Duration::from_millis(
                    (1000 / speed).max(50) as u64
                ));
            }
        }
        println!();
        println!("【回放结束】");
    }
}

/// 获取事件的详情字符串。
fn event_detail(event: &ExecutionEvent) -> String {
    match event {
        ExecutionEvent::OrderCreated { .. } => "订单已创建".into(),
        ExecutionEvent::OrderValidated { .. } => "通过所有校验".into(),
        ExecutionEvent::OrderQueued { position, .. } => format!("队列位置: {}", position),
        ExecutionEvent::OrderSubmitted { gateway, .. } => format!("已提交到: {}", gateway),
        ExecutionEvent::OrderAccepted { gateway, .. } => format!("{} 已接受", gateway),
        ExecutionEvent::OrderPartiallyFilled {
            filled,
            remaining,
            price,
            ..
        } => {
            format!(
                "成交: {:.2}  剩余: {:.2}  @ {:.4}",
                filled, remaining, price
            )
        }
        ExecutionEvent::OrderFilled {
            avg_price,
            slippage,
            ..
        } => {
            format!("均价: {:.4}  滑点: {:.2}%", avg_price, slippage * 100.0)
        }
        ExecutionEvent::OrderRejected { reason, .. } => reason.clone(),
        ExecutionEvent::OrderExpired { .. } => "订单超时未成交".into(),
        ExecutionEvent::OrderCancelled { reason, .. } => reason.clone(),
        ExecutionEvent::OrderRetry { attempt, .. } => format!("第 {} 次重试", attempt),
        ExecutionEvent::OrderFailed { error, .. } => error.clone(),
        ExecutionEvent::QueuePaused { .. } => "执行队列暂停".into(),
        ExecutionEvent::QueueResumed { .. } => "执行队列恢复".into(),
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn sample_events() -> Vec<ExecutionEvent> {
        let now = Local::now();
        vec![
            ExecutionEvent::OrderCreated {
                order_id: "EX-001".into(),
                timestamp: now,
            },
            ExecutionEvent::OrderValidated {
                order_id: "EX-001".into(),
                timestamp: now,
            },
            ExecutionEvent::OrderQueued {
                order_id: "EX-001".into(),
                position: 1,
                timestamp: now,
            },
            ExecutionEvent::OrderSubmitted {
                order_id: "EX-001".into(),
                gateway: "Mock".into(),
                timestamp: now,
            },
            ExecutionEvent::OrderAccepted {
                order_id: "EX-001".into(),
                gateway: "Mock".into(),
                timestamp: now,
            },
            ExecutionEvent::OrderFilled {
                order_id: "EX-001".into(),
                avg_price: 0.45,
                slippage: 0.005,
                timestamp: now,
            },
            ExecutionEvent::OrderCreated {
                order_id: "EX-002".into(),
                timestamp: now,
            },
            ExecutionEvent::OrderValidated {
                order_id: "EX-002".into(),
                timestamp: now,
            },
            ExecutionEvent::OrderRejected {
                order_id: "EX-002".into(),
                reason: "资金不足".into(),
                timestamp: now,
            },
        ]
    }

    #[test]
    fn replay_order_ids() {
        let replay = OrderReplay::new(sample_events());
        let ids = replay.order_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"EX-001"));
        assert!(ids.contains(&"EX-002"));
    }

    #[test]
    fn replay_events_for_order() {
        let replay = OrderReplay::new(sample_events());
        let ex1 = replay.events_for_order("EX-001");
        assert_eq!(ex1.len(), 6); // Created → Validated → Queued → Submitted → Accepted → Filled
    }

    #[test]
    fn replay_nonexistent_order() {
        let replay = OrderReplay::new(sample_events());
        let events = replay.events_for_order("EX-999");
        assert!(events.is_empty());
    }
}
