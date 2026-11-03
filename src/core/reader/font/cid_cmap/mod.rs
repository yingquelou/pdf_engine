//! 嵌入 CMap（码空间/码→CID）与 ToUnicode CMap（码→Unicode）解析。
//!
//! - `cid`：Type 0 `/Encoding` 的嵌入 CMap，含 `usecmap` 基座合并；
//! - `to_unicode`：`/ToUnicode` CMap，含 `usecmap` 基座映射继承；
//! - `use_cmap`：CMap **流字典**的 `/UseCMap` 基座（9.7.5.3，Table 120）；
//! - `cmap_dict`：CMap 流字典（Table 120）全部条目的类型化读端，含流内
//!   `def` 声明的对照采集；
//! - `consistency`：CMap ↔ CIDFont 一致性的只读诊断出口（只报告、不改行为）；
//! - `token`：两个解析器共用的 PostScript 词法与段辅助。

mod cid;
mod cmap_dict;
mod consistency;
mod to_unicode;
mod token;
mod use_cmap;

#[cfg(test)]
mod tests;

pub(super) use cid::{CidCMap, identity_cmap};
pub use cmap_dict::{
    CMapBaseReference, CMapDictionary, CMapEntry, CMapFileDeclarations, CMapWritingMode,
};
pub use consistency::{CMapConsistencyIssue, CMapConsistencyRule, CMapPosition, Type0CMaps};
pub(super) use to_unicode::ToUnicodeMap;
pub(super) use use_cmap::{parse_to_unicode_stream, resolve_cid_base};

use super::{FontError, FontResult};

const MAX_CMAP_BYTES: usize = 16 * 1024 * 1024;
const MAX_CMAP_ENTRIES: usize = 1_000_000;
const MAX_UNICODE_BYTES: usize = 1024;

/// 源字符码空间：同长度码值的闭区间。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeSpace {
    length: usize,
    low: u32,
    high: u32,
}

impl CodeSpace {
    fn contains(&self, length: usize, code: u32) -> bool {
        self.length == length && (self.low..=self.high).contains(&code)
    }
}

fn invalid_cmap(message: &str) -> FontError {
    FontError::Invalid(format!("invalid CID CMap: {message}"))
}
