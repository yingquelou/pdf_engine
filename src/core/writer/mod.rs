//! PDF 序列化器：`Document` → 字节流。
//!
//! 目录模块：
//! - `value`：值/对象/流的文本序列化；
//! - `version`：输出与内容-版本校验；
//! - `serialize`：xref/对象流策略与对象布局；
//! - `context`：写出阶段的偏移/条目状态；
//! - `document_writer`：`DocumentWriter` 编排与兼容入口；
//! - `incremental`：增量更新写出（7.5.6，`/Prev` 链）；
//! - `hybrid`：混合引用文件写出（7.5.8.4，经典主表 + `/XRefStm` 更新段）；
//! - `signature`：签名增量更新（ByteRange 不动点与 Contents 回填）。

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
    /// 流压缩后端失败（注入的压缩后端返回错误）。
    /// writer 先缓冲后一次性写出，失败时输出端不会收到任何字节。
    Compression(String),
    /// 试图把流对象打包进对象流（`/ObjStm`）——ISO 32000-1 7.5.7 明确禁止
    /// 「Stream objects」存于对象流。writer 在写出任何字节前拒绝；打包
    /// 选择本身已把流对象留在顶层，本变体只可能在调用方绕过策略直接
    /// 请求打包时出现（`write_object_stream` 的咽喉点守卫）。
    StreamInObjectStream(i64),
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
            WriteError::Compression(s) => write!(f, "stream compression failed: {s}"),
            WriteError::StreamInObjectStream(num) => write!(
                f,
                "7.5.7 forbids storing stream objects in an object stream: object {num} \
                 is a stream and was refused (it must stay a top-level object)"
            ),
        }
    }
}

impl std::error::Error for WriteError {}

impl From<io::Error> for WriteError {
    fn from(e: io::Error) -> Self {
        WriteError::Io(e)
    }
}

mod compression;
mod context;
mod document_writer;
mod hybrid;
mod incremental;
mod serialize;
mod signature;
#[cfg(test)]
mod tests;
mod value;
mod version;

pub use compression::{DeflateBackend, Flate2DeflateBackend, ImagePredictor, StreamCompression};
pub use document_writer::{DocumentWriter, write_document};
pub use hybrid::HybridReferencePolicy;
pub use signature::{SignedIncremental, SigningSpec};
pub use value::{write_object, write_value};

pub(crate) use value::{format_float, validate_ascii85_payloads};
