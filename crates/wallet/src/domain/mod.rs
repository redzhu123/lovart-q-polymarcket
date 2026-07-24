//! Wallet 领域模型（P2-06 第四节）。
//!
//! 统一钱包领域对象：
//! - Wallet：钱包容器
//! - Account：账户
//! - Address：地址（脱敏）
//! - Network：网络（Ethereum/Polygon/Solana 等）
//! - Balance：余额
//! - Allowance：授权额度
//! - Nonce：交易序号
//! - Currency：货币
//! - Asset：资产类型
//!
//! 所有市场统一使用此 Domain。

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// Network — 网络
// ============================================================================

/// 区块链网络。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Network {
    /// Ethereum 主网。
    Ethereum,
    /// Polygon（Matic）。
    Polygon,
    /// Polygon Mumbai 测试网。
    PolygonMumbai,
    /// Arbitrum。
    Arbitrum,
    /// Solana。
    Solana,
    /// 自定义网络。
    Custom(u64),
}

impl Network {
    /// 链 ID。
    pub fn chain_id(&self) -> u64 {
        match self {
            Network::Ethereum => 1,
            Network::Polygon => 137,
            Network::PolygonMumbai => 80001,
            Network::Arbitrum => 42161,
            Network::Solana => 0, // Solana 不使用 EVM 链 ID
            Network::Custom(id) => *id,
        }
    }

    /// 网络名称（中文）。
    pub fn as_zh(&self) -> &'static str {
        match self {
            Network::Ethereum => "Ethereum 主网",
            Network::Polygon => "Polygon",
            Network::PolygonMumbai => "Polygon Mumbai 测试网",
            Network::Arbitrum => "Arbitrum",
            Network::Solana => "Solana",
            Network::Custom(_) => "自定义网络",
        }
    }

    /// 网络简称。
    pub fn short_name(&self) -> &'static str {
        match self {
            Network::Ethereum => "ETH",
            Network::Polygon => "MATIC",
            Network::PolygonMumbai => "MUMBAI",
            Network::Arbitrum => "ARB",
            Network::Solana => "SOL",
            Network::Custom(_) => "CUSTOM",
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (chain_id={})", self.as_zh(), self.chain_id())
    }
}

// ============================================================================
// Currency — 货币
// ============================================================================

/// 货币。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    /// USDC（USD Coin）。
    USDC,
    /// USDT（Tether）。
    USDT,
    /// ETH（Ether）。
    ETH,
    /// MATIC（Polygon）。
    MATIC,
    /// SOL（Solana）。
    SOL,
    /// 自定义货币。
    Custom(String),
}

impl Currency {
    /// 货币符号。
    pub fn symbol(&self) -> &str {
        match self {
            Currency::USDC => "USDC",
            Currency::USDT => "USDT",
            Currency::ETH => "ETH",
            Currency::MATIC => "MATIC",
            Currency::SOL => "SOL",
            Currency::Custom(s) => s.as_str(),
        }
    }

    /// 中文名称。
    pub fn as_zh(&self) -> &'static str {
        match self {
            Currency::USDC => "USD Coin",
            Currency::USDT => "Tether",
            Currency::ETH => "Ether",
            Currency::MATIC => "Polygon",
            Currency::SOL => "Solana",
            Currency::Custom(_) => "自定义货币",
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

// ============================================================================
// Address — 地址（脱敏）
// ============================================================================

/// 区块链地址（自动脱敏的 Display/Debug）。
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(String);

impl Address {
    /// 创建地址。
    pub fn new<S: Into<String>>(s: S) -> Self {
        Self(s.into())
    }

    /// 获取原始地址字符串。
    pub fn reveal(&self) -> &str {
        &self.0
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 脱敏显示：保留前 6 和后 4 字符。
    pub fn masked(&self) -> String {
        pm_trading::mask::mask_address(&self.0)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.masked())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.masked())
    }
}

impl Default for Address {
    fn default() -> Self {
        Self(String::new())
    }
}

impl From<String> for Address {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Address {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ============================================================================
// Nonce — 交易序号
// ============================================================================

/// 交易序号（Nonce）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nonce(u64);

impl Nonce {
    pub fn new(n: u64) -> Self {
        Self(n)
    }

    pub fn zero() -> Self {
        Self(0)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub fn increment(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

impl fmt::Display for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Nonce {
    fn from(n: u64) -> Self {
        Self(n)
    }
}

// ============================================================================
// Asset — 资产
// ============================================================================

/// 资产类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Asset {
    /// 原生代币（ETH/MATIC/SOL）。
    Native,
    /// ERC-20 代币。
    Erc20,
    /// ERC-721 NFT。
    Erc721,
    /// 预测市场代币。
    PredictionToken,
    /// 自定义资产。
    Custom,
}

impl Asset {
    pub fn as_zh(&self) -> &'static str {
        match self {
            Asset::Native => "原生代币",
            Asset::Erc20 => "ERC-20",
            Asset::Erc721 => "ERC-721",
            Asset::PredictionToken => "预测市场代币",
            Asset::Custom => "自定义",
        }
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_zh())
    }
}

// ============================================================================
// Account — 账户
// ============================================================================

/// 钱包账户。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// 账户 ID。
    pub account_id: String,
    /// 账户名称。
    pub name: String,
    /// 钱包地址（脱敏）。
    pub address: Address,
    /// 所属钱包 ID。
    pub wallet_id: String,
    /// 链 ID。
    pub chain_id: u64,
    /// 网络。
    pub network: Network,
    /// 主货币。
    pub currency: Currency,
    /// 是否活跃。
    pub active: bool,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 标签。
    pub tags: std::collections::HashMap<String, String>,
}

impl Account {
    /// 创建账户。
    pub fn new(
        account_id: String,
        name: String,
        address: Address,
        wallet_id: String,
        network: Network,
        currency: Currency,
    ) -> Self {
        Self {
            account_id,
            name,
            address,
            wallet_id,
            chain_id: network.chain_id(),
            network,
            currency,
            active: true,
            created_at: Local::now(),
            tags: std::collections::HashMap::new(),
        }
    }

    /// 创建默认主账户。
    pub fn default_main(now: DateTime<Local>) -> Self {
        Self {
            account_id: "ACC-MAIN-001".to_string(),
            name: "主账户".to_string(),
            address: Address::new("0x0000000000000000000000000000000000000000"),
            wallet_id: "WALLET-MAIN-001".to_string(),
            chain_id: 137,
            network: Network::Polygon,
            currency: Currency::USDC,
            active: true,
            created_at: now,
            tags: std::collections::HashMap::new(),
        }
    }

    /// 安全摘要（中文，脱敏）。
    pub fn safe_summary(&self) -> String {
        format!(
            "{} | {} | {} | {} | {}",
            self.account_id,
            self.name,
            self.address.masked(),
            self.network.short_name(),
            self.currency.symbol(),
        )
    }
}

// ============================================================================
// Wallet — 钱包
// ============================================================================

/// 钱包。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    /// 钱包 ID。
    pub wallet_id: String,
    /// 钱包名称。
    pub name: String,
    /// 账户列表。
    pub accounts: Vec<Account>,
    /// 主网络。
    pub network: Network,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 标签。
    pub tags: std::collections::HashMap<String, String>,
}

impl Wallet {
    /// 创建钱包。
    pub fn new(wallet_id: String, name: String, network: Network) -> Self {
        Self {
            wallet_id,
            name,
            accounts: Vec::new(),
            network,
            created_at: Local::now(),
            tags: std::collections::HashMap::new(),
        }
    }

    /// 添加账户。
    pub fn add_account(&mut self, account: Account) {
        self.accounts.push(account);
    }

    /// 活跃账户数。
    pub fn active_account_count(&self) -> usize {
        self.accounts.iter().filter(|a| a.active).count()
    }

    /// 安全摘要（中文，脱敏）。
    pub fn safe_summary(&self) -> String {
        format!(
            "钱包: {} ({}) | 网络: {} | 账户: {} 个（活跃 {}）",
            self.wallet_id,
            self.name,
            self.network.as_zh(),
            self.accounts.len(),
            self.active_account_count(),
        )
    }
}

// ============================================================================
// Balance — 余额
// ============================================================================

/// 账户余额。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    /// 账户 ID。
    pub account_id: String,
    /// 可用余额。
    pub available: f64,
    /// 锁定余额（冻结中）。
    pub locked: f64,
    /// 总余额。
    pub total: f64,
    /// 货币。
    pub currency: Currency,
    /// 未实现盈亏。
    pub unrealized_pnl: f64,
    /// 已实现盈亏。
    pub realized_pnl: f64,
    /// 更新时间。
    pub updated_at: Option<DateTime<Local>>,
}

impl Balance {
    /// 创建余额。
    pub fn new(account_id: &str, currency: Currency, amount: f64) -> Self {
        Self {
            account_id: account_id.to_string(),
            available: amount,
            locked: 0.0,
            total: amount,
            currency,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            updated_at: Some(Local::now()),
        }
    }

    /// 创建零余额。
    pub fn zero(account_id: &str, currency: Currency) -> Self {
        Self::new(account_id, currency, 0.0)
    }

    /// 创建模拟余额（用于测试）。
    pub fn mock(amount: f64) -> Self {
        Self::new("ACC-MOCK", Currency::USDC, amount)
    }

    /// 冻结资金。
    pub fn freeze(&mut self, amount: f64) -> bool {
        if amount <= 0.0 || amount > self.available {
            return false;
        }
        self.available -= amount;
        self.locked += amount;
        self.updated_at = Some(Local::now());
        true
    }

    /// 解冻资金。
    pub fn unfreeze(&mut self, amount: f64) -> bool {
        if amount <= 0.0 || amount > self.locked {
            return false;
        }
        self.locked -= amount;
        self.available += amount;
        self.updated_at = Some(Local::now());
        true
    }

    /// 扣款（从锁定中）。
    pub fn debit(&mut self, amount: f64) -> bool {
        if amount <= 0.0 || amount > self.locked {
            return false;
        }
        self.locked -= amount;
        self.total -= amount;
        self.updated_at = Some(Local::now());
        true
    }

    /// 入金。
    pub fn credit(&mut self, amount: f64) {
        self.available += amount;
        self.total += amount;
        self.updated_at = Some(Local::now());
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "{} | 可用: {:.2} {} | 锁定: {:.2} | 总计: {:.2} | 未实现: {:.2} | 已实现: {:.2}",
            self.account_id,
            self.available,
            self.currency.symbol(),
            self.locked,
            self.total,
            self.unrealized_pnl,
            self.realized_pnl,
        )
    }
}

// ============================================================================
// Allowance — 授权额度
// ============================================================================

/// 授权额度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Allowance {
    /// 授权 ID。
    pub allowance_id: String,
    /// 账户 ID。
    pub account_id: String,
    /// 被授权方地址。
    pub spender: Address,
    /// 授权总额度。
    pub amount: f64,
    /// 已使用额度。
    pub used: f64,
    /// Nonce。
    pub nonce: Nonce,
    /// 过期时间。
    pub expires_at: Option<DateTime<Local>>,
    /// 创建时间。
    pub created_at: DateTime<Local>,
    /// 是否已撤销。
    pub revoked: bool,
}

impl Allowance {
    /// 创建授权。
    pub fn new(allowance_id: String, account_id: String, spender: Address, amount: f64) -> Self {
        Self {
            allowance_id,
            account_id,
            spender,
            amount,
            used: 0.0,
            nonce: Nonce::zero(),
            expires_at: None,
            created_at: Local::now(),
            revoked: false,
        }
    }

    /// 剩余额度。
    pub fn remaining(&self) -> f64 {
        if self.revoked {
            return 0.0;
        }
        (self.amount - self.used).max(0.0)
    }

    /// 是否可用。
    pub fn is_usable(&self) -> bool {
        if self.revoked {
            return false;
        }
        if let Some(expiry) = self.expires_at {
            if Local::now() >= expiry {
                return false;
            }
        }
        self.remaining() > 0.0
    }

    /// 消耗额度。
    pub fn consume(&mut self, amount: f64) -> bool {
        if amount <= 0.0 || amount > self.remaining() {
            return false;
        }
        self.used += amount;
        self.nonce.increment();
        true
    }

    /// 撤销授权。
    pub fn revoke(&mut self) {
        self.revoked = true;
    }

    /// 中文摘要。
    pub fn summary_zh(&self) -> String {
        format!(
            "{} | 授权方: {} | 总额度: {:.2} | 已用: {:.2} | 剩余: {:.2} | 状态: {}",
            self.allowance_id,
            self.spender.masked(),
            self.amount,
            self.used,
            self.remaining(),
            if self.revoked {
                "已撤销"
            } else if self.is_usable() {
                "可用"
            } else {
                "不可用"
            },
        )
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_chain_ids() {
        assert_eq!(Network::Ethereum.chain_id(), 1);
        assert_eq!(Network::Polygon.chain_id(), 137);
        assert_eq!(Network::Custom(56).chain_id(), 56);
    }

    #[test]
    fn network_display() {
        let display = format!("{}", Network::Polygon);
        assert!(display.contains("Polygon"));
        assert!(display.contains("137"));
    }

    #[test]
    fn currency_symbols() {
        assert_eq!(Currency::USDC.symbol(), "USDC");
        assert_eq!(Currency::USDT.symbol(), "USDT");
    }

    #[test]
    fn address_masking() {
        let addr = Address::new("0x1234567890abcdef1234567890abcdef12345678");
        let display = format!("{}", addr);
        assert!(!display.contains("1234567890abcdef"));
        assert!(display.starts_with("0x1234"));
        assert!(display.ends_with("5678"));
    }

    #[test]
    fn address_debug_masks() {
        let addr = Address::new("0xabcdef1234567890abcdef1234567890abcdef12");
        let debug = format!("{:?}", addr);
        assert!(!debug.contains("abcdef1234567890"));
        assert!(debug.starts_with("Address("));
    }

    #[test]
    fn nonce_increment() {
        let mut nonce = Nonce::zero();
        assert_eq!(nonce.value(), 0);
        assert_eq!(nonce.increment(), 1);
        assert_eq!(nonce.increment(), 2);
    }

    #[test]
    fn wallet_creation() {
        let wallet = Wallet::new("W1".into(), "主钱包".into(), Network::Polygon);
        assert_eq!(wallet.wallet_id, "W1");
        assert_eq!(wallet.accounts.len(), 0);
        assert!(wallet.safe_summary().contains("主钱包"));
    }

    #[test]
    fn wallet_add_account() {
        let mut wallet = Wallet::new("W1".into(), "测试".into(), Network::Ethereum);
        let account = Account::default_main(Local::now());
        wallet.add_account(account);
        assert_eq!(wallet.accounts.len(), 1);
        assert_eq!(wallet.active_account_count(), 1);
    }

    #[test]
    fn account_safe_summary_masks() {
        let account = Account::default_main(Local::now());
        let summary = account.safe_summary();
        assert!(!summary.contains("00000000000000000000"));
    }

    #[test]
    fn balance_freeze_unfreeze() {
        let mut b = Balance::new("ACC-1", Currency::USDC, 1000.0);
        assert!(b.freeze(300.0));
        assert_eq!(b.available, 700.0);
        assert_eq!(b.locked, 300.0);
        assert_eq!(b.total, 1000.0);

        assert!(b.unfreeze(100.0));
        assert_eq!(b.available, 800.0);
        assert_eq!(b.locked, 200.0);
    }

    #[test]
    fn balance_freeze_insufficient() {
        let mut b = Balance::new("ACC-1", Currency::USDC, 100.0);
        assert!(!b.freeze(200.0));
        assert_eq!(b.available, 100.0);
    }

    #[test]
    fn balance_debit_credit() {
        let mut b = Balance::new("ACC-1", Currency::USDC, 1000.0);
        b.freeze(500.0);
        assert!(b.debit(500.0));
        assert_eq!(b.total, 500.0);
        assert_eq!(b.locked, 0.0);

        b.credit(200.0);
        assert_eq!(b.total, 700.0);
        assert_eq!(b.available, 700.0);
    }

    #[test]
    fn allowance_usage() {
        let mut a = Allowance::new(
            "ALW-1".into(),
            "ACC-1".into(),
            Address::new("0xspender"),
            1000.0,
        );
        assert_eq!(a.remaining(), 1000.0);
        assert!(a.is_usable());

        assert!(a.consume(300.0));
        assert_eq!(a.remaining(), 700.0);
        assert_eq!(a.used, 300.0);

        assert!(!a.consume(800.0)); // insufficient
    }

    #[test]
    fn allowance_revoke() {
        let mut a = Allowance::new(
            "ALW-1".into(),
            "ACC-1".into(),
            Address::new("0xspender"),
            1000.0,
        );
        a.revoke();
        assert!(a.revoked);
        assert!(!a.is_usable());
        assert_eq!(a.remaining(), 0.0);
    }

    #[test]
    fn asset_zh() {
        assert_eq!(Asset::Native.as_zh(), "原生代币");
        assert_eq!(Asset::PredictionToken.as_zh(), "预测市场代币");
    }
}
