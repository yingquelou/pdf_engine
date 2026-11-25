//! 页面件字典（14.5 Page-Piece Dictionaries，Table 318/319）。
//!
//! 目录（Table 28）、页面（Table 30）与 Form XObject（Table 95）的
//! `/PieceInfo` 字典按产品名索引到 data dictionary：`LastModified`
//! （必填日期）与可选 `Private`（任意私有数据）。本层只做类型化读取，
//! 不解释私有数据。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::reader::navigation::{catalog, resolve};
use crate::core::types::{Dict, Document, PdfDate, Value};

/// `/PieceInfo` 读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum PieceInfoError {
    /// 目录/页面树不可解析。
    Navigation(String),
    /// 条目结构损坏。
    Malformed(String),
}

impl fmt::Display for PieceInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Navigation(message) | Self::Malformed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for PieceInfoError {}

fn fail(message: impl Into<String>) -> PieceInfoError {
    PieceInfoError::Malformed(message.into())
}

fn navigation(error: crate::core::reader::navigation::NavigationError) -> PieceInfoError {
    PieceInfoError::Navigation(error.to_string())
}

/// 单个产品的 data dictionary（Table 319）。
#[derive(Debug, Clone, PartialEq)]
pub struct DataDictionary {
    /// `/LastModified` 原始日期串。
    pub last_modified: Option<String>,
    /// `/LastModified` 解析后的 PDF 日期（格式非法时为 `None`，原文保留）。
    pub last_modified_parsed: Option<PdfDate>,
    /// `/Private` 私有数据（任意类型，原样保留）。
    pub private: Option<Value>,
    /// 完整 data dictionary，含未知条目。
    pub dict: Dict,
}

/// 已解析的页面件字典：产品名 → data dictionary。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PieceInfo {
    entries: BTreeMap<String, DataDictionary>,
}

impl PieceInfo {
    /// 全部条目（按产品名升序）。
    pub fn entries(&self) -> &BTreeMap<String, DataDictionary> {
        &self.entries
    }

    /// 按产品名取 data dictionary。
    pub fn get(&self, product: &str) -> Option<&DataDictionary> {
        self.entries.get(product)
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Document {
    /// 解析任意 `/PieceInfo` 字典值（解引用后按 Table 318/319 读取）。
    pub fn piece_info(&self, value: &Value) -> Result<PieceInfo, PieceInfoError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        let dict = resolved
            .as_dict()
            .ok_or_else(|| fail("/PieceInfo is not a dictionary"))?;
        let mut entries = BTreeMap::new();
        for (product, raw) in &dict.0 {
            let resolved = resolve(self, raw).map_err(navigation)?;
            let data = resolved
                .as_dict()
                .ok_or_else(|| fail(format!("/PieceInfo /{product} is not a data dictionary")))?;
            let last_modified = match data.get("LastModified") {
                Some(Value::String(text)) => Some(text.clone()),
                Some(Value::HexString(text)) => Some(decode_hex_string(text)),
                Some(other) => {
                    return Err(fail(format!(
                        "/PieceInfo /{product} /LastModified is not a text string ({})",
                        other.type_name()
                    )));
                }
                None => None,
            };
            let last_modified_parsed = last_modified.as_ref().and_then(|text| {
                crate::core::reader::metadata::parse_pdf_date(&Value::String(text.clone()))
            });
            entries.insert(
                product.clone(),
                DataDictionary {
                    last_modified,
                    last_modified_parsed,
                    private: data.get("Private").cloned(),
                    dict: data.clone(),
                },
            );
        }
        Ok(PieceInfo { entries })
    }

    /// 目录 `/PieceInfo`（PDF 1.4）。
    pub fn catalog_piece_info(&self) -> Result<Option<PieceInfo>, PieceInfoError> {
        let catalog = catalog(self).map_err(navigation)?;
        let Some(value) = catalog.get("PieceInfo") else {
            return Ok(None);
        };
        self.piece_info(value).map(Some)
    }

    /// Form XObject 的 `/PieceInfo`（Table 95，PDF 1.3）；`value` 为表单流或其引用。
    pub fn form_piece_info(&self, value: &Value) -> Result<Option<PieceInfo>, PieceInfoError> {
        let resolved = resolve(self, value).map_err(navigation)?;
        let Value::Stream(dict, _) = &resolved else {
            return Err(fail("form XObject is not a stream"));
        };
        let Some(piece) = dict.get("PieceInfo") else {
            return Ok(None);
        };
        self.piece_info(piece).map(Some)
    }

    /// 第 `page_index`（0 基）页的 `/PieceInfo`（PDF 1.3）。
    pub fn page_piece_info(&self, page_index: usize) -> Result<Option<PieceInfo>, PieceInfoError> {
        let page = self
            .locate_page_node(page_index)
            .map_err(|error| PieceInfoError::Navigation(error.to_string()))?;
        let dict = page
            .as_dict()
            .ok_or_else(|| fail("page node is not a dictionary"))?;
        let Some(value) = dict.get("PieceInfo") else {
            return Ok(None);
        };
        self.piece_info(value).map(Some)
    }
}

/// 十六进制文本串解码为字节串再按 Latin-1 取值（`/LastModified` 为日期 ASCII）。
fn decode_hex_string(hex: &str) -> String {
    let digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let mut out = String::with_capacity(digits.len() / 2);
    for pair in digits.as_chunks::<2>().0 {
        let hi = (pair[0] as char).to_digit(16).unwrap_or(0) as u8;
        let lo = (pair[1] as char).to_digit(16).unwrap_or(0) as u8;
        out.push((hi << 4 | lo) as char);
    }
    out
}
