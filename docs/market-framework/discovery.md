# Market Discovery（P3.0 第八节）

## 概述

启动时自动发现所有已注册的 MarketPlugin。系统无需手动修改代码即可新增市场。

## 当前实现

- **静态注册模式**：插件通过代码显式注册到 `MarketRegistry`
- **动态加载预留**：接口已设计，未来支持从 `.so` / `.dll` 动态加载

## API

```rust
// 发现所有已注册插件
let result = Discovery::discover_all(&registry);

// 按能力过滤
let result = Discovery::discover_by_capability(&registry, &MarketCapability::LiveTrading);

// 发现并生成中文报告
let report = Discovery::discover_and_report(&registry);
```

## DiscoveryResult

```rust
pub struct DiscoveryResult {
    pub plugin_ids: Vec<String>,    // 发现的插件 ID
    pub discovered_at: String,       // 发现时间
    pub elapsed_ms: u64,             // 耗时
    pub errors: Vec<String>,         // 错误列表
}
```

## 发现报告示例

```
【市场发现】2026-07-25 10:30:00
  结果: ✅ 发现 3 个市场插件
  耗时: 1ms

  已发现插件:
    1. polymarket-v1
    2. binance-v1
    3. okx-v1
```

## 未来动态加载设计

```rust
// 预留接口
pub trait DynamicPlugin: MarketPlugin {
    fn library_path(&self) -> &str;  // .so/.dll 路径
    fn load_symbols(&self) -> Result<()>;
}

// 动态发现
struct DynamicDiscovery {
    plugin_dir: PathBuf,
}
```

## 发现流程

1. 调用 `Discovery::discover_all()`
2. 遍历 `MarketRegistry` 中所有已注册插件
3. 输出发现报告（中文）
4. 返回 `DiscoveryResult`
