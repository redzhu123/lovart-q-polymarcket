//! Prometheus 文本格式输出。
//!
//! 将 Counter、Gauge、Histogram 渲染为 Prometheus exposition format。

use super::{Counter, Gauge, Histogram};

/// 将一组指标渲染为 Prometheus 文本格式
pub fn to_prometheus_text(
    counters: &[&Counter],
    gauges: &[&Gauge],
    histograms: &[&Histogram],
) -> String {
    let mut lines = Vec::new();

    for c in counters {
        lines.push(format!("# HELP {} {}", c.name, c.help));
        lines.push(format!("# TYPE {} counter", c.name));
        lines.push(format!("{} {}", c.name, c.get()));
    }

    for g in gauges {
        lines.push(format!("# HELP {} {}", g.name, g.help));
        lines.push(format!("# TYPE {} gauge", g.name));
        lines.push(format!("{} {}", g.name, g.get()));
    }

    for h in histograms {
        lines.push(format!("# HELP {} {}", h.name, h.help));
        lines.push(format!("# TYPE {} histogram", h.name));
        let count = h.count();
        let sum = h.sum();
        let mut cumulative: u64 = 0;
        for (bound, bc) in h.bucket_counts() {
            cumulative += bc;
            lines.push(format!(
                "{}_bucket{{le=\"{}\"}} {}",
                h.name, bound, cumulative
            ));
        }
        lines.push(format!("{}_bucket{{le=\"+Inf\"}} {}", h.name, count));
        lines.push(format!("{}_sum {}", h.name, sum));
        lines.push(format!("{}_count {}", h.name, count));
    }

    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{Counter, Gauge, Histogram};

    #[test]
    fn prometheus_text_output_contains_all_types() {
        let counter = Counter::new("test_requests_total", "Total requests");
        counter.inc_by(10);

        let gauge = Gauge::new("test_active_connections", "Active connections");
        gauge.set(5);

        let histogram = Histogram::new("test_latency_ms", "Request latency");
        histogram.observe(50.0);
        histogram.observe(150.0);

        let text = to_prometheus_text(&[&counter], &[&gauge], &[&histogram]);

        assert!(text.contains("test_requests_total 10"));
        assert!(text.contains("test_active_connections 5"));
        assert!(text.contains("test_latency_ms_bucket"));
        assert!(text.contains("test_latency_ms_sum"));
        assert!(text.contains("test_latency_ms_count"));
        assert!(text.contains("# HELP"));
        assert!(text.contains("# TYPE"));
    }

    #[test]
    fn prometheus_text_with_zero_values() {
        let counter = Counter::new("test_zero", "Zero counter");
        let text = to_prometheus_text(&[&counter], &[], &[]);
        assert!(text.contains("test_zero 0"));
    }
}
