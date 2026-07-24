//! 生命周期管理：统一管理所有模块的生命周期。
//!
//! 从 `pm-oms::lifecycle` 和 `pm-trading::state` 提取并统一。
//!
//! # 生命周期状态
//!
//! Uninitialized → Initializing → Ready → Running
//!                                   ↓        ↓
//!                                 Pausing → Paused
//!                                   ↓        ↓
//!                                Stopping → Stopped
//!                                   ↓
//!                               Recovery → Ready
//!                                   ↓
//!                               Shutdown（终端）

use async_trait::async_trait;

/// 生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LifecycleState {
    /// 未初始化
    Uninitialized = 0,
    /// 初始化中
    Initializing = 1,
    /// 就绪（已初始化，未启动）
    Ready = 2,
    /// 运行中
    Running = 3,
    /// 暂停中
    Pausing = 4,
    /// 已暂停
    Paused = 5,
    /// 停止中
    Stopping = 6,
    /// 已停止
    Stopped = 7,
    /// 恢复中（从错误恢复）
    Recovery = 8,
    /// 已关闭（终端状态）
    Shutdown = 9,
}

impl LifecycleState {
    /// 中文名称
    pub fn zh(&self) -> &'static str {
        match self {
            LifecycleState::Uninitialized => "未初始化",
            LifecycleState::Initializing => "初始化中",
            LifecycleState::Ready => "就绪",
            LifecycleState::Running => "运行中",
            LifecycleState::Pausing => "暂停中",
            LifecycleState::Paused => "已暂停",
            LifecycleState::Stopping => "停止中",
            LifecycleState::Stopped => "已停止",
            LifecycleState::Recovery => "恢复中",
            LifecycleState::Shutdown => "已关闭",
        }
    }

    /// 是否可操作（Ready 或 Running）
    pub fn is_operational(&self) -> bool {
        matches!(self, LifecycleState::Ready | LifecycleState::Running)
    }

    /// 是否为终端状态
    pub fn is_terminal(&self) -> bool {
        matches!(self, LifecycleState::Stopped | LifecycleState::Shutdown)
    }
}

/// 生命周期 trait
///
/// 所有模块统一实现此接口，由 LifecycleManager 统一管理。
#[async_trait]
pub trait Lifecycle: Send + Sync {
    /// 组件名称
    fn name(&self) -> &str;

    /// 当前状态
    fn state(&self) -> LifecycleState;

    /// 初始化
    async fn initialize(&mut self) -> anyhow::Result<()>;

    /// 启动
    async fn start(&mut self) -> anyhow::Result<()>;

    /// 暂停
    async fn pause(&mut self) -> anyhow::Result<()>;

    /// 恢复
    async fn resume(&mut self) -> anyhow::Result<()>;

    /// 停止
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// 关闭（释放资源）
    async fn shutdown(&mut self) -> anyhow::Result<()>;

    /// 恢复（从错误中恢复）
    async fn recover(&mut self) -> anyhow::Result<()>;
}

/// 生命周期管理器
///
/// 统一管理所有注册组件的生命周期。
pub struct LifecycleManager {
    components: Vec<Box<dyn Lifecycle>>,
    state: LifecycleState,
}

impl LifecycleManager {
    /// 创建新的生命周期管理器
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            state: LifecycleState::Uninitialized,
        }
    }

    /// 注册组件
    pub fn register(&mut self, component: Box<dyn Lifecycle>) {
        tracing::info!("注册生命周期组件: {}", component.name());
        self.components.push(component);
    }

    /// 初始化所有组件
    pub async fn initialize_all(&mut self) -> anyhow::Result<()> {
        self.state = LifecycleState::Initializing;
        tracing::info!("开始初始化所有组件（共 {} 个）", self.components.len());

        for component in &mut self.components {
            tracing::info!("初始化组件: {}", component.name());
            component.initialize().await.map_err(|e| {
                tracing::error!("组件 {} 初始化失败: {}", component.name(), e);
                anyhow::anyhow!("组件 {} 初始化失败: {}", component.name(), e)
            })?;
        }

        self.state = LifecycleState::Ready;
        tracing::info!("所有组件初始化完成，状态: {}", self.state.zh());
        Ok(())
    }

    /// 启动所有组件
    pub async fn start_all(&mut self) -> anyhow::Result<()> {
        if self.state != LifecycleState::Ready && self.state != LifecycleState::Paused {
            anyhow::bail!("无法启动: 当前状态为 {}", self.state.zh());
        }

        tracing::info!("开始启动所有组件");
        for component in &mut self.components {
            tracing::info!("启动组件: {}", component.name());
            component.start().await.map_err(|e| {
                tracing::error!("组件 {} 启动失败: {}", component.name(), e);
                anyhow::anyhow!("组件 {} 启动失败: {}", component.name(), e)
            })?;
        }

        self.state = LifecycleState::Running;
        tracing::info!("所有组件已启动");
        Ok(())
    }

    /// 停止所有组件
    pub async fn stop_all(&mut self) -> anyhow::Result<()> {
        self.state = LifecycleState::Stopping;
        tracing::info!("开始停止所有组件");

        // 逆序停止（后启动的先停止）
        for component in self.components.iter_mut().rev() {
            tracing::info!("停止组件: {}", component.name());
            if let Err(e) = component.stop().await {
                tracing::warn!("组件 {} 停止失败: {}", component.name(), e);
            }
        }

        self.state = LifecycleState::Stopped;
        tracing::info!("所有组件已停止");
        Ok(())
    }

    /// 关闭所有组件（释放资源）
    pub async fn shutdown_all(&mut self) -> anyhow::Result<()> {
        tracing::info!("开始关闭所有组件");
        for component in self.components.iter_mut().rev() {
            tracing::info!("关闭组件: {}", component.name());
            if let Err(e) = component.shutdown().await {
                tracing::warn!("组件 {} 关闭失败: {}", component.name(), e);
            }
        }

        self.state = LifecycleState::Shutdown;
        tracing::info!("所有组件已关闭");
        Ok(())
    }

    /// 当前状态
    pub fn state(&self) -> LifecycleState {
        self.state
    }

    /// 组件数量
    pub fn component_count(&self) -> usize {
        self.components.len()
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestComponent {
        name: String,
        state: LifecycleState,
    }

    impl TestComponent {
        fn new(name: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                state: LifecycleState::Uninitialized,
            }
        }
    }

    #[async_trait]
    impl Lifecycle for TestComponent {
        fn name(&self) -> &str {
            &self.name
        }

        fn state(&self) -> LifecycleState {
            self.state
        }

        async fn initialize(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Ready;
            Ok(())
        }

        async fn start(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Running;
            Ok(())
        }

        async fn pause(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Paused;
            Ok(())
        }

        async fn resume(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Running;
            Ok(())
        }

        async fn stop(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Stopped;
            Ok(())
        }

        async fn shutdown(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Shutdown;
            Ok(())
        }

        async fn recover(&mut self) -> anyhow::Result<()> {
            self.state = LifecycleState::Ready;
            Ok(())
        }
    }

    #[tokio::test]
    async fn lifecycle_manager_full_cycle() {
        let mut mgr = LifecycleManager::new();
        assert_eq!(mgr.state(), LifecycleState::Uninitialized);

        mgr.register(Box::new(TestComponent::new("Gateway")));
        mgr.register(Box::new(TestComponent::new("OMS")));
        assert_eq!(mgr.component_count(), 2);

        mgr.initialize_all().await.unwrap();
        assert_eq!(mgr.state(), LifecycleState::Ready);

        mgr.start_all().await.unwrap();
        assert_eq!(mgr.state(), LifecycleState::Running);

        mgr.stop_all().await.unwrap();
        assert_eq!(mgr.state(), LifecycleState::Stopped);

        mgr.shutdown_all().await.unwrap();
        assert_eq!(mgr.state(), LifecycleState::Shutdown);
    }

    #[test]
    fn lifecycle_state_zh() {
        assert_eq!(LifecycleState::Running.zh(), "运行中");
        assert_eq!(LifecycleState::Shutdown.zh(), "已关闭");
    }

    #[test]
    fn lifecycle_state_is_operational() {
        assert!(LifecycleState::Ready.is_operational());
        assert!(LifecycleState::Running.is_operational());
        assert!(!LifecycleState::Uninitialized.is_operational());
    }

    #[test]
    fn lifecycle_state_is_terminal() {
        assert!(!LifecycleState::Running.is_terminal());
        assert!(LifecycleState::Stopped.is_terminal());
        assert!(LifecycleState::Shutdown.is_terminal());
    }
}
