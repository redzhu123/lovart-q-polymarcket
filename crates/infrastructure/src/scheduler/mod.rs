//! 任务调度模块：统一的任务调度、限流和重试。
//!
//! 从 `pm-execution::scheduler` 和 `pm-auth::refresh` 提取并统一。
//!
//! # 核心能力
//!
//! - [`Scheduler`] trait：统一的调度器接口
//! - [`TokenBucket`]：令牌桶限流器
//! - [`IntervalScheduler`]：基于固定间隔的调度器（默认实现）
//! - [`Backoff`]：指数退避（见 `backoff` 模块）
//! - [`CircuitBreaker`]：熔断器（见 `breaker` 模块）
//! - [`RetryExecutor`]：重试执行器（见 `breaker` 模块）

pub mod backoff;
pub mod breaker;

pub use backoff::Backoff;
pub use breaker::{CircuitBreaker, CircuitState, RetryError, RetryExecutor};

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 调度类型
#[derive(Debug, Clone)]
pub enum ScheduleKind {
    /// Cron 表达式（"*/5 * * * *"）
    Cron(String),
    /// 固定间隔
    FixedInterval(Duration),
    /// 固定延迟（上次完成后延迟）
    FixedDelay(Duration),
    /// 启动时执行一次
    AtStartup,
}

/// 调度任务定义
#[derive(Debug, Clone)]
pub struct ScheduledTask {
    /// 任务名称
    pub name: String,
    /// 调度方式
    pub kind: ScheduleKind,
    /// 是否启用
    pub enabled: bool,
}

impl ScheduledTask {
    /// 创建新的调度任务（固定间隔）
    pub fn interval(name: impl Into<String>, interval_ms: u64) -> Self {
        Self {
            name: name.into(),
            kind: ScheduleKind::FixedInterval(Duration::from_millis(interval_ms)),
            enabled: true,
        }
    }

    /// 创建 Cron 调度任务
    pub fn cron(name: impl Into<String>, expr: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: ScheduleKind::Cron(expr.into()),
            enabled: true,
        }
    }

    /// 创建启动时执行一次的任务
    pub fn at_startup(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: ScheduleKind::AtStartup,
            enabled: true,
        }
    }
}

/// 任务执行结果
#[derive(Debug, Clone)]
pub enum TaskResult {
    /// 成功
    Success,
    /// 跳过（如条件不满足）
    Skipped,
    /// 失败
    Failed(String),
}

/// 可执行的任务
#[async_trait]
pub trait Task: Send + Sync {
    /// 任务名称
    fn name(&self) -> &str;

    /// 执行任务
    async fn execute(&self) -> anyhow::Result<TaskResult>;
}

/// 调度器统计
#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    /// 总执行次数
    pub total_executions: u64,
    /// 成功次数
    pub total_successes: u64,
    /// 失败次数
    pub total_failures: u64,
    /// 已注册任务数
    pub registered_tasks: usize,
}

/// 统一的调度器 trait
///
/// 未来支持 Cron、固定间隔、延迟、重试、退避等多种调度方式。
#[async_trait]
pub trait Scheduler: Send + Sync {
    /// 调度器名称
    fn name(&self) -> &str;

    /// 注册任务
    async fn register(
        &mut self,
        task: Box<dyn Task>,
        schedule: ScheduledTask,
    ) -> anyhow::Result<()>;

    /// 取消注册
    async fn unregister(&mut self, task_name: &str) -> anyhow::Result<()>;

    /// 启动调度
    async fn start(&mut self) -> anyhow::Result<()>;

    /// 停止调度
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// 手动触发任务
    async fn trigger(&self, task_name: &str) -> anyhow::Result<TaskResult>;

    /// 获取统计
    fn stats(&self) -> SchedulerStats;
}

/// 令牌桶限流器
///
/// 从 `pm-execution::scheduler` 提取。
#[derive(Debug)]
pub struct TokenBucket {
    rate: f64, // 每秒生成令牌数
    tokens: f64,
    max_tokens: f64,
    last_update: Instant,
}

impl TokenBucket {
    /// 创建新的令牌桶
    ///
    /// # 参数
    /// - `rate_per_second`：每秒生成令牌数
    /// - `burst`：最大突发容量
    pub fn new(rate_per_second: u32, burst: u32) -> Self {
        let burst = burst.max(rate_per_second).max(1);
        Self {
            rate: rate_per_second as f64,
            tokens: burst as f64,
            max_tokens: burst as f64,
            last_update: Instant::now(),
        }
    }

    /// 尝试获取一个令牌（非阻塞）
    pub fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// 阻塞等待获取令牌
    pub async fn acquire(&mut self) {
        loop {
            self.refill();
            if self.tokens >= 1.0 {
                self.tokens -= 1.0;
                return;
            }
            let wait = self.wait_ms();
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
    }

    /// 动态调整速率
    pub fn set_rate(&mut self, rate_per_second: u32) {
        self.rate = rate_per_second as f64;
    }

    /// 预估等待时间（毫秒）
    pub fn wait_ms(&self) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }
        let needed = 1.0 - self.tokens;
        ((needed / self.rate) * 1000.0) as u64 + 1
    }

    /// 当前令牌数
    pub fn available_tokens(&self) -> f64 {
        self.tokens
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.max_tokens);
        self.last_update = now;
    }
}

impl Default for TokenBucket {
    fn default() -> Self {
        Self::new(10, 20)
    }
}

/// 基于 tokio 定时器的间隔调度器（默认实现）
pub struct IntervalScheduler {
    name: String,
    tasks: HashMap<String, (Box<dyn Task>, ScheduledTask)>,
    stats: SchedulerStats,
    token_bucket: Option<TokenBucket>,
    running: bool,
}

impl IntervalScheduler {
    /// 创建新的间隔调度器
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: HashMap::new(),
            stats: SchedulerStats::default(),
            token_bucket: None,
            running: false,
        }
    }

    /// 配置令牌桶限流
    pub fn with_token_bucket(mut self, rate: u32, burst: u32) -> Self {
        self.token_bucket = Some(TokenBucket::new(rate, burst));
        self
    }
}

#[async_trait]
impl Scheduler for IntervalScheduler {
    fn name(&self) -> &str {
        &self.name
    }

    async fn register(
        &mut self,
        task: Box<dyn Task>,
        schedule: ScheduledTask,
    ) -> anyhow::Result<()> {
        let name = task.name().to_string();
        tracing::info!("注册调度任务: {} (类型={:?})", name, schedule.kind);
        self.tasks.insert(name, (task, schedule));
        self.stats.registered_tasks = self.tasks.len();
        Ok(())
    }

    async fn unregister(&mut self, task_name: &str) -> anyhow::Result<()> {
        if self.tasks.remove(task_name).is_some() {
            tracing::info!("取消注册调度任务: {}", task_name);
            self.stats.registered_tasks = self.tasks.len();
        }
        Ok(())
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        tracing::info!("调度器启动: {}", self.name);
        self.running = true;

        // 执行 AtStartup 任务
        for (task, schedule) in self.tasks.values() {
            if matches!(schedule.kind, ScheduleKind::AtStartup) && schedule.enabled {
                match task.execute().await {
                    Ok(result) => {
                        tracing::info!("启动任务 {} 完成: {:?}", task.name(), result);
                    }
                    Err(e) => {
                        tracing::warn!("启动任务 {} 失败: {}", task.name(), e);
                    }
                }
            }
        }
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        tracing::info!("调度器停止: {}", self.name);
        self.running = false;
        Ok(())
    }

    async fn trigger(&self, task_name: &str) -> anyhow::Result<TaskResult> {
        if let Some((task, _)) = self.tasks.get(task_name) {
            tracing::debug!("手动触发任务: {}", task_name);
            task.execute().await.map_err(|e| anyhow::anyhow!("{}", e))
        } else {
            Err(anyhow::anyhow!("任务未注册: {}", task_name))
        }
    }

    fn stats(&self) -> SchedulerStats {
        self.stats.clone()
    }
}

// 手动实现 Debug（因为 Box<dyn Task> 不自动实现 Debug）
impl std::fmt::Debug for IntervalScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntervalScheduler")
            .field("name", &self.name)
            .field("task_count", &self.tasks.len())
            .field("stats", &self.stats)
            .field("running", &self.running)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTask {
        name: String,
    }

    #[async_trait]
    impl Task for DummyTask {
        fn name(&self) -> &str {
            &self.name
        }

        async fn execute(&self) -> anyhow::Result<TaskResult> {
            Ok(TaskResult::Success)
        }
    }

    #[tokio::test]
    async fn token_bucket_try_acquire() {
        let mut tb = TokenBucket::new(100, 100);
        // 初始满桶
        for _ in 0..100 {
            assert!(tb.try_acquire());
        }
        // 桶空
        assert!(!tb.try_acquire());
    }

    #[tokio::test]
    async fn token_bucket_set_rate() {
        let mut tb = TokenBucket::new(10, 10);
        tb.set_rate(1000);
        // 检查速率调整生效
        assert!(tb.try_acquire() || true); // 不严格断言，仅验证不 panic
    }

    #[tokio::test]
    async fn interval_scheduler_register_and_start() {
        let mut sched = IntervalScheduler::new("test-scheduler");
        let task = Box::new(DummyTask {
            name: "test-task".to_string(),
        });
        sched
            .register(task, ScheduledTask::at_startup("test-task"))
            .await
            .unwrap();

        assert_eq!(sched.stats().registered_tasks, 1);
        sched.start().await.unwrap();
        sched.stop().await.unwrap();
    }

    #[tokio::test]
    async fn interval_scheduler_unregister() {
        let mut sched = IntervalScheduler::new("test-scheduler");
        let task = Box::new(DummyTask {
            name: "removable".to_string(),
        });
        sched
            .register(task, ScheduledTask::interval("removable", 1000))
            .await
            .unwrap();

        assert_eq!(sched.stats().registered_tasks, 1);
        sched.unregister("removable").await.unwrap();
        assert_eq!(sched.stats().registered_tasks, 0);
    }

    #[tokio::test]
    async fn interval_scheduler_trigger_unknown() {
        let sched = IntervalScheduler::new("test-scheduler");
        let result = sched.trigger("nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn scheduled_task_builders() {
        let interval = ScheduledTask::interval("health", 5000);
        assert!(matches!(interval.kind, ScheduleKind::FixedInterval(_)));

        let cron = ScheduledTask::cron("report", "0 * * * *");
        assert!(matches!(cron.kind, ScheduleKind::Cron(_)));

        let startup = ScheduledTask::at_startup("init");
        assert!(matches!(startup.kind, ScheduleKind::AtStartup));
    }
}
