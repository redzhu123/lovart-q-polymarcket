//! 指数退避策略。
//!
//! 从 `pm-gateway::retry` 提取并统一。

/// 指数退避计算器
#[derive(Debug, Clone)]
pub struct Backoff {
    /// 基础延迟（毫秒）
    base_ms: u64,
    /// 最大延迟（毫秒）
    max_ms: u64,
    /// 乘数因子
    multiplier: f64,
    /// 当前尝试次数
    attempt: u32,
    /// 累计等待时间（毫秒）
    pub total_waited_ms: u64,
}

impl Backoff {
    /// 创建新的退避策略
    ///
    /// # 参数
    /// - `base_ms`：基础延迟
    /// - `max_ms`：最大延迟上限
    /// - `multiplier`：每次退避的乘数因子（通常 2.0）
    pub fn new(base_ms: u64, max_ms: u64, multiplier: f64) -> Self {
        Self {
            base_ms,
            max_ms,
            multiplier,
            attempt: 0,
            total_waited_ms: 0,
        }
    }

    /// 计算下一次退避延迟（毫秒）
    pub fn next_delay_ms(&mut self) -> u64 {
        let delay = (self.base_ms as f64 * self.multiplier.powi(self.attempt as i32)) as u64;
        self.attempt += 1;
        let capped = delay.min(self.max_ms);
        self.total_waited_ms += capped;
        capped
    }

    /// 重置退避状态
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.total_waited_ms = 0;
    }

    /// 当前尝试次数
    pub fn attempt_count(&self) -> u32 {
        self.attempt
    }

    /// 是否已耗尽重试次数
    pub fn exhausted(&self, max_retries: u32) -> bool {
        self.attempt >= max_retries
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(100, 10_000, 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_exponentially() {
        let mut b = Backoff::new(100, 10_000, 2.0);
        let d1 = b.next_delay_ms();
        let d2 = b.next_delay_ms();
        let d3 = b.next_delay_ms();
        // 指数增长：100, 200, 400
        assert!(d1 <= 100);
        assert!(d2 <= 200);
        assert!(d3 <= 400);
        assert_eq!(b.attempt_count(), 3);
    }

    #[test]
    fn backoff_respects_max() {
        let mut b = Backoff::new(1000, 1000, 2.0);
        // 多次退避后不应超过 max
        for _ in 0..10 {
            let delay = b.next_delay_ms();
            assert!(delay <= 1000);
        }
    }

    #[test]
    fn backoff_exhausted() {
        let mut b = Backoff::new(10, 1000, 2.0);
        for _ in 0..5 {
            b.next_delay_ms();
        }
        assert!(b.exhausted(5));
        assert!(!b.exhausted(6));
    }

    #[test]
    fn backoff_reset() {
        let mut b = Backoff::new(100, 10_000, 2.0);
        b.next_delay_ms();
        b.next_delay_ms();
        b.reset();
        assert_eq!(b.attempt_count(), 0);
        assert_eq!(b.total_waited_ms, 0);
    }

    #[test]
    fn default_backoff() {
        let b = Backoff::default();
        assert_eq!(b.attempt_count(), 0);
    }
}
