//! 附录 F.4 hint 表读端的集成验证：自己按 F.3 分部装配一个**真正线性化**的
//! 最小文件，把位级手工组装的页偏移 hint 表（F.4.1，Table F.3/F.4）与共享
//! 对象 hint 表（F.4.2，Table F.5/F.6）注入主 hint 流，再从公开 API
//! （`Document::linearization` / `Document::linearization_hints`）解析回
//! 类型化数据。
//!
//! 页偏移表按 F.4 前言恒在流偏移 0、且没有 Table F.2 键，故它的字节长度就是
//! 后面那张表 `/S` 的最小合法位置：本文件把两表按真实字节相邻装配，再用
//! `hints.page_offset_end_byte()` 与 `position_of("S")` 相互印证。
//!
//! 本库写端尚不产出线性化布局（`docs/ROADMAP.md` 的 F5 缺口），`PdfBuilder`
//! 也就无从直接造出 F.3.1 要求的两段交叉引用表形态，因此这里按分部顺序
//! 逐字节写出；所有可变数字一律定宽，两次装配即可让偏移收敛到自洽。

use pdf_engine::core::reader::linearization::LinearizationIssue;
use pdf_engine::core::reader::linearization::hints::SharedObjectHintTable;

use super::*;

/// F.4.3 缩略图 hint 表的集成用例（同目录子模块，复用本文件的位写手与装配器；
/// 本文件已逼近 600 行/25600 字节的上限，故新家族独立成文件）。
mod thumbnail;

/// 位写手与分部装配脚手架（供本目录各家族模块复用）。
mod support;

/// F.4.1 页偏移 hint 表家族。
mod page_offset;

/// F.4.2 共享对象 hint 表家族。
mod shared_object;

/// F.4.4 generic hint 表家族。
mod generic;

use support::{
    BitWriter, PAGE_COUNT, build, hint_entries, hint_payload, page_offset_bytes,
    page_offset_table_bits, shared_object_table_bits,
};
