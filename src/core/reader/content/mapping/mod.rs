//! 读端内容操作符与写端 [`content::Op`] 的双向映射及无损序列化。
//!
//! 三件套：
//! - [`to_content_op`]：读端 `Op` → 写端 `Op`，写端模型无法表达的
//!   操作符（`TD` 双操作数、`d0`/`d1`、`BX`/`EX`、内嵌图像、未建模
//!   透传、越界的 `J`/`j`/`Tr`、非 UTF-8 的 `'`/`"`/`TJ` 串）返回
//!   [`OpMappingError`]；
//! - [`from_content_op`]：写端 `Op` → 读端 `Op`，总是成功；
//!   `MoveNextLineSetLeading`（单操作数 `TD`）按有损方式映射；
//! - [`serialize_ops`]：把读端 `Op` 序列化为可被
//!   [`parse_content_stream`](super::parse_content_stream) 重新解析的
//!   字节——可映射的操作符走写端序列化器，其余走原始回退，保证
//!   `parse → serialize → parse` 模型级往返。

use crate::core::content::{
    ContentStream, LineCap, LineJoin, MarkedContentProperties, Op as WriteOp, TextArrayItem,
};
use crate::core::types::Value;
use crate::core::writer::{format_float, write_value};

use super::parser::hex_upper;
use super::{InlineImage, Op, TextElement};

mod from_op;
mod serialize;
mod to_op;

pub use self::from_op::from_content_op;
pub use self::serialize::serialize_ops;
pub use self::to_op::to_content_op;

/// 读端操作符无法由写端 `content::Op` 模型表达时的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpMappingError {
    /// 写端模型没有对应变体（或该变体的字节形式不合规范）。
    NotRepresentableByWriter { operator: String, reason: String },
    /// 数值操作数超出写端变体的可表示范围（`J`/`j` 非 0–2、`Tr` 非 0–255）。
    OperandOutOfRange { operator: String, value: i64 },
}

impl std::fmt::Display for OpMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRepresentableByWriter { operator, reason } => {
                write!(
                    f,
                    "operator '{operator}' is not representable by writer Op: {reason}"
                )
            }
            Self::OperandOutOfRange { operator, value } => {
                write!(
                    f,
                    "operator '{operator}' operand {value} is out of writer range"
                )
            }
        }
    }
}

impl std::error::Error for OpMappingError {}

fn not_representable(operator: &str, reason: impl Into<String>) -> OpMappingError {
    OpMappingError::NotRepresentableByWriter {
        operator: operator.into(),
        reason: reason.into(),
    }
}
