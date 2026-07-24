//! Gateway Transport 抽象层（P2-03）。
//!
//! 统一封装 HTTP 和 WebSocket 通信。
//! 业务层（PolymarketGateway）禁止直接访问 reqwest 或 tungstenite。
//!
//! # 模块
//!
//! - [`rest`]：HTTP Transport trait + ReqwestTransport 实现。
//! - [`websocket`]：WebSocket Transport trait + 实现。

pub mod rest;
pub mod websocket;