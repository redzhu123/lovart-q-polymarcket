//! pm-scanner::pipeline：统一模块计时与 Pipeline Timeline（V1.01 第二、八节）。
//!
//! **只增强可观测性，不改变任何交易/策略/Shadow/Execution 逻辑。**
//!
//! - [`ModuleStats`]：单个模块的统一统计（名称 / 耗时 / 输入数 / 输出数 / 成功 / 错误 / 警告）。
//! - [`Stopwatch`]：轻量计时器，`new(name)` 开始，`finish` 产出 [`ModuleStats`]。
//! - [`print_module_stats_table`]：第二节 Execution Timeline 表（模块 / 耗时 / 输入 / 输出）。
//! - [`print_pipeline_timeline`]：第八节 Pipeline（HTTP↓Deserialize↓…↓Storage + 总耗时）。
//!
//! 计时由 driver 在各阶段包围产生；Shadow/Paper/Execution 封装在 Strategy hook 内不可拆分，
//! 故其行报 input/output 事件计数、耗时并入"策略"行并标注。计数才是诊断核心
//! （opportunity=0 -> 三者 input=0/output=0，直接回答"为何无 Paper/Execution/Shadow"）。

use std::time::Instant;

use crate::display::{DASH, SEP};

/// 单个模块的统一统计（V1.01 第二节）。
#[derive(Debug, Clone)]
pub struct ModuleStats {
    /// 模块名（中文，如 "HTTP 请求"）。
    pub name: String,
    /// 耗时（毫秒）。
    pub duration_ms: u128,
    /// 输入数量。
    pub input_count: u64,
    /// 输出数量。
    pub output_count: u64,
    /// 是否成功。
    pub success: bool,
    /// 错误数。
    pub error_count: u64,
    /// 警告数。
    pub warning_count: u64,
}

impl ModuleStats {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            duration_ms: 0,
            input_count: 0,
            output_count: 0,
            success: true,
            error_count: 0,
            warning_count: 0,
        }
    }

    /// 总耗时（毫秒）。
    pub fn total_duration_ms(stats: &[ModuleStats]) -> u128 {
        stats.iter().map(|s| s.duration_ms).sum()
    }
}

/// 轻量计时器：`new(name)` 记录起始，`finish` 产出 [`ModuleStats`]。
///
/// 用法：
/// ```ignore
/// let sw = Stopwatch::new("HTTP 请求");
/// // ... do work ...
/// let ms = sw.finish(0, 100, true); // input=0, output=100, success=true
/// ```
pub struct Stopwatch {
    name: String,
    start: Instant,
}

impl Stopwatch {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
        }
    }

    /// 结束计时，返回 [`ModuleStats`]。
    pub fn finish(self, input: u64, output: u64, success: bool) -> ModuleStats {
        ModuleStats {
            name: self.name,
            duration_ms: self.start.elapsed().as_millis(),
            input_count: input,
            output_count: output,
            success,
            error_count: if success { 0 } else { 1 },
            warning_count: 0,
        }
    }

    /// 结束计时并附带错误 / 警告计数。
    pub fn finish_full(
        self,
        input: u64,
        output: u64,
        success: bool,
        errors: u64,
        warnings: u64,
    ) -> ModuleStats {
        ModuleStats {
            name: self.name,
            duration_ms: self.start.elapsed().as_millis(),
            input_count: input,
            output_count: output,
            success,
            error_count: errors,
            warning_count: warnings,
        }
    }

    /// 不产出 ModuleStats，仅返回已耗毫秒（用于不需要计入表的内部子计时）。
    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }
}

/// 一行 label + 空行 + value + 空行（与既有仪表盘风格一致）。
fn kv(label: &str, value: &str) {
    println!("{}", label);
    println!();
    println!("{}", value);
    println!();
}

/// 第二节：Execution Timeline 表（模块 / 耗时 / 输入 / 输出）。
///
/// 一眼定位数据在哪一步消失：输入 >0 而输出 =0 的模块即"吞掉"数据的环节。
pub fn print_module_stats_table(stats: &[ModuleStats]) {
    println!("{}", SEP);
    println!();
    println!("执行时间线（模块统计）");
    println!();
    println!("{}", DASH);
    println!();
    // 表头
    println!(
        "{:<16} {:>10} {:>10} {:>10}",
        "模块", "耗时(ms)", "输入", "输出"
    );
    println!("{}", DASH);
    for s in stats {
        let mark = if !s.success {
            "🔴"
        } else if s.error_count > 0 || s.warning_count > 0 {
            "🟡"
        } else {
            "🟢"
        };
        println!(
            "{} {:<14} {:>9} {:>10} {:>10}",
            mark, s.name, s.duration_ms, s.input_count, s.output_count
        );
    }
    println!("{}", DASH);
    println!();
    kv(
        "总耗时",
        &format!("{} 毫秒", ModuleStats::total_duration_ms(stats)),
    );
    // 错误 / 警告汇总
    let errors: u64 = stats.iter().map(|s| s.error_count).sum();
    let warnings: u64 = stats.iter().map(|s| s.warning_count).sum();
    if errors > 0 || warnings > 0 {
        kv(
            "错误 / 警告",
            &format!("错误 {} · 警告 {}", errors, warnings),
        );
    }
}

/// 第八节：Pipeline Timeline（HTTP↓Deserialize↓…↓Storage + 总耗时）。
pub fn print_pipeline_timeline(stats: &[ModuleStats]) {
    println!("{}", SEP);
    println!();
    println!("流水线时间线");
    println!();
    println!("{}", DASH);
    println!();
    for (i, s) in stats.iter().enumerate() {
        let mark = if !s.success { "🔴" } else { "🟢" };
        println!("{} {} -- {} 毫秒", mark, s.name, s.duration_ms);
        if i + 1 < stats.len() {
            println!("↓");
        }
    }
    println!();
    kv(
        "总耗时",
        &format!("{} 毫秒", ModuleStats::total_duration_ms(stats)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_stats_total_duration_sums() {
        let stats = vec![
            ModuleStats {
                name: "HTTP".into(),
                duration_ms: 236,
                input_count: 0,
                output_count: 100,
                success: true,
                error_count: 0,
                warning_count: 0,
            },
            ModuleStats {
                name: "策略".into(),
                duration_ms: 2,
                input_count: 15,
                output_count: 0,
                success: true,
                error_count: 0,
                warning_count: 0,
            },
        ];
        assert_eq!(ModuleStats::total_duration_ms(&stats), 238);
    }

    #[test]
    fn stopwatch_finish_records_duration() {
        let sw = Stopwatch::new("测试");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let ms = sw.finish(5, 3, true);
        assert_eq!(ms.name, "测试");
        assert_eq!(ms.input_count, 5);
        assert_eq!(ms.output_count, 3);
        assert!(ms.success);
        assert!(
            ms.duration_ms >= 1,
            "duration should be >= 1ms, got {}",
            ms.duration_ms
        );
        assert_eq!(ms.error_count, 0);
    }

    #[test]
    fn stopwatch_finish_full_carries_errors() {
        let sw = Stopwatch::new("失败模块");
        let ms = sw.finish_full(10, 0, false, 3, 2);
        assert!(!ms.success);
        assert_eq!(ms.error_count, 3);
        assert_eq!(ms.warning_count, 2);
    }

    #[test]
    fn stopwatch_elapsed_ms_does_not_consume() {
        let sw = Stopwatch::new("多次读");
        let _ = sw.elapsed_ms();
        let _ = sw.elapsed_ms();
        let ms = sw.finish(0, 0, true);
        assert_eq!(ms.input_count, 0);
    }
}
