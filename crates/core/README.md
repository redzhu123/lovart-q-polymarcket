# pm-core

跨 crate 公共原语。

## 职责
- `Side`：订单方向（Buy/Sell）。被 `pm-portfolio` 与 `pm-execution` 共用，置于 core 避免二者相互依赖。
- `CoreError`：跨 crate 通用错误（thiserror）。
- 模拟标记等无业务行为的底层类型。

## 依赖
`thiserror`, `serde`。无内部 crate 依赖（叶子 crate）。

## 用途
被几乎所有 crate 间接或直接引用，提供最底层的共享类型。不放任何业务逻辑；业务 struct 归各 engine crate，共享 DTO 归 `pm-models`。

## 设计约束
- 禁止 `unwrap/expect/panic`，全部 `Result`。
- 保持极小：只放真正跨 crate 共享且无行为的类型。
