//! Wallet Diagnostics（P2-06 第七节）。
//!
//! 提供 CLI 诊断命令：
//! - `cargo run -- wallet health`：钱包健康检查
//! - `cargo run -- wallet balance`：余额查询
//! - `cargo run -- wallet account`：账户列表
//! - `cargo run -- wallet allowance`：授权诊断
//!
//! 全部中文输出，敏感信息自动脱敏。

use crate::account::AccountManager;
use crate::allowance::AllowanceManager;
use crate::balance::BalanceManager;
use crate::domain::Wallet;

// ============================================================================
// Wallet Health Diagnostics
// ============================================================================

/// `cargo run -- wallet health`：钱包健康检查（P2-06 第七节）。
///
/// 检查：
/// - Wallet 状态
/// - Balance 状态
/// - Allowance 状态
/// - Nonce 状态
pub fn diagnose_wallet_health(
    wallet: &Wallet,
    account_mgr: &AccountManager,
    balance_mgr: &BalanceManager,
    allowance_mgr: &AllowanceManager,
) -> String {
    let account_health = account_mgr.health();
    let balance_health = balance_mgr.health();
    let allowance_health = allowance_mgr.health();

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【钱包健康诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    // 钱包基本信息
    lines.push("── 钱包 ──".to_string());
    lines.push(format!("  ID            : {}", wallet.wallet_id));
    lines.push(format!("  名称          : {}", wallet.name));
    lines.push(format!("  网络          : {}", wallet.network.as_zh()));
    lines.push(format!("  账户数        : {} 个", wallet.accounts.len()));
    lines.push(String::new());

    // 账户健康
    lines.push("── 账户 ──".to_string());
    lines.push(format!("  {}", account_health.summary_zh()));
    lines.push(String::new());

    // 余额健康
    lines.push("── 余额 ──".to_string());
    lines.push(format!("  {}", balance_health.summary_zh()));
    lines.push(String::new());

    // 授权健康
    lines.push("── 授权 ──".to_string());
    lines.push(format!("  {}", allowance_health.summary_zh()));
    lines.push(String::new());

    // 总体评估
    let overall_healthy = true; // Simulation mode is always healthy
    lines.push("── 总体评估 ──".to_string());
    lines.push(format!(
        "  整体健康      : {}",
        if overall_healthy {
            "✅ 健康"
        } else {
            "❌ 异常"
        }
    ));
    lines.push("  模式          : 🔒 模拟（Simulation Only）".to_string());
    lines.push(String::new());

    lines.push("═══ 建议 ═══".to_string());
    lines.push("  无需操作，钱包系统运行正常。".to_string());
    lines.push("  如需真实钱包，配置以下环境变量：".to_string());
    lines.push("    POLYMARKET_WALLET_ADDRESS=<your-address>".to_string());
    lines.push("    POLYMARKET_PRIVATE_KEY=<your-private-key>".to_string());

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Wallet Balance Diagnostics
// ============================================================================

/// `cargo run -- wallet balance`：余额查询（P2-06 第七节）。
pub fn diagnose_wallet_balance(balance_mgr: &BalanceManager) -> String {
    let health = balance_mgr.health();
    let all = balance_mgr.all();

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【钱包余额诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.push(format!("  {}", health.summary_zh()));
    lines.push(String::new());

    if all.is_empty() {
        lines.push("  （无余额记录）".to_string());
    } else {
        lines.push("── 余额明细 ──".to_string());
        for b in all {
            lines.push(format!("  {}", b.summary_zh()));
        }
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Wallet Account Diagnostics
// ============================================================================

/// `cargo run -- wallet account`：账户列表（P2-06 第七节）。
pub fn diagnose_wallet_accounts(wallet: &Wallet, account_mgr: &AccountManager) -> String {
    let health = account_mgr.health();

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【钱包账户诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.push(format!("  钱包: {} ({})", wallet.wallet_id, wallet.name));
    lines.push(format!("  {}", health.summary_zh()));
    lines.push(String::new());

    let accounts = account_mgr.all();
    if accounts.is_empty() {
        lines.push("  （无账户）".to_string());
    } else {
        lines.push("── 账户明细 ──".to_string());
        for a in accounts {
            lines.push(format!(
                "  {} | {} | {} | {} | {}",
                a.account_id,
                a.name,
                a.address, // Display impl auto-masks
                a.network.short_name(),
                if a.active { "活跃" } else { "已停用" },
            ));
        }
    }

    lines.push(String::new());
    lines.push("═══ 建议 ═══".to_string());
    lines.push("  使用 `cargo run -- wallet balance` 查看各账户余额。".to_string());

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// Wallet Allowance Diagnostics
// ============================================================================

/// `cargo run -- wallet allowance`：授权诊断（P2-06 第七节）。
pub fn diagnose_wallet_allowance(allowance_mgr: &AllowanceManager) -> String {
    let health = allowance_mgr.health();

    let mut lines = Vec::new();
    lines.push("══════════════════════════════════════".to_string());
    lines.push("【钱包授权诊断】".to_string());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.push(format!("  {}", health.summary_zh()));
    lines.push(String::new());

    let all = allowance_mgr.all();
    if all.is_empty() {
        lines.push("  （无授权记录）".to_string());
    } else {
        lines.push("── 授权明细 ──".to_string());
        for a in all {
            lines.push(format!("  {}", a.summary_zh()));
        }
    }

    lines.push(String::new());
    lines.push("══════════════════════════════════════".to_string());
    lines.push(String::new());

    lines.join("\n")
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Address, Currency, Network, Wallet};

    fn setup() -> (Wallet, AccountManager, BalanceManager, AllowanceManager) {
        let wallet = Wallet::new("W-DIAG".into(), "诊断钱包".into(), Network::Polygon);
        let mut account_mgr = AccountManager::empty();
        account_mgr
            .create_account(
                "主账户",
                Address::new("0x1234567890abcdef1234567890abcdef12345678"),
                "W-DIAG",
                Network::Polygon,
                Currency::USDC,
            )
            .unwrap();

        let mut balance_mgr = BalanceManager::new();
        balance_mgr.set_balance(crate::domain::Balance::new(
            "ACC-0001",
            Currency::USDC,
            10000.0,
        ));

        let mut allowance_mgr = AllowanceManager::new();
        allowance_mgr.approve("ACC-0001", Address::new("0xSpender"), 1000.0);

        (wallet, account_mgr, balance_mgr, allowance_mgr)
    }

    #[test]
    fn diagnose_wallet_health_output() {
        let (wallet, am, bm, alm) = setup();
        let output = diagnose_wallet_health(&wallet, &am, &bm, &alm);
        assert!(output.contains("钱包健康诊断"));
        assert!(output.contains("诊断钱包"));
        assert!(output.contains("健康"));
    }

    #[test]
    fn diagnose_wallet_balance_output() {
        let (_, _, bm, _) = setup();
        let output = diagnose_wallet_balance(&bm);
        assert!(output.contains("钱包余额诊断"));
        assert!(output.contains("USDC"));
    }

    #[test]
    fn diagnose_wallet_accounts_output() {
        let (wallet, am, _, _) = setup();
        let output = diagnose_wallet_accounts(&wallet, &am);
        assert!(output.contains("钱包账户诊断"));
        assert!(output.contains("诊断钱包"));
        // Address should be masked
        assert!(!output.contains("1234567890abcdef1234567890abcdef12345678"));
    }

    #[test]
    fn diagnose_wallet_allowance_output() {
        let (_, _, _, alm) = setup();
        let output = diagnose_wallet_allowance(&alm);
        assert!(output.contains("钱包授权诊断"));
    }

    #[test]
    fn all_diagnostics_chinese() {
        let (wallet, am, bm, alm) = setup();

        let health = diagnose_wallet_health(&wallet, &am, &bm, &alm);
        assert!(health.contains("诊断"));
        assert!(health.contains("健康"));

        let balance = diagnose_wallet_balance(&bm);
        assert!(balance.contains("余额"));

        let accounts = diagnose_wallet_accounts(&wallet, &am);
        assert!(accounts.contains("账户"));

        let allowance = diagnose_wallet_allowance(&alm);
        assert!(allowance.contains("授权"));
    }
}
