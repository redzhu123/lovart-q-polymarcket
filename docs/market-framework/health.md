# Market Health（P3.0 第七节）

## 概述

统一所有市场的健康检查，覆盖六个维度：

1. **REST API** — HTTP/HTTPS 连接状态
2. **WebSocket** — 实时推送连接状态
3. **Gateway** — 交易所网关状态
4. **Authentication** — API Key / Token 有效性
5. **Latency** — 请求响应延迟
6. **Streaming** — gRPC stream / SSE 状态

## HealthDimension

```rust
pub enum HealthDimension {
    Rest,
    WebSocket,
    Gateway,
    Authentication,
    Latency,
    Streaming,
}
```

## DimensionCheck

```rust
// 成功的检查
let check = DimensionCheck::ok(HealthDimension::Rest, 10);

// 失败的检查
let check = DimensionCheck::fail(HealthDimension::WebSocket, "连接超时");

// 不适用的检查
let check = DimensionCheck::not_applicable(HealthDimension::Streaming);
```

## MarketHealthReport

```rust
// 快速创建健康报告
let report = MarketHealthReport::healthy("Polymarket");

// 详细报告
let checks = vec![
    DimensionCheck::ok(HealthDimension::Rest, 5),
    DimensionCheck::ok(HealthDimension::WebSocket, 12),
    DimensionCheck::ok(HealthDimension::Gateway, 1),
    DimensionCheck::fail(HealthDimension::Authentication, "Token 过期"),
];
let report = MarketHealthReport::with_checks("Binance", checks);

// 检查状态
assert!(!report.overall_healthy());
println!("{}", report.report_zh());
```

## 健康报告示例

```
══════ Binance 健康报告 ══════
时间: 2026-07-25 10:30:00
总体状态: ❌ 异常

--- 维度检查 ---
  ✅ REST API           正常 (5ms)
  ✅ WebSocket          正常 (12ms)
  ✅ 网关               正常 (1ms)
  ❌ 认证               Token 过期 (0ms)

健康率: 3/4
════════════════════════════
```

## 合并报告

```rust
let reports = vec![
    MarketHealthReport::healthy("Polymarket"),
    MarketHealthReport::healthy("Binance"),
];
let merged = MarketHealthReport::merge(&reports);
println!("{}", merged.report_zh());
// 输出: "全部市场（2 个）"
```

## CLI 使用

```bash
# 多市场健康检查
cargo run -- markets health
```

## MarketHealthStatus

| 状态 | Emoji | 说明 |
|------|-------|------|
| `Healthy` | ✅ | 所有检查通过 |
| `Degraded` | ⚠️ | 部分功能正常 |
| `Unhealthy` | ❌ | 无法正常服务 |
| `Unknown` | ❓ | 尚未执行检查 |
