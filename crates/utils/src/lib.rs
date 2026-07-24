//! pm-utils：纯工具函数。
//!
//! 仅放无副作用、无 IO、无业务依赖的辅助函数：
//! - 数值格式化（NaN 安全）：[`fmt_money`] / [`fmt_sum`] / [`fmt_qty`] / [`fmt_pnl`] /
//!   [`fmt_roi`] / [`fmt_pct`] / [`fmt_scans`]。
//! - 统计数学（空集合安全）：[`mean`] / [`median`] / [`ratio`]。
//!
//! 这些函数被仪表盘展示（pm-scanner::display）与报告（pm-backtest / pm-metrics）共用。

/// 浮点比较容差。
const EPS: f64 = 1e-9;

/// 把金额格式化为两位小数（NaN 或绝对值极小兜底为 `0.00`，避免 `-0.00` 浮点漂移）。
pub fn fmt_money(v: f64) -> String {
    if v.is_nan() || v.abs() < 0.005 {
        "0.00".to_string()
    } else {
        format!("{:.2}", v)
    }
}

/// 把价格（SUM）格式化为两位小数（NaN 兜底为 `0.00`）。
pub fn fmt_sum(v: f64) -> String {
    if v.is_nan() {
        "0.00".to_string()
    } else {
        format!("{:.2}", v)
    }
}

/// 把份额格式化为两位小数（NaN 兜底为 `0.00`）。
pub fn fmt_qty(v: f64) -> String {
    if v.is_nan() {
        "0.00".to_string()
    } else {
        format!("{:.2}", v)
    }
}

/// 把盈亏格式化为带正负号的两位小数（NaN 兜底为 `0.00`）。
pub fn fmt_pnl(v: f64) -> String {
    if v.is_nan() {
        return "0.00".to_string();
    }
    if v >= 0.0 {
        format!("+{:.2}", v)
    } else {
        format!("{:.2}", v)
    }
}

/// 把 ROI（小数形式，如 0.0312）格式化为百分比字符串（如 `3.12%`）。NaN 兜底为 `0.00%`。
pub fn fmt_roi(v: f64) -> String {
    if v.is_nan() {
        return "0.00%".to_string();
    }
    format!("{:.2}%", v * 100.0)
}

/// 把比率（小数，如 0.0031）格式化为百分比字符串（如 `0.31%`）。NaN 兜底为 `0.00%`。
pub fn fmt_pct(v: f64) -> String {
    if v.is_nan() {
        return "0.00%".to_string();
    }
    format!("{:.2}%", v * 100.0)
}

/// 把扫描周期数格式化为一位小数（NaN 兜底为 `0.0`）。
pub fn fmt_scans(v: f64) -> String {
    if v.is_nan() {
        "0.0".to_string()
    } else {
        format!("{:.1}", v)
    }
}

/// 均值（空切片或含非有限值时为 0.0）。
pub fn mean(v: &[f64]) -> f64 {
    let f: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if f.is_empty() {
        0.0
    } else {
        f.iter().sum::<f64>() / f.len() as f64
    }
}

/// 中位数（空切片为 0.0）。NaN 视作相等，避免 sort 异常。
pub fn median(v: &[f64]) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return 0.0;
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

/// 比率（分母为 0 时返回 0.0）。
pub fn ratio(n: u64, d: u64) -> f64 {
    if d > 0 { n as f64 / d as f64 } else { 0.0 }
}

/// 浮点近似比较（用于测试与不变式断言）。
pub fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_nan_safe() {
        assert_eq!(fmt_money(f64::NAN), "0.00");
        assert_eq!(fmt_money(-0.0001), "0.00"); // 避免 -0.00
        assert_eq!(fmt_money(1234.5), "1234.50");
        assert_eq!(fmt_pnl(1.5), "+1.50");
        assert_eq!(fmt_pnl(-1.5), "-1.50");
        assert_eq!(fmt_pnl(f64::NAN), "0.00");
        assert_eq!(fmt_roi(0.0312), "3.12%");
        assert_eq!(fmt_pct(0.0031), "0.31%");
        assert_eq!(fmt_scans(f64::NAN), "0.0");
        assert_eq!(fmt_sum(f64::NAN), "0.00");
    }

    #[test]
    fn mean_median_empty() {
        assert_eq!(mean(&[]), 0.0);
        assert_eq!(median(&[]), 0.0);
    }

    #[test]
    fn mean_median_basic() {
        assert!(approx(mean(&[1.0, 2.0, 3.0]), 2.0));
        assert!(approx(median(&[1.0, 3.0, 2.0]), 2.0));
        assert!(approx(median(&[1.0, 2.0, 3.0, 4.0]), 2.5));
    }

    #[test]
    fn mean_filters_non_finite() {
        assert!(approx(mean(&[1.0, f64::NAN, 3.0]), 2.0));
    }

    #[test]
    fn ratio_zero_denom() {
        assert_eq!(ratio(5, 0), 0.0);
        assert!(approx(ratio(3, 4), 0.75));
    }
}
