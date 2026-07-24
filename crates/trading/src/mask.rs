//! 敏感信息脱敏工具（V1.07 第十二节）。
//!
//! 禁止打印 API Key、Secret、Private Key。
//! 所有日志输出敏感信息前必须经过本模块脱敏。

/// 脱敏地址（如 0x1234...89AB）。
/// 保留前 6 和后 4 个字符，中间用 `...` 替换。
pub fn mask_address(s: &str) -> String {
    if s.len() <= 10 {
        return "***".to_string();
    }
    let prefix = &s[..6];
    let suffix = &s[s.len() - 4..];
    format!("{}...{}", prefix, suffix)
}

/// 脱敏 API Key（如 abcd...efgh）。
/// 保留前 4 和后 4 个字符，中间用 `...` 替换。
pub fn mask_api_key(s: &str) -> String {
    if s.len() <= 8 {
        return "***".to_string();
    }
    let prefix = &s[..4];
    let suffix = &s[s.len() - 4..];
    format!("{}...{}", prefix, suffix)
}

/// 脱敏 Secret（完全不显示内容，仅显示 `[SECRET]`）。
pub fn mask_secret(_s: &str) -> String {
    "[SECRET]".to_string()
}

/// 脱敏 Passphrase（完全不显示内容，仅显示 `[PASSPHRASE]`）。
pub fn mask_passphrase(_s: &str) -> String {
    "[PASSPHRASE]".to_string()
}

/// 脱敏 Private Key（完全不显示内容，仅显示 `[PRIVATE_KEY]`）。
pub fn mask_private_key(_s: &str) -> String {
    "[PRIVATE_KEY]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_address_works() {
        assert_eq!(
            mask_address("0x1234567890abcdef1234567890abcdef12345678"),
            "0x1234...5678"
        );
    }

    #[test]
    fn mask_address_short() {
        assert_eq!(mask_address("0x1234"), "***");
    }

    #[test]
    fn mask_api_key_works() {
        assert_eq!(mask_api_key("abcdefghijklmnop"), "abcd...mnop");
    }

    #[test]
    fn mask_api_key_short() {
        assert_eq!(mask_api_key("abc"), "***");
    }

    #[test]
    fn mask_secret_hides_all() {
        assert_eq!(mask_secret("my-super-secret-key-12345"), "[SECRET]");
    }

    #[test]
    fn mask_passphrase_hides_all() {
        assert_eq!(
            mask_passphrase("correct horse battery staple"),
            "[PASSPHRASE]"
        );
    }

    #[test]
    fn mask_private_key_hides_all() {
        assert_eq!(mask_private_key("0xdeadbeef"), "[PRIVATE_KEY]");
    }
}
