//! Polymarket REST API 客户端（V1.08 第三节）。
//!
//! 封装所有 Polymarket CLOB REST API 调用。
//! 负责：认证头 / 请求签名 / 错误处理。
//! 禁止泄漏到 Execution 层。

use reqwest::Client;
use tracing;

use crate::adapter::{PolymarketBalanceJson, PolymarketOrderJson, PolymarketPositionJson};
use crate::config::GatewayConfig;

/// Polymarket REST API 客户端。
pub struct PolymarketRestClient {
    /// HTTP 客户端。
    client: Client,
    /// API 基础 URL。
    base_url: String,
    /// API 密钥（从环境变量读取）。
    api_key: String,
    /// API 密钥环境变量名。
    #[allow(dead_code)]
    api_key_env: String,
    /// API 超时（毫秒）。
    #[allow(dead_code)]
    timeout_ms: u64,
}

impl PolymarketRestClient {
    /// 创建新的 REST 客户端。
    pub fn new(cfg: &GatewayConfig) -> Self {
        let api_key = std::env::var(&cfg.api_key_env).unwrap_or_default();

        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(cfg.api_timeout_ms))
            .build()
            .expect("Failed to build reqwest Client");

        tracing::info!(
            base_url = %cfg.polymarket_api_url,
            timeout_ms = %cfg.api_timeout_ms,
            "Polymarket REST 客户端已创建"
        );

        Self {
            client,
            base_url: cfg.polymarket_api_url.clone(),
            api_key,
            api_key_env: cfg.api_key_env.clone(),
            timeout_ms: cfg.api_timeout_ms,
        }
    }

    /// 获取认证头（若 API 密钥存在）。
    fn auth_headers(&self) -> Vec<(&str, String)> {
        let mut headers = Vec::new();
        if !self.api_key.is_empty() {
            headers.push(("Authorization", format!("Bearer {}", self.api_key)));
        }
        headers
    }

    // ---- 订单操作 ----

    /// 创建订单（POST /order）。
    pub async fn create_order(
        &self,
        token_id: &str,
        price: f64,
        size: f64,
        side: &str,
        order_type: &str,
    ) -> Result<PolymarketOrderJson, String> {
        let url = format!("{}/order", self.base_url);
        let body = serde_json::json!({
            "token_id": token_id,
            "price": format!("{:.4}", price),
            "size": format!("{:.2}", size),
            "side": side,
            "type": order_type,
        });

        let mut req = self.client.post(&url).json(&body);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }

        tracing::debug!(
            url = %url,
            token_id = %token_id,
            side = %side,
            price = %price,
            size = %size,
            "Polymarket 下单请求"
        );

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP 请求失败: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %body, "Polymarket 下单失败");
            return Err(format!(
                "Polymarket 下单失败: HTTP {} — {}",
                status.as_u16(),
                body
            ));
        }

        let order: PolymarketOrderJson = resp
            .json()
            .await
            .map_err(|e| format!("JSON 解析失败: {}", e))?;

        tracing::info!(
            order_id = %order.id,
            status = %order.status,
            "Polymarket 下单成功"
        );

        Ok(order)
    }

    /// 取消订单（DELETE /order/{order_id}）。
    pub async fn cancel_order(&self, order_id: &str) -> Result<PolymarketOrderJson, String> {
        let url = format!("{}/order/{}", self.base_url, order_id);

        let mut req = self.client.delete(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP 请求失败: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("取消订单失败: HTTP {}", resp.status().as_u16()));
        }

        let order: PolymarketOrderJson = resp
            .json()
            .await
            .map_err(|e| format!("JSON 解析失败: {}", e))?;

        tracing::info!(order_id = %order_id, "Polymarket 订单已取消");
        Ok(order)
    }

    /// 查询单个订单（GET /order/{order_id}）。
    pub async fn get_order(&self, order_id: &str) -> Result<PolymarketOrderJson, String> {
        let url = format!("{}/order/{}", self.base_url, order_id);

        let mut req = self.client.get(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP 请求失败: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("查询订单失败: HTTP {}", resp.status().as_u16()));
        }

        let order: PolymarketOrderJson = resp
            .json()
            .await
            .map_err(|e| format!("JSON 解析失败: {}", e))?;
        Ok(order)
    }

    /// 查询所有活跃订单（GET /orders）。
    pub async fn list_orders(&self) -> Result<Vec<PolymarketOrderJson>, String> {
        let url = format!("{}/orders", self.base_url);

        let mut req = self.client.get(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP 请求失败: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("查询订单列表失败: HTTP {}", resp.status().as_u16()));
        }

        let orders: Vec<PolymarketOrderJson> = resp
            .json()
            .await
            .map_err(|e| format!("JSON 解析失败: {}", e))?;

        tracing::debug!(count = %orders.len(), "Polymarket 订单列表查询成功");
        Ok(orders)
    }

    // ---- 余额 ----

    /// 查询余额（GET /balance）。
    pub async fn get_balance(&self) -> Result<PolymarketBalanceJson, String> {
        let url = format!("{}/balance", self.base_url);

        let mut req = self.client.get(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP 请求失败: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("查询余额失败: HTTP {}", resp.status().as_u16()));
        }

        let balance: PolymarketBalanceJson = resp
            .json()
            .await
            .map_err(|e| format!("JSON 解析失败: {}", e))?;

        tracing::debug!(
            available = %balance.available,
            total = %balance.total,
            "Polymarket 余额查询成功"
        );

        Ok(balance)
    }

    // ---- 持仓 ----

    /// 查询持仓（GET /positions）。
    pub async fn get_positions(&self) -> Result<Vec<PolymarketPositionJson>, String> {
        let url = format!("{}/positions", self.base_url);

        let mut req = self.client.get(&url);
        for (k, v) in self.auth_headers() {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP 请求失败: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("查询持仓失败: HTTP {}", resp.status().as_u16()));
        }

        let positions: Vec<PolymarketPositionJson> = resp
            .json()
            .await
            .map_err(|e| format!("JSON 解析失败: {}", e))?;

        tracing::debug!(count = %positions.len(), "Polymarket 持仓查询成功");
        Ok(positions)
    }

    // ---- 健康检查 ----

    /// Ping API（GET /ping 或 /time）。
    pub async fn ping(&self) -> bool {
        let url = format!("{}/time", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// 获取 API 基础 URL。
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 是否已配置 API 密钥。
    pub fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}
