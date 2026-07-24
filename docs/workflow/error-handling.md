# Workflow 错误处理

> P2-02 | 更新: 2026-07-23

## 1. 错误分级

| 级别 | 触发 | 处理 |
|------|------|------|
| 步骤失败 | API 请求失败 / 非 2xx / 参数校验失败 | 记录失败原因，状态机进入 Failed，终止后续步骤 |
| 非法状态转换 | 状态机收到非法转换 | 记录错误，强制进入 Failed |
| 校验失败 | Workflow 完整性规则未满足 | 报告标记 `success=false`，列出失败规则 |
| 配置错误 | workflow.toml 解析失败 / 缺失 | 退化为安全默认（DryRun） |
| fixtures 缺失 | Replay 找不到 fixtures 文件 | 直接报错终止 |

## 2. 步骤失败处理

每个步骤（`StepRecord`）独立记录 `success` 与 `failure_reason`：

1. 步骤开始（记录开始时间）。
2. 执行 API 调用 / 本地构建。
3. 失败时调用 `step.fail(reason)`，记录原因。
4. `finish_step` 完成计时并录入 recorder；若失败，状态机 `force_failed`。
5. 生命周期编排遇到失败步骤即 short-circuit，不再执行后续步骤，直接 `finalize` 生成报告。

报告仍会生成（含已完成的步骤 + 失败步骤），`success=false`，健康评分下降。

## 3. 校验失败处理

`WorkflowValidator` 按模式校验 trace：

- **DryRun / Replay**：6 条规则（到达终态 / 含提交订单 / 提交后查订单状态 / 成交后同步持仓 / 持仓后同步余额 / 无真实写操作）。
- **LiveReadOnly**：5 条规则（到达终态 / 无写操作 / 读取市场 / 读取订单簿 / 无下单步骤）。

任一规则未满足 -> `ValidationReport.passed=false`，失败原因记入 `failures`，报告 `success=false`。

## 4. 健康评分（0-100）

初始 100，扣分项：

- 每个失败步骤：-8
- 每条校验失败：-15
- 平均步骤耗时 > 5000ms：-10
- 整体未成功：-20

下限 0。

## 5. 网络与认证错误

- LiveReadOnly 未认证（无 `POLYMARKET_API_KEY`）：自动跳过 Balance / Position 读取，步骤标记为「未认证，跳过」，不视为失败。
- Live 网络错误：步骤失败，记录错误，状态机进入 Failed。
- Mock 模式：无网络，fixtures 提供确定性响应。

## 6. 日志规范

- 统一 `tracing`，禁止 `println!`（CLI 用户面输出除外）。
- 状态转换、失败原因、耗时、API 顺序全部输出中文日志。
- 错误使用 `tracing::error!`，警告使用 `tracing::warn!`。
