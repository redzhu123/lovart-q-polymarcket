//! pm-wallet：Wallet Infrastructure（P2-06）。
//!
//! 企业级钱包基础设施，作为认证层与交易所之间的钱包层。
//! 提供统一的账户/余额/授权/签名/仓库管理。
//!
//! # 架构
//!
//! ```text
//! Authentication
//!       │
//!       ▼
//! ┌──────────────────────────┐
//! │  Wallet Domain           │
//! │  ├─ AccountManager       │  ← 多账户管理
//! │  ├─ BalanceManager       │  ← 余额追踪
//! │  ├─ AllowanceManager     │  ← 授权管理
//! │  └─ WalletSigner         │  ← 交易签名
//! └─────────┬────────────────┘
//!           │
//! ┌─────────▼────────────────┐
//! │  WalletRepository        │  ← Memory / CSV / SQLite(预留)
//! └──────────────────────────┘
//!           │
//!           ▼
//! Exchange
//! ```
//!
//! # 模块
//!
//! - [`domain`]：统一领域模型（Wallet/Account/Address/Network/Balance/Allowance/Nonce/Currency/Asset）
//! - [`account`]：AccountManager — 多账户管理
//! - [`balance`]：BalanceManager — 余额追踪
//! - [`allowance`]：AllowanceManager — 授权管理
//! - [`signer`]：WalletSigner trait + EVM/Ed25519 实现
//! - [`repository`]：WalletRepository trait + Memory/CSV 实现
//! - [`diagnostics`]：健康诊断命令
//!
//! # 业务约束
//!
//! - 禁止真实交易 / 真实私钥签名 / 自动签名发送订单。
//! - 所有日志使用 tracing，中文输出。
//! - 所有敏感信息自动脱敏。
//!
//! Simulation Only -- 不连接真实钱包 / 不签名 / 不暴露私钥。

pub mod account;
pub mod allowance;
pub mod balance;
pub mod diagnostics;
pub mod domain;
pub mod repository;
pub mod signer;

// ---- 核心重导出 ----
pub use account::{AccountManager, AccountManagerHealth};
pub use allowance::AllowanceManager;
pub use balance::BalanceManager;
pub use diagnostics::{
    diagnose_wallet_accounts, diagnose_wallet_allowance, diagnose_wallet_balance,
    diagnose_wallet_health,
};
pub use domain::{Account, Address, Allowance, Asset, Balance, Currency, Network, Nonce, Wallet};
pub use repository::{
    RepositoryHealth, RepositoryType, WalletRepository, create_repository,
    csv::CsvWalletRepository, memory::InMemoryWalletRepository,
};
pub use signer::{
    NoopWalletSigner, WalletSignRequest, WalletSignResponse, WalletSigner, WalletSignerHealth,
    ed25519::Ed25519Signer, evm::EvmSigner,
};

// ---- 常用导出 ----
pub mod prelude {
    pub use crate::account::{AccountManager, AccountManagerHealth};
    pub use crate::allowance::AllowanceManager;
    pub use crate::balance::BalanceManager;
    pub use crate::create_csv_wallet;
    pub use crate::create_default_wallet;
    pub use crate::diagnostics::{
        diagnose_wallet_accounts, diagnose_wallet_allowance, diagnose_wallet_balance,
        diagnose_wallet_health,
    };
    pub use crate::domain::{
        Account, Address, Allowance, Asset, Balance, Currency, Network, Nonce, Wallet,
    };
    pub use crate::repository::{
        RepositoryHealth, RepositoryType, WalletRepository, create_repository,
        csv::CsvWalletRepository, memory::InMemoryWalletRepository,
    };
    pub use crate::signer::{
        NoopWalletSigner, WalletSignRequest, WalletSignResponse, WalletSigner, WalletSignerHealth,
        ed25519::Ed25519Signer, evm::EvmSigner,
    };
}

// ============================================================================
// 工厂函数
// ============================================================================

/// 创建默认钱包（Memory 仓库）。
pub fn create_default_wallet()
-> anyhow::Result<(Wallet, AccountManager, BalanceManager, AllowanceManager)> {
    let _now = chrono::Local::now();
    let wallet = Wallet::new("WALLET-MAIN-001".into(), "主钱包".into(), Network::Polygon);

    let mut account_mgr = AccountManager::empty();
    account_mgr.create_account(
        "主账户",
        Address::new("0x0000000000000000000000000000000000000000"),
        &wallet.wallet_id,
        Network::Polygon,
        Currency::USDC,
    )?;

    let mut balance_mgr = BalanceManager::new();
    balance_mgr.set_balance(Balance::new("ACC-0001", Currency::USDC, 10_000.0));

    let allowance_mgr = AllowanceManager::new();

    Ok((wallet, account_mgr, balance_mgr, allowance_mgr))
}

/// 创建带 CSV 持久化的钱包。
pub fn create_csv_wallet(
    base_path: std::path::PathBuf,
) -> anyhow::Result<(
    Wallet,
    AccountManager,
    BalanceManager,
    AllowanceManager,
    Box<dyn WalletRepository>,
)> {
    let (wallet, account_mgr, balance_mgr, allowance_mgr) = create_default_wallet()?;
    let repo = create_repository(RepositoryType::Csv, Some(base_path))?;
    Ok((wallet, account_mgr, balance_mgr, allowance_mgr, repo))
}

// ============================================================================
// 中文 tracing 初始化
// ============================================================================

/// 初始化 Wallet 中文 tracing。
pub fn init_wallet_logging(level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("PM_WALLET_LOG").unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .try_init();
}

// ============================================================================
// 集成测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn prelude_exports_compile() {
        let _wallet = Wallet::new("W1".into(), "测试".into(), Network::Polygon);
        let _account = Account::default_main(Local::now());
        let _address = Address::new("0x1234");
        let _balance = Balance::mock(1000.0);
        let _allowance = Allowance::new("ALW-1".into(), "ACC-1".into(), Address::new("0xs"), 500.0);
        let _nonce = Nonce::zero();
        let _currency = Currency::USDC;
        let _network = Network::Polygon;
        let _asset = Asset::Native;
        let _ = NoopWalletSigner::new();
        let _ = EvmSigner::new(137);
        let _ = Ed25519Signer::new();
        let _ = AccountManager::empty();
        let _ = BalanceManager::new();
        let _ = AllowanceManager::new();
        let _ = InMemoryWalletRepository::new();
    }

    #[test]
    fn default_factory_works() {
        let (wallet, account_mgr, balance_mgr, _allowance_mgr) = create_default_wallet().unwrap();
        assert_eq!(wallet.wallet_id, "WALLET-MAIN-001");
        assert_eq!(account_mgr.count(), 1);
        assert_eq!(balance_mgr.count(), 1);
    }

    #[test]
    fn full_wallet_lifecycle() {
        let (wallet, mut account_mgr, mut balance_mgr, mut allowance_mgr) =
            create_default_wallet().unwrap();

        // 1. 创建账户
        let _account = account_mgr
            .create_account(
                "副账户",
                Address::new("0xBBBB"),
                &wallet.wallet_id,
                Network::Ethereum,
                Currency::ETH,
            )
            .unwrap();
        assert_eq!(account_mgr.count(), 2);

        // 2. 设置余额
        balance_mgr.set_balance(Balance::new("ACC-0002", Currency::ETH, 100.0));
        assert_eq!(balance_mgr.count(), 2);

        // 3. 冻结 + 扣款
        assert!(
            balance_mgr
                .freeze("ACC-0001", Currency::USDC, 500.0)
                .unwrap()
        );
        assert!(
            balance_mgr
                .debit("ACC-0001", Currency::USDC, 500.0)
                .unwrap()
        );
        let b = balance_mgr.get("ACC-0001", Currency::USDC).unwrap();
        assert!(approx(b.total, 9500.0));

        // 4. 授权
        let alw = allowance_mgr.approve("ACC-0001", Address::new("0xProtocol"), 2000.0);
        assert!(alw.is_usable());
        assert_eq!(allowance_mgr.count(), 1);

        // 5. 消耗授权
        assert!(allowance_mgr.consume("ALW-0001", 500.0).unwrap());
        let a = allowance_mgr.find_by_id("ALW-0001").unwrap();
        assert!(approx(a.remaining(), 1500.0));

        // 6. 撤销授权
        allowance_mgr.revoke("ALW-0001").unwrap();
        let a = allowance_mgr.find_by_id("ALW-0001").unwrap();
        assert!(!a.is_usable());
    }

    #[test]
    fn memory_repository_integration() {
        let mut repo = InMemoryWalletRepository::new();
        let wallet = Wallet::new("W1".into(), "测试".into(), Network::Polygon);
        repo.save_wallet(&wallet).unwrap();

        let loaded = repo.get_wallet("W1").unwrap().unwrap();
        assert_eq!(loaded.name, "测试");

        let account = Account::default_main(Local::now());
        repo.save_account(&account).unwrap();
        assert_eq!(repo.list_accounts().unwrap().len(), 1);

        let balance = Balance::new("ACC-1", Currency::USDC, 500.0);
        repo.save_balance(&balance).unwrap();
        assert_eq!(repo.list_balances("ACC-1").unwrap().len(), 1);
    }

    #[test]
    fn csv_repository_integration() {
        let dir = std::env::temp_dir().join(format!("wallet-int-{}", rand::random::<u64>()));
        let mut repo = CsvWalletRepository::new(dir).unwrap();

        let wallet = Wallet::new("W-CSV".into(), "CSV测试".into(), Network::Polygon);
        repo.save_wallet(&wallet).unwrap();
        assert!(repo.get_wallet("W-CSV").unwrap().is_some());

        let h = repo.health();
        assert!(h.ok);
    }

    #[test]
    fn signer_integration() {
        let evm_signer = EvmSigner::new(137);
        assert_eq!(evm_signer.chain_id(), 137);

        let req = WalletSignRequest::new(b"test".to_vec(), 137, "evm");
        let resp = evm_signer.sign_request(&req).unwrap();
        assert_eq!(resp.algorithm, "ecdsa");

        let ed_signer = Ed25519Signer::new();
        assert_eq!(ed_signer.algorithm(), "ed25519");
    }

    #[test]
    fn address_masking_integration() {
        let addr = Address::new("0x1234567890abcdef1234567890abcdef12345678");
        // Display should mask
        let display = format!("{}", addr);
        assert!(!display.contains("7890abcd"));
        // Debug should mask
        let debug = format!("{:?}", addr);
        assert!(debug.starts_with("Address("));
        // reveal gives raw
        assert_eq!(addr.reveal(), "0x1234567890abcdef1234567890abcdef12345678");
    }

    #[test]
    fn balance_arithmetic_correctness() {
        let mut b = Balance::new("ACC-1", Currency::USDC, 1000.0);

        // Freeze
        assert!(b.freeze(300.0));
        assert!(approx(b.available, 700.0));
        assert!(approx(b.locked, 300.0));
        assert!(approx(b.total, 1000.0));

        // Partial unfreeze
        assert!(b.unfreeze(100.0));
        assert!(approx(b.available, 800.0));
        assert!(approx(b.locked, 200.0));

        // Debit
        assert!(b.debit(200.0));
        assert!(approx(b.total, 800.0));
        assert!(approx(b.locked, 0.0));

        // Credit
        b.credit(500.0);
        assert!(approx(b.total, 1300.0));
        assert!(approx(b.available, 1300.0));
    }

    #[tokio::test]
    async fn all_diagnostics_integration() {
        let (wallet, account_mgr, balance_mgr, allowance_mgr) = create_default_wallet().unwrap();

        let health = diagnose_wallet_health(&wallet, &account_mgr, &balance_mgr, &allowance_mgr);
        assert!(health.contains("钱包健康诊断"));
        assert!(health.contains("健康"));

        let balance = diagnose_wallet_balance(&balance_mgr);
        assert!(balance.contains("余额"));

        let accounts = diagnose_wallet_accounts(&wallet, &account_mgr);
        assert!(accounts.contains("账户"));

        let allowance = diagnose_wallet_allowance(&allowance_mgr);
        assert!(allowance.contains("授权"));
    }
}
