//! 统一追踪标识符（TraceId）：跨生命周期各阶段共享。
//!
//! 格式：`TRC-{round}-{seq}-{short_hash}`
//! 示例：`TRC-00042-003-a3f8`
//!
//! 从 Market → Candidate → Opportunity → Shadow → Paper → Execution → Settlement
//! 全链路共享同一个 TraceId，方便调试与审计。

use std::fmt;
use std::hash::{Hash, Hasher};

/// 统一追踪标识符。
///
/// 包装 `String`，提供类型安全与格式化方法。
/// 使用 `PartialEq/Eq/Hash` 按内部字符串比较，可用作 HashMap key。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceId(pub String);

impl TraceId {
    /// 生成新的 TraceId。
    ///
    /// - `round`：当前扫描轮次
    /// - `seq`：本轮内序号（从 0 开始）
    /// - `market_id`：源市场 ID（取前 6 字符做简写）
    pub fn generate(round: u64, seq: u64, market_id: &str) -> Self {
        // 取 market_id 的前 6 个字符（ASCII 字母数字保留，否则用十六进制哈希简写）
        let short: String = market_id
            .chars()
            .take(6)
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        let short = if short.len() >= 3 {
            short
        } else {
            // 太短则用简单哈希的前 6 个十六进制字符
            let mut h: u64 = 0;
            for b in market_id.bytes() {
                h = h.wrapping_mul(31).wrapping_add(b as u64);
            }
            format!("{:06x}", h % 0x1000000)
        };
        TraceId(format!("TRC-{:05}-{:03}-{}", round, seq, short))
    }

    /// 生成不含轮次的简化 TraceId（用于 CSV 回放等独立场景）。
    pub fn generate_simple(market_id: &str, ts_ms: i64) -> Self {
        let short: String = market_id
            .chars()
            .take(6)
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        TraceId(format!("TRC-{}-{}", ts_ms, short))
    }

    /// 返回内部字符串切片（用于 CSV 字段）。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否为空（未追踪）。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for TraceId {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for TraceId {}

impl Hash for TraceId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Default for TraceId {
    fn default() -> Self {
        TraceId(String::new())
    }
}

/// 生成 UUID-v4 风格唯一 ID 的简易实现（不依赖 uuid crate）。
/// 用于 `TraceId::generate_simple` 的 timestamp 之外的随机后缀。
#[allow(dead_code)]
fn random_hex_suffix(len: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:0width$x}", nanos % (16u128.pow(len as u32)), width = len)
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_generates_unique() {
        let mut ids = std::collections::HashSet::new();
        for i in 0..1000u64 {
            let tid = TraceId::generate(1, i, "0xabc123def");
            assert!(ids.insert(tid.0.clone()), "duplicate at i={}", i);
        }
    }

    #[test]
    fn trace_id_format_contains_round_seq() {
        let tid = TraceId::generate(42, 7, "market_xyz");
        let s = tid.to_string();
        assert!(s.starts_with("TRC-"), "unexpected format: {}", s);
        assert!(s.contains("00042"), "missing round: {}", s);
        assert!(s.contains("007"), "missing seq: {}", s);
    }

    #[test]
    fn trace_id_short_market_id_fallback() {
        // 很短的 market_id 应使用哈希回退
        let tid = TraceId::generate(0, 0, "ab");
        let s = tid.to_string();
        assert!(s.len() > 10, "too short: {}", s);
    }

    #[test]
    fn trace_id_market_id_prefix_preserved() {
        let tid = TraceId::generate(0, 0, "HELLO_WORLD");
        assert!(tid.to_string().contains("HELLO"), "missing prefix: {}", tid);
    }

    #[test]
    fn trace_id_simple_generates() {
        let tid = TraceId::generate_simple("market_abc", 1700000000000);
        assert!(tid.to_string().starts_with("TRC-"));
        assert!(tid.to_string().contains("market"));
    }

    #[test]
    fn trace_id_eq_and_hash() {
        let a = TraceId("test".into());
        let b = TraceId("test".into());
        let c = TraceId("other".into());
        assert_eq!(a, b);
        assert_ne!(a, c);

        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(a.clone());
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }
}
