//! 密钥脱敏工具函数。
//!
//! 从 `pm-trading::mask` 提取并统一。
//! 所有函数接受明文并返回脱敏后的字符串。

/// 脱敏 API Key：前4位 + *** + 后4位
pub fn mask_api_key(s: &str) -> String {
    if s.len() <= 8 {
        return "***".to_string();
    }
    format!("{}***{}", &s[..4], &s[s.len() - 4..])
}

/// 脱敏地址：前6位 + ... + 后4位
pub fn mask_address(s: &str) -> String {
    if s.len() <= 10 {
        return "***".to_string();
    }
    format!("{}...{}", &s[..6], &s[s.len() - 4..])
}

/// 脱敏 Secret：完全隐藏
pub fn mask_secret(s: &str) -> String {
    if s.is_empty() {
        "[EMPTY]".to_string()
    } else {
        "[SECRET]".to_string()
    }
}

/// 脱敏 Passphrase：完全隐藏
pub fn mask_passphrase(s: &str) -> String {
    if s.is_empty() {
        "[EMPTY]".to_string()
    } else {
        "[PASSPHRASE]".to_string()
    }
}

/// 脱敏私钥：完全隐藏
pub fn mask_private_key(s: &str) -> String {
    if s.is_empty() {
        "[EMPTY]".to_string()
    } else {
        "[PRIVATE_KEY]".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_api_key_partial() {
        let result = mask_api_key("sk-abc123def456ghij");
        assert!(result.contains("***"));
        assert!(!result.contains("abc123def456"));
        assert!(result.starts_with("sk-a"));
    }

    #[test]
    fn mask_api_key_short() {
        assert_eq!(mask_api_key("short"), "***");
    }

    #[test]
    fn mask_address_format() {
        let result = mask_address("0x1234567890abcdef1234567890abcdef12345678");
        assert!(result.starts_with("0x1234"));
        assert!(result.ends_with("5678"));
    }

    #[test]
    fn mask_secret_hidden() {
        assert_eq!(mask_secret("my-secret"), "[SECRET]");
        assert_eq!(mask_secret(""), "[EMPTY]");
    }

    #[test]
    fn mask_passphrase_hidden() {
        assert_eq!(
            mask_passphrase("correct horse battery staple"),
            "[PASSPHRASE]"
        );
    }

    #[test]
    fn mask_private_key_hidden() {
        assert_eq!(mask_private_key("0xdeadbeef"), "[PRIVATE_KEY]");
    }
}
