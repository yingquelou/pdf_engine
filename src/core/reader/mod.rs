//! PDF 解析器：文件 → [`crate::core::types::Document`]。
//!
//! 目录模块：
//! - `lexer`：窗口词法器与 token；
//! - `object`：对象/间接对象/trailer/xref 语法解析；
//! - `parser`：文件级 `Parser` 与 `ObjectSource`；
//! - `document`：`Document` 的懒加载对象解析；
//! - `font`：简单 Type 1 字体字典与度量的语义访问；
//! - `xref`：xref 策略链、XRef 流、全文扫描。
//!
//! 解析采用窗口式懒加载，不把整个 PDF 读入内存；兼容经典 xref 表、
//! XRef 流与全文扫描三层回退。

use std::fmt;

/// PDF 解析过程中遇到的致命错误。
///
/// 携带绝对文件字节偏移与可读消息；模块内 Result 别名以本类型为
/// `Err` 分支，调用方可据此报告或尝试恢复。
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PDF parse error at offset {}: {}",
            self.offset, self.message
        )
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

fn err(offset: u64, msg: impl Into<String>) -> ParseError {
    ParseError {
        offset: offset as usize,
        message: msg.into(),
    }
}

/// 默认窗口大小：解析对象 / xref 时从文件读取的字节数。
const DEFAULT_WINDOW: usize = 64 * 1024;
/// 末尾扫描窗口：定位 `startxref` 时读取的文件末尾字节数。
const TAIL_WINDOW: usize = 2048;
/// 全文扫描的块大小（流式扫描，不一次性加载整个文件）。
const SCAN_CHUNK: usize = 256 * 1024;
/// 全文扫描滑动窗口保留的前块尾部字节数（处理跨块关键字）。
const SCAN_OVERLAP: usize = 4096;

/// 在 `hay` 中从 `from` 起查找 `needle`，返回首次出现偏移。
pub(super) fn find_subslice(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    if needle.is_empty() {
        return Some(from);
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

pub mod action;
pub mod annotation;
pub mod color_space;
pub mod content;
mod document;
pub mod embedded;
pub mod font;
pub mod form;
pub mod graphics_state;
pub mod group;
pub mod image;
mod lexer;
pub mod metadata;
pub mod movie;
pub mod navigation;
mod object;
pub mod opi;
pub mod optional_content;
pub mod output_intent;
pub mod page_box;
pub mod page_content;
pub mod page_presentation;
mod parser;
pub mod rendition;
mod security;
pub mod separation;
pub mod shading;
pub mod signature;
pub mod sound;
pub mod structure;
#[cfg(test)]
mod tests;
pub mod text;
pub mod three_d;
pub mod thumbnail;
pub mod transition;
pub mod viewport;
pub mod xmp;
mod xref;

pub use lexer::Lexer;
pub use parser::Parser;

#[cfg(test)]
pub(crate) use object::{parse_indirect_object, parse_object, parse_trailer, parse_xref};
#[cfg(test)]
pub(crate) use parser::ObjectSource;
#[cfg(test)]
pub(crate) use xref::scan_all_objects;
