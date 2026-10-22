//! PDF 序列化器：`Document` → 字节流。
//!
//! 目录模块：
//! - `value`：值/对象/流的文本序列化；
//! - `version`：输出与内容-版本校验；
//! - `serialize`：xref/对象流策略与对象布局；
//! - `context`：写出阶段的偏移/条目状态；
//! - `document_writer`：`DocumentWriter` 编排与兼容入口。

use std::fmt;
use std::io;

/// 序列化错误。
#[derive(Debug)]
pub enum WriteError {
    Io(io::Error),
    /// Writing encrypted documents is not supported; decrypted content must not
    /// be emitted while retaining the source encryption dictionary.
    EncryptedDocument,
    /// 自由对象号 0 不可被写入为正常对象；或对象号非正等。
    InvalidObject(String),
    /// 目标版本不在 1.0–1.7 范围内，或所需特性高于目标版本支持。
    UnsupportedVersion(String),
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::Io(e) => write!(f, "write failed: {e}"),
            WriteError::EncryptedDocument => {
                write!(f, "writing encrypted PDF documents is not supported")
            }
            WriteError::InvalidObject(s) => write!(f, "invalid object: {s}"),
            WriteError::UnsupportedVersion(s) => {
                write!(f, "unsupported pdf version: {s}")
            }
        }
    }
}

impl std::error::Error for WriteError {}

impl From<io::Error> for WriteError {
    fn from(e: io::Error) -> Self {
        WriteError::Io(e)
    }
}

mod context;
mod document_writer;
mod serialize;
#[cfg(test)]
mod tests;
mod value;
mod version;

pub use document_writer::{DocumentWriter, write_document};
pub use value::{write_object, write_value};

pub(crate) use value::format_float;
