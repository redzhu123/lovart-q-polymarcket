//! 响应校验器模块。
//!
//! 提供统一的 API 响应校验：
//! - HTTP 状态码
//! - Content-Type
//! - JSON 解析
//! - JSON Schema 校验
//! - 字段类型/完整性/空值/范围检查

pub mod field;
pub mod response;
pub mod schema;

pub use field::FieldValidator;
pub use response::ResponseValidator;
pub use schema::JsonSchemaValidator;
