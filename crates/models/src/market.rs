//! 市场数据模型：Gamma API 市场结构与单轮机会快照。
//!
//! `Market` 仅保留 Scanner 真正用到的字段，其余由 serde 忽略。
//! `OppSnapshot` 是一轮扫描中某个机会的瞬时快照（喂给 Tracker 的输入）。

use serde::Deserialize;

/// 单个市场的最小化数据结构（仅保留需要的字段，其余由 serde 忽略）。
///
/// V1.02：补 `id` / `condition_id` / `description` / `category`（均 `#[serde(default)]`），
/// 供 `GammaProvider` 转换为 [`crate::datasource::UnifiedMarket`]。原字段语义不变。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    /// Gamma 市场 id（数值字符串）。
    #[serde(default)]
    pub id: Option<String>,
    /// conditionId -- CLOB 订单簿/价格查询所用的稳定标识。
    #[serde(default)]
    pub condition_id: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    /// 市场描述（部分市场缺失）。
    #[serde(default)]
    pub description: Option<String>,
    /// 分类（部分市场缺失）。
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
    /// outcomePrices 是 JSON 编码的字符串，如 `"[\"0.43\",\"0.57\"]"`。
    /// 注意：归一化中间价，YES+NO 恒为 1.0，无法体现套利（见 scanner 模块注释）。
    #[serde(default)]
    pub outcome_prices: Option<String>,
    /// 成交额（数值字段，缺失视为 0）。
    #[serde(default)]
    pub volume_num: f64,
    /// 流动性（数值字段，缺失视为 0）。
    #[serde(default)]
    pub liquidity_num: f64,
}

impl Market {
    /// 解析 outcomePrices，返回二元市场的 (YES, NO) 价格。
    /// 仅当恰好 2 个价格时返回（多元市场不参与 YES/NO 套利判定）。
    pub fn yes_no_prices(&self) -> Option<(f64, f64)> {
        let raw = self.outcome_prices.as_ref()?;
        // 兼容字符串与数字两种元素写法
        let values: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;
        if values.len() != 2 {
            return None;
        }
        Some((to_f64(&values[0])?, to_f64(&values[1])?))
    }
}

/// 把 JSON 值（字符串或数字）转为 f64。
fn to_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// 一轮扫描中某个机会的瞬时快照（喂给 Tracker 的输入）。
#[derive(Debug, Clone)]
pub struct OppSnapshot {
    pub question: String,
    pub yes_price: f64,
    pub no_price: f64,
    pub sum: f64,
    pub volume: f64,
    pub liquidity: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_yes_no_prices_string_form() {
        let m = Market {
            id: None,
            condition_id: None,
            question: Some("Q".into()),
            description: None,
            category: None,
            active: true,
            closed: false,
            outcome_prices: Some(r#"["0.43","0.57"]"#.into()),
            volume_num: 100.0,
            liquidity_num: 50.0,
        };
        assert_eq!(m.yes_no_prices(), Some((0.43, 0.57)));
    }

    #[test]
    fn market_yes_no_prices_number_form() {
        let m = Market {
            id: None,
            condition_id: None,
            question: None,
            description: None,
            category: None,
            active: true,
            closed: false,
            outcome_prices: Some("[0.2, 0.8]".into()),
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        assert_eq!(m.yes_no_prices(), Some((0.2, 0.8)));
    }

    #[test]
    fn market_yes_no_prices_multi_outcome_is_none() {
        let m = Market {
            id: None,
            condition_id: None,
            question: None,
            description: None,
            category: None,
            active: true,
            closed: false,
            outcome_prices: Some(r#"["0.1","0.2","0.7"]"#.into()),
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        assert_eq!(m.yes_no_prices(), None);
    }

    #[test]
    fn market_yes_no_prices_missing_is_none() {
        let m = Market {
            id: None,
            condition_id: None,
            question: None,
            description: None,
            category: None,
            active: true,
            closed: false,
            outcome_prices: None,
            volume_num: 0.0,
            liquidity_num: 0.0,
        };
        assert_eq!(m.yes_no_prices(), None);
    }
}
