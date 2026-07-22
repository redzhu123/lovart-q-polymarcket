//! Data Validator（V1.02 第七节）。
//!
//! 校验 [`UnifiedMarket`] 的数据合法性：question 非空、价格在 [0,1]、volume/liquidity 非负。
//! 非法数据**统计 + 打印**（tracing），但不擅自丢弃（下游 `find_opportunities` 自有过滤）。

use pm_models::UnifiedMarket;

/// 单条校验错误。
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub reason: String,
}

impl ValidationError {
    pub fn new(field: &str, reason: &str) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

/// 一批市场的校验报告。
#[derive(Debug, Clone, Default)]
pub struct ValidatorReport {
    /// 校验总数。
    pub total: usize,
    /// 合法数。
    pub valid: usize,
    /// 非法数。
    pub invalid: usize,
    /// 非法明细（market_id, error）。
    pub errors: Vec<(String, ValidationError)>,
}

impl ValidatorReport {
    /// 非法率（0.0..=1.0）。
    pub fn invalid_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.invalid as f64 / self.total as f64
    }
}

/// 数据校验器。
pub struct Validator;

impl Validator {
    /// 校验单个市场。
    pub fn validate(m: &UnifiedMarket) -> Result<(), ValidationError> {
        if m.question.trim().is_empty() {
            return Err(ValidationError::new("question", "问题为空"));
        }
        if let Some(y) = m.yes_price {
            if !(0.0..=1.0).contains(&y) {
                return Err(ValidationError::new("yes_price", "YES 价超出 [0,1]"));
            }
        }
        if let Some(n) = m.no_price {
            if !(0.0..=1.0).contains(&n) {
                return Err(ValidationError::new("no_price", "NO 价超出 [0,1]"));
            }
        }
        if m.volume < 0.0 {
            return Err(ValidationError::new("volume", "成交额为负"));
        }
        if m.liquidity < 0.0 {
            return Err(ValidationError::new("liquidity", "流动性为负"));
        }
        Ok(())
    }

    /// 校验一批市场，统计合法 / 非法，并对非法项打印 tracing 警告（中文）。
    pub fn validate_many(markets: &[UnifiedMarket]) -> ValidatorReport {
        let mut report = ValidatorReport {
            total: markets.len(),
            ..Default::default()
        };
        for m in markets {
            match Self::validate(m) {
                Ok(()) => report.valid += 1,
                Err(e) => {
                    report.invalid += 1;
                    tracing::warn!(
                        market_id = %m.market_id,
                        field = %e.field,
                        reason = %e.reason,
                        "非法市场数据已记录"
                    );
                    report.errors.push((m.market_id.clone(), e));
                }
            }
        }
        if report.invalid > 0 {
            tracing::warn!(
                invalid = report.invalid,
                total = report.total,
                "数据校验发现非法市场"
            );
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pm_models::MarketStatus;

    fn um(id: &str, question: &str, yes: Option<f64>, no: Option<f64>) -> UnifiedMarket {
        UnifiedMarket {
            market_id: id.into(),
            question: question.into(),
            description: None,
            status: MarketStatus::Active,
            yes_price: yes,
            no_price: no,
            volume: 0.0,
            liquidity: 0.0,
            category: None,
            outcome_count: 2,
            provider: "test".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn valid_market_passes() {
        assert!(Validator::validate(&um("1", "Q", Some(0.4), Some(0.5))).is_ok());
    }

    #[test]
    fn empty_question_invalid() {
        let err = Validator::validate(&um("1", "  ", Some(0.4), Some(0.5))).unwrap_err();
        assert_eq!(err.field, "question");
    }

    #[test]
    fn price_out_of_range_invalid() {
        let err = Validator::validate(&um("1", "Q", Some(1.5), Some(0.5))).unwrap_err();
        assert_eq!(err.field, "yes_price");
        let err = Validator::validate(&um("1", "Q", Some(0.4), Some(-0.1))).unwrap_err();
        assert_eq!(err.field, "no_price");
    }

    #[test]
    fn negative_volume_or_liquidity_invalid() {
        let mut m = um("1", "Q", Some(0.4), Some(0.5));
        m.volume = -1.0;
        assert_eq!(Validator::validate(&m).unwrap_err().field, "volume");
        m.volume = 0.0;
        m.liquidity = -5.0;
        assert_eq!(Validator::validate(&m).unwrap_err().field, "liquidity");
    }

    #[test]
    fn missing_prices_are_valid() {
        // 价格缺失（None）不算非法（仅范围校验存在值）
        assert!(Validator::validate(&um("1", "Q", None, None)).is_ok());
    }

    #[test]
    fn validate_many_counts() {
        let markets = vec![
            um("1", "Q1", Some(0.4), Some(0.5)),    // valid
            um("2", "", Some(0.4), Some(0.5)),      // invalid: empty question
            um("3", "Q3", Some(2.0), Some(0.5)),    // invalid: price range
            um("4", "Q4", Some(0.4), Some(0.5)),    // valid
        ];
        let r = Validator::validate_many(&markets);
        assert_eq!(r.total, 4);
        assert_eq!(r.valid, 2);
        assert_eq!(r.invalid, 2);
        assert_eq!(r.errors.len(), 2);
        assert!((r.invalid_rate() - 0.5).abs() < 1e-9);
    }
}
