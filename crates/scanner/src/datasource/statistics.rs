//! Data Statistics（V1.02 第十节）。
//!
//! 每轮扫描的市场数据统计：数据源 / Market / OrderBook / Price / Liquidity / Invalid / Cached / 更新时间。
//! 全中文打印，便于一眼看清本轮数据规模与质量。

use chrono::{DateTime, Local};

use pm_models::UnifiedMarket;

use crate::display::{DASH, SEP};

/// 一轮市场数据统计。
#[derive(Debug, Clone)]
pub struct DataStatistics {
    pub provider: String,
    pub market_count: usize,
    pub orderbook_count: usize,
    pub price_count: usize,
    pub liquidity_count: usize,
    pub invalid_count: usize,
    pub cached_count: usize,
    pub updated_at: String,
}

impl DataStatistics {
    /// 从本轮市场列表 + 校验/缓存计数构造。
    pub fn build(
        markets: &[UnifiedMarket],
        provider: &str,
        invalid_count: usize,
        cached_count: usize,
        now: DateTime<Local>,
    ) -> Self {
        let price_count = markets.iter().filter(|m| m.has_prices()).count();
        let liquidity_count = markets.iter().filter(|m| m.liquidity > 0.0).count();
        Self {
            provider: provider.into(),
            market_count: markets.len(),
            // 扫描循环未拉取订单簿（Gamma 不支持）； datasource 诊断单独统计
            orderbook_count: 0,
            price_count,
            liquidity_count,
            invalid_count,
            cached_count,
            updated_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    /// 打印【市场数据统计】区块（第十节）。
    pub fn print_block(&self) {
        println!("{}", SEP);
        println!();
        println!("市场数据统计");
        println!();
        println!("{}", DASH);
        println!();
        println!("数据源    : {}", self.provider);
        println!("Market    : {}", self.market_count);
        println!("OrderBook : {}", self.orderbook_count);
        println!("Price     : {}", self.price_count);
        println!("Liquidity : {}", self.liquidity_count);
        println!("Invalid   : {}", self.invalid_count);
        println!("Cached    : {}", self.cached_count);
        println!("更新时间  : {}", self.updated_at);
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    fn um(id: &str, yes: Option<f64>, no: Option<f64>, liq: f64) -> UnifiedMarket {
        UnifiedMarket {
            market_id: id.into(),
            question: id.into(),
            description: None,
            status: MarketStatus::Active,
            yes_price: yes,
            no_price: no,
            volume: 0.0,
            liquidity: liq,
            category: None,
            outcome_count: if yes.is_some() && no.is_some() { 2 } else { 0 },
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn counts_price_and_liquidity() {
        let markets = vec![
            um("a", Some(0.4), Some(0.5), 100.0),  // 有价 + 有流动性
            um("b", Some(0.4), Some(0.5), 0.0),    // 有价 + 无流动性
            um("c", None, None, 50.0),             // 无价 + 有流动性
        ];
        let s = DataStatistics::build(&markets, "gamma", 1, 0, Local::now());
        assert_eq!(s.market_count, 3);
        assert_eq!(s.price_count, 2);     // a, b
        assert_eq!(s.liquidity_count, 2); // a, c
        assert_eq!(s.invalid_count, 1);
        assert_eq!(s.cached_count, 0);
        assert_eq!(s.provider, "gamma");
        assert_eq!(s.orderbook_count, 0);
    }

    #[test]
    fn empty_markets_zero_counts() {
        let s = DataStatistics::build(&[], "gamma", 0, 0, Local::now());
        assert_eq!(s.market_count, 0);
        assert_eq!(s.price_count, 0);
    }
}
