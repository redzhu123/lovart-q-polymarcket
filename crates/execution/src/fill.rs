//! Fill Engine（Execution Simulator 的成交模拟核心）。
//!
//! Simulation Model -- 以下所有参数与公式均为简化的成交模拟假设，
//! 不是真实市场模型。未来接入真实行情 / 撮合时整体替换本模块即可，
//! ExecutionEngine / CSV / 控制台无需改动。
//!
//! 职责：
//! - 为每笔新订单分配随机成交延迟（0..=`max_fill_delay` 个扫描周期）。
//! - 为每笔订单生成分批成交计划（1~3 批，比例之和为 1.0）。
//! - 根据订单规模计算滑点（size-based，含小幅随机扰动）。
//! - 提供"流动性失败"判定（模拟撮合不到对手盘 -> 零成交 -> Expired）。

use rand::RngExt;

// ---- 分批成交档位概率（Simulation Model，可调）----
/// 单批一次性成交的概率。
const PROB_SINGLE_FILL: f64 = 0.70;
/// 两批成交的概率。
const PROB_TWO_FILL: f64 = 0.20;
// 三批成交概率 = 1 - PROB_SINGLE_FILL - PROB_TWO_FILL = 0.10

/// 流动性失败概率：订单从一开始就撮合不到对手盘 -> Expired（零成交）。
const PROB_LIQUIDITY_FAIL: f64 = 0.04;

// ---- 滑点模型（Simulation Model）----
/// 基础滑点（与规模无关的固定成分）。
const SLIPPAGE_BASE: f64 = 0.0005; // 0.05%
/// 每份额额外滑点（规模冲击成本）：数量越大滑点越高。
const SLIPPAGE_PER_SHARE: f64 = 0.00001; // 每份额 0.001%
/// 滑点随机扰动幅度（±）。
const SLIPPAGE_JITTER: f64 = 0.0001; // ±0.01%

/// Fill Engine：持有线程本地 RNG，提供成交模拟的各种随机决策。
/// Simulation Only -- 不连接任何真实撮合引擎。
pub struct FillEngine {
    rng: rand::rngs::ThreadRng,
    /// 最大成交延迟（扫描周期数），延迟在 0..=max_fill_delay 内随机。
    max_fill_delay: u32,
}

impl FillEngine {
    /// 以最大成交延迟构造（来自 `Config.execution.max_fill_delay`）。
    pub fn new(max_fill_delay: u32) -> Self {
        Self {
            rng: rand::rng(),
            max_fill_delay,
        }
    }

    /// 分配成交延迟（扫描周期数），范围 0..=max_fill_delay。
    pub fn assign_delay(&mut self) -> u32 {
        if self.max_fill_delay == 0 {
            0
        } else {
            self.rng.random_range(0..=self.max_fill_delay)
        }
    }

    /// 判定订单是否"流动性失败"（零成交 -> Expired）。
    pub fn liquidity_fail(&mut self) -> bool {
        self.rng.random_bool(PROB_LIQUIDITY_FAIL)
    }

    /// 生成分批成交计划：返回各批比例（之和为 1.0）。
    /// Simulation Model -- 固定档位 + 随机切分，未来可替换为真实分批模型。
    pub fn partial_schedule(&mut self) -> Vec<f64> {
        let p: f64 = self.rng.random();
        if p < PROB_SINGLE_FILL {
            // 单批：一次性全部成交
            vec![1.0]
        } else if p < PROB_SINGLE_FILL + PROB_TWO_FILL {
            // 两批：随机切分，避免总用 0.5/0.5
            let split = self.rng.random_range(0.3..=0.7);
            vec![split, 1.0 - split]
        } else {
            // 三批：两个随机切点，保证每批都 > 0.05
            let a = self.rng.random_range(0.2..=0.4);
            let b = self.rng.random_range(0.3..=0.5);
            let c = 1.0 - a - b;
            if c > 0.05 {
                vec![a, b, c]
            } else {
                // 切分异常兜底：退化为单批
                vec![1.0]
            }
        }
    }

    /// 计算滑点（小数形式，如 0.0031 = 0.31%）。
    /// Simulation Model -- slippage = SLIPPAGE_BASE + SLIPPAGE_PER_SHARE * quantity + jitter。
    /// 订单规模（份额）越大，冲击成本越高。未来替换为基于订单簿深度的真实滑点。
    pub fn slippage(&mut self, quantity: f64) -> f64 {
        let base = SLIPPAGE_BASE + SLIPPAGE_PER_SHARE * quantity;
        let jitter = self.rng.random_range(-SLIPPAGE_JITTER..=SLIPPAGE_JITTER);
        (base + jitter).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_within_range() {
        let mut f = FillEngine::new(3);
        for _ in 0..100 {
            let d = f.assign_delay();
            assert!(d <= 3);
        }
    }

    #[test]
    fn delay_zero_when_max_zero() {
        let mut f = FillEngine::new(0);
        for _ in 0..50 {
            assert_eq!(f.assign_delay(), 0);
        }
    }

    #[test]
    fn partial_schedule_sums_to_one() {
        let mut f = FillEngine::new(3);
        for _ in 0..100 {
            let s = f.partial_schedule();
            assert!(!s.is_empty());
            let sum: f64 = s.iter().sum();
            assert!((sum - 1.0).abs() < 1e-6, "sum={}", sum);
            for frac in &s {
                assert!(*frac > 0.0);
            }
        }
    }

    #[test]
    fn slippage_nonnegative() {
        let mut f = FillEngine::new(3);
        for q in [0.0, 100.0, 10000.0] {
            assert!(f.slippage(q) >= 0.0);
        }
    }
}
