# API Workflow 设计文档

> P2-02 API Workflow Certification | 更新: 2026-07-23

## 1. 目标

验证所有 Polymarket API 能组成完整交易生命周期（Workflow），为下一阶段 Exchange Gateway 开发提供稳定、可靠的 API 调用流程基础。

## 2. 职责边界

| 维度 | 说明 |
|------|------|
| 依赖 | **仅依赖** 已认证的 `pm-api-test`（ApiClient / ResponseValidator / LiveGuard） |
| 禁止依赖 | Strategy / Risk / Gateway / Execution |
| 职责 | API 调用流程（状态机 + 录制 + 校验 + 报告） |
| 不做 | 不开发真实交易策略、不修改已有架构、不开发新业务模块 |

## 3. 模块结构

```
crates/api-workflow/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 导出 + prelude + 便利函数（run_dryrun/run_replay/run_live_readonly）
│   ├── config.rs           # WorkflowConfig / WorkflowMode（从 workflow.toml 加载）
│   ├── engine.rs           # WorkflowEngine：驱动状态机 + 录制器 + 校验器
│   ├── state_machine/mod.rs# WorkflowState + 转换表 + StateMachine
│   ├── recorder/mod.rs     # StepRecord / ApiCallRecord / WorkflowRecorder / WorkflowTrace
│   ├── validator/mod.rs    # WorkflowValidator + 完整性规则 + ValidationReport
│   ├── report/
│   │   ├── mod.rs
│   │   ├── types.rs        # WorkflowReport / StepSummary
│   │   └── generator.rs    # Markdown / JSON / Trace 生成（全中文）
│   └── workflows/
│       ├── mod.rs          # Workflow trait + fixtures 校验
│       ├── dryrun.rs       # DryRunWorkflow
│       ├── replay.rs       # ReplayWorkflow
│       └── live.rs         # LiveReadOnlyWorkflow
└── tests/                  # 六个自动化测试
```

## 4. 三种 Workflow

| Workflow | 客户端 | 网络 | 下单 | 用途 |
|----------|--------|------|------|------|
| DryRun | Mock | 否 | DryRun（构建+校验，不发送） | 默认，验证完整生命周期 |
| Replay | Mock（fixtures） | 否 | DryRun | 从 fixtures 确定性回放 |
| LiveReadOnly | Live（如 `enable_live_reads=true`） | 是 | **禁止** | 真实只读：Markets / OrderBook / Balance / Position |

## 5. 安全

- 默认 DryRun，禁止真实下单。
- LiveReadOnly 任何情况下 `enable_live=false`（永不真实下单）；仅允许 GET。
- 提交订单步骤仅构建请求并校验参数，不发送（`dry_run=true`）。
- Workflow 校验器强制：DryRun/Replay 中所有写操作必须 `dry_run=true`；LiveReadOnly 中禁止任何写操作。

## 6. 可重复执行 / 可追踪

- 所有 Workflow 可重复执行，报告覆盖写入 `reports/workflow/`。
- 每一步记录：开始/结束时间、耗时、API 请求/响应、失败原因（Workflow Trace）。
- 状态机记录全部状态转换历史。
