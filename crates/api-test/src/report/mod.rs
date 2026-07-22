//! 测试报告模块（V1.08）。
//!
//! 自动生成 Markdown / HTML / JSON 格式的测试报告。

pub mod generator;
pub mod types;

pub use generator::ReportGenerator;
pub use types::*;
