# Polymarket Rate Limit 参考

> 最后更新: 2026-07-23

---

## 1. 概述

Polymarket 使用 **Cloudflare 滑动窗口限流**。超限的请求会被**延迟/排队**而非立即拒绝，但在极端超限情况下可能返回 HTTP 429。

所有限流窗口为 **10 秒滑动窗口**（除特别注明外）。

---

## 2. Gamma API 限流

**Base URL**: `https://gamma-api.polymarket.com`

| 端点 | 限制 | 窗口 |
|------|------|------|
| **通用** (所有端点) | 4,000 req | 10s |
| `/events` | 500 req | 10s |
| `/markets` | 300 req | 10s |
| `/markets` + `/events` 合计 | 900 req | 10s |
| `/comments` | 200 req | 10s |
| `/tags` | 200 req | 10s |
| `/public-search` | 350 req | 10s |

**当前项目影响**: `GammaProvider` 以每次 100 条、最多 50 页的方式拉取 markets 列表。按 10 秒间隔扫描，实际请求量远低于 300/10s 限制。

---

## 3. CLOB API 限流

**Base URL**: `https://clob.polymarket.com`

### 3.1 公开端点

| 端点 | 限制 | 窗口 |
|------|------|------|
| **通用** (所有公开端点) | 9,000 req | 10s |
| `/book` | 1,500 req | 10s |
| `/books` (批量) | 500 req | 10s |
| `/price` | 1,500 req | 10s |
| `/prices` (批量) | 500 req | 10s |
| `/midpoint` | 1,500 req | 10s |
| `/midpoints` (批量) | 500 req | 10s |
| `/prices-history` | 1,000 req | 10s |
| `/ok` (健康检查) | 100 req | 10s |

### 3.2 交易端点

| 端点 | Burst 限制 | 持续限制 | 窗口 |
|------|-----------|----------|------|
| `POST /order` | 5,000 req | 120,000 req | 10s / 10min |
| `DELETE /order` | 5,000 req | 120,000 req | 10s / 10min |
| `/cancel-all` | 250 req | - | 10s |
| `/cancel-market-orders` | 1,500 req | - | 10s |

### 3.3 当前项目影响

| 操作 | 预估频率 | 是否触限 |
|------|----------|----------|
| 订单簿获取 (REST) | 500 markets × 1 req/scan | 500/10s，远低于 1,500 |
| 批量订单簿 (优化后) | 1 req/scan | 远低于 500 |
| 下单 | < 10/scan | 远低于 5,000 |
| 撤单 | < 10/scan | 远低于 5,000 |

---

## 4. Data API 限流

**Base URL**: `https://data-api.polymarket.com`

Data API 未公布具体限流数值。建议保持在 **60-120 req/min** 以内。该 API 主要用于定时对账（分钟级）而非实时调用。

---

## 5. WebSocket 限流

### 5.1 连接限制

- 每个 IP 的 WebSocket 连接数限制未正式公布
- 建议保持 **≤ 5 个并发连接**

### 5.2 消息限制

- 订阅消息: 无明确限制，但建议在连接建立后**批量发送**（而非逐个订阅）
- PING/PONG: 必须每 **~10 秒**发送一次，不要更频繁

### 5.3 订阅数量

- Market Channel: 单次订阅的 `assets_ids` 数量未公布上限
- User Channel: `markets` 数量未公布上限，但建议单连接订阅 ≤ 200 个市场

---

## 6. 当前项目的 Rate Limiter 实现

### 6.1 已有实现

```rust
// crates/execution/src/scheduler.rs
pub struct SchedulerConfig {
    pub max_orders_per_second: u32,    // 默认 10
    pub max_orders_per_minute: u32,    // 0 = 不限制
    pub burst_size: u32,               // 默认 5
}
```

使用 Token Bucket 算法，`try_acquire()` 非阻塞，`acquire()` 异步阻塞。

### 6.2 需要新增

| 限流器 | 用途 | 建议配置 |
|--------|------|----------|
| `ClobRateLimiter` | CLOB 公开 API 调用 | Token bucket, 100 req/s |
| `GammaRateLimiter` | Gamma API 市场扫描 | Token bucket, 25 req/s |
| `OrderRateLimiter` | POST/DELETE /order | Token bucket, 50 req/s |
| `DataApiRateLimiter` | Data API 对账 | Token bucket, 2 req/s |

---

## 7. 最佳实践

### 7.1 批量优先

| 单次调用 | 批量替换 |
|----------|----------|
| `GET /book?token_id=A` + `GET /book?token_id=B` | `POST /books` (一次请求) |
| `GET /price?token_id=A` + `GET /price?token_id=B` | `POST /prices` (一次请求) |
| `DELETE /order` × N | `DELETE /orders` (一次请求) |

使用批量端点可减少 10-100 倍的请求数。

### 7.2 缓存策略

| 数据 | 缓存时间 | 原因 |
|------|----------|------|
| tick-size | 永久（直到 tick_size_change 事件） | 仅在极端价格时改变 |
| neg-risk | 永久 | 市场属性不变 |
| fee-rate-bps | 1 小时 | 很少改变 |
| order book | WebSocket 实时更新 + REST 5s 兜底 | 变化频繁 |
| balance | 30s | 变化较快 |

### 7.3 指数退避

```rust
fn retry_delay(attempt: u32) -> Duration {
    let base = 1_000; // 1 秒
    let max_delay = 60_000; // 60 秒
    let delay = base * 2u64.pow(attempt.min(6));
    Duration::from_millis(delay.min(max_delay))
}
```

### 7.4 HTTP 429 处理

收到 429 时:
1. 读取 `Retry-After` 响应头（如有）
2. 若无 `Retry-After`，使用指数退避
3. 暂停该端点的所有后续请求，直到退避时间结束
4. 考虑降低该端点的 Token Bucket 填充速率

---

## 8. 监控指标建议

| 指标 | 说明 |
|------|------|
| `api_requests_total` | 按端点统计的请求总数 |
| `api_rate_limit_remaining` | 从响应头提取的剩余配额 |
| `api_429_count` | 触发限流的次数 |
| `api_latency_p50/p99` | 请求延迟分布 |
| `ws_reconnects_total` | WebSocket 重连次数 |

---

## 9. 限流速查表

| API | 端点 | 每 10s 限制 | 每秒限制（近似） |
|-----|------|------------|-----------------|
| Gamma | 通用 | 4,000 | 400 |
| Gamma | /markets | 300 | 30 |
| Gamma | /events | 500 | 50 |
| CLOB | 通用 | 9,000 | 900 |
| CLOB | /book | 1,500 | 150 |
| CLOB | /books | 500 | 50 |
| CLOB | /price | 1,500 | 150 |
| CLOB | POST /order | 5,000 | 500 |
| CLOB | DELETE /order | 5,000 | 500 |
| CLOB | /cancel-all | 250 | 25 |
| CLOB | /prices-history | 1,000 | 100 |
| Data | 所有 | 未公布 | ~6-12（保守） |
| WS | PING | - | 0.1 (1/10s) |
