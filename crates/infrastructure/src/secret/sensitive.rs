//! 敏感字符串：自动在 Display 和 Debug 输出中脱敏。
//!
//! 从 `pm-auth::credential::SensitiveString` 和 `pm-trading::mask` 提取并统一。
//!
//! # 示例
//!
//! ```ignore
//! let key = SensitiveString::new("sk-abc123def456ghij");
//! println!("{}", key);  // 输出: sk-a***hij
//! println!("{:?}", key); // 输出: [API_KEY]
//! let plain = key.reveal(); // 显式获取明文
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// 敏感字符串，自动在 Display/Debug 中脱敏。
///
/// 仅在显式调用 [`reveal`](SensitiveString::reveal) 时返回明文。
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveString(String);

impl SensitiveString {
    /// 创建新的敏感字符串
    pub fn new<S: Into<String>>(s: S) -> Self {
        Self(s.into())
    }

    /// 显式获取明文（仅限有权限的代码使用）
    pub fn reveal(&self) -> &str {
        &self.0
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 字符串长度
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 脱敏输出：前4位 + *** + 后4位
    pub fn masked(&self) -> String {
        let s = &self.0;
        if s.len() <= 8 {
            return "***".to_string();
        }
        format!("{}***{}", &s[..4], &s[s.len() - 4..])
    }

    /// 地址脱敏：前6位 + ... + 后4位
    pub fn masked_address(&self) -> String {
        let s = &self.0;
        if s.len() <= 10 {
            return "***".to_string();
        }
        format!("{}...{}", &s[..6], &s[s.len() - 4..])
    }

    /// 完全隐藏：仅显示类型标记
    pub fn masked_full(&self) -> String {
        if self.0.is_empty() {
            "[EMPTY]".to_string()
        } else {
            "[PRIVATE_KEY]".to_string()
        }
    }
}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "SensitiveString(\"\")")
        } else {
            write!(f, "SensitiveString(\"[已脱敏]\")")
        }
    }
}

impl fmt::Display for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "")
        } else {
            write!(f, "{}", self.masked())
        }
    }
}

impl From<String> for SensitiveString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SensitiveString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_string_masks_display() {
        let key = SensitiveString::new("sk-abc123def456ghij");
        let displayed = format!("{}", key);
        // 不应包含完整明文
        assert!(!displayed.contains("abc123def456"));
        // 应包含脱敏标记
        assert!(displayed.contains("***"));
    }

    #[test]
    fn sensitive_string_masks_debug() {
        let key = SensitiveString::new("secret-value-here");
        let debugged = format!("{:?}", key);
        assert!(debugged.contains("已脱敏"));
        assert!(!debugged.contains("secret-value-here"));
    }

    #[test]
    fn reveal_returns_plaintext() {
        let key = SensitiveString::new("my-api-key");
        assert_eq!(key.reveal(), "my-api-key");
    }

    #[test]
    fn empty_string_shows_empty() {
        let key = SensitiveString::new("");
        assert!(key.is_empty());
        assert_eq!(key.len(), 0);
        let debugged = format!("{:?}", key);
        assert!(debugged.contains("\"\""));
    }

    #[test]
    fn masked_address_format() {
        let addr = SensitiveString::new("0x1234567890abcdef1234567890abcdef12345678");
        let masked = addr.masked_address();
        assert!(masked.starts_with("0x1234"));
        assert!(masked.ends_with("5678"));
        assert!(masked.contains("..."));
    }

    #[test]
    fn masked_full_hides_everything() {
        let pk = SensitiveString::new("0xdeadbeefcafebabe");
        assert_eq!(pk.masked_full(), "[PRIVATE_KEY]");
    }

    #[test]
    fn default_is_empty() {
        let default = SensitiveString::default();
        assert!(default.is_empty());
    }

    #[test]
    fn from_string_and_str() {
        let a = SensitiveString::from("hello".to_string());
        let b = SensitiveString::from("hello");
        assert_eq!(a.reveal(), b.reveal());
    }
}
