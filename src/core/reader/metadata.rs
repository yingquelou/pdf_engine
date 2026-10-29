//! 元数据与文件标识读取（14.3、14.4）。
//!
//! - `DocumentInfo`：trailer `/Info` 文档信息字典语义封装（Title/Author/Subject/Keywords/Creator/Producer/CreationDate/ModDate/Trapped），
//!   CreationDate/ModDate 可经 [`DocumentInfo::creation_date_parsed`] 解析为 [`PdfDate`]，
//!   Table 317 之外的自定义条目保留在 [`DocumentInfo::dict`]。
//! - `file_identifier`：trailer `/ID` 文件标识符读取（永久 ID + 更新 ID）。

use crate::core::reader::content::text_value;
use crate::core::types::{Dict, Document, PdfDate, Value};

/// `/Info /Trapped` 取值（14.3.3 Table 317，PDF 1.3）。
///
/// 规范要求以名字 `True`/`False`/`Unknown` 写出（不是布尔值）；
/// 缺省值为 `Unknown`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrappedState {
    /// 文档已完成陷印（名字 `True`）。
    Trapped,
    /// 文档尚未陷印（名字 `False`）。
    NotTrapped,
    /// 未知或仅部分陷印（名字 `Unknown`，缺省语义）。
    Unknown,
}

impl TrappedState {
    /// 由 `/Trapped` 名字解析；其他名字返回 `None`。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "True" => Some(Self::Trapped),
            "False" => Some(Self::NotTrapped),
            "Unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// 名字（不含前导 `/`）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Trapped => "True",
            Self::NotTrapped => "False",
            Self::Unknown => "Unknown",
        }
    }
}

/// 文档信息字典（14.3.3 Table 317）。
///
/// 所有字段均为可选；文本串按 PDF 文本串类型（literal/hex）原样保留，
/// 调用方可按需解码为 UTF-16BE 或 PDFDocEncoding。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentInfo {
    pub title: Option<Value>,
    pub author: Option<Value>,
    pub subject: Option<Value>,
    pub keywords: Option<Value>,
    pub creator: Option<Value>,
    pub producer: Option<Value>,
    pub creation_date: Option<Value>,
    pub mod_date: Option<Value>,
    /// `/Trapped` 原始值（PDF 1.3；标准取值为名字 `True`/`False`/`Unknown`）。
    pub trapped: Option<Value>,
    /// 完整 `/Info` 字典；Table 317 之外的自定义文本串条目经此访问。
    pub dict: Dict,
}

impl DocumentInfo {
    /// `/CreationDate` 文本串解析为 PDF 日期（7.9.4）；
    /// 缺失、非文本串、解码失败或格式非法返回 `None`。
    pub fn creation_date_parsed(&self) -> Option<PdfDate> {
        self.creation_date.as_ref().and_then(parse_pdf_date)
    }

    /// `/ModDate` 文本串解析为 PDF 日期（7.9.4）；规则同 [`Self::creation_date_parsed`]。
    pub fn mod_date_parsed(&self) -> Option<PdfDate> {
        self.mod_date.as_ref().and_then(parse_pdf_date)
    }

    /// `/Trapped` 标准名字解析（Table 317）；缺失、非名字或非标准名字返回 `None`。
    pub fn trapped_state(&self) -> Option<TrappedState> {
        self.trapped
            .as_ref()?
            .as_name()
            .and_then(TrappedState::from_name)
    }
}

/// 文本串日期值按 7.9.4 解析；非文本串、解码失败或格式非法返回 `None`。
pub(crate) fn parse_pdf_date(value: &Value) -> Option<PdfDate> {
    PdfDate::parse(&text_value(value)?).ok()
}

impl Document {
    /// 解析 trailer `/Info` 文档信息字典（14.3.3）。
    ///
    /// 支持间接引用；缺失 `/Info` 返回 `None`；字典条目非文本串时保留
    /// 原始 `Value` 供调用方判断。
    pub fn document_info(&self) -> Option<DocumentInfo> {
        let info_ref = self.trailer.dict.get("Info")?;
        let info_value = if let Some((num, _gen)) = info_ref.as_ref() {
            self.get_object(num).ok()??.value
        } else {
            info_ref.clone()
        };
        let dict = info_value.as_dict()?;
        Some(DocumentInfo {
            title: dict.get("Title").cloned(),
            author: dict.get("Author").cloned(),
            subject: dict.get("Subject").cloned(),
            keywords: dict.get("Keywords").cloned(),
            creator: dict.get("Creator").cloned(),
            producer: dict.get("Producer").cloned(),
            creation_date: dict.get("CreationDate").cloned(),
            mod_date: dict.get("ModDate").cloned(),
            trapped: dict.get("Trapped").cloned(),
            dict: dict.clone(),
        })
    }

    /// 读取 trailer `/ID` 文件标识符（14.4）。
    ///
    /// 返回 `(permanent_id, update_id)`；两者均为 16 字节字符串。
    /// 缺失 `/ID` 或格式非法返回 `None`。
    pub fn file_identifier(&self) -> Option<([u8; 16], [u8; 16])> {
        let id_array = self.trailer.dict.get("ID")?.as_array()?;
        if id_array.len() < 2 {
            return None;
        }
        let perm_bytes = string_to_bytes(&id_array.0[0])?;
        let upd_bytes = string_to_bytes(&id_array.0[1])?;
        if perm_bytes.len() != 16 || upd_bytes.len() != 16 {
            return None;
        }
        let mut perm_arr = [0u8; 16];
        let mut upd_arr = [0u8; 16];
        perm_arr.copy_from_slice(&perm_bytes);
        upd_arr.copy_from_slice(&upd_bytes);
        Some((perm_arr, upd_arr))
    }

    /// 读取 XMP 元数据流（14.3.2）：目录 `/Metadata` 指向流的解码原文。
    ///
    /// 缺失、非流或解码失败返回 `None`（本层不校验 XML 良构性）。
    pub fn xmp_metadata(&self) -> Option<Vec<u8>> {
        let catalog = crate::core::reader::navigation::catalog(self).ok()?;
        let metadata = catalog.get("Metadata")?;
        let resolved = if let Some((num, _gen)) = metadata.as_ref() {
            self.get_object(num).ok()??
        } else {
            crate::core::types::Object {
                num: 0,
                generation: 0,
                value: metadata.clone(),
            }
        };
        if !matches!(resolved.value, Value::Stream(_, _)) {
            return None;
        }
        self.decode_stream_bytes(&resolved.value).ok()
    }
}

fn string_to_bytes(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::String(s) => Some(s.as_bytes().to_vec()),
        Value::HexString(s) => hex_decode(s),
        _ => None,
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.replace(' ', "");
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{Array, Dict, Object, Xref};

    fn make_doc_with_info(dict: Dict) -> Document {
        let mut trailer = crate::core::types::Trailer::new();
        trailer.dict = trailer
            .dict
            .clone()
            .insert("Info", Value::Ref(2, 0))
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(3));
        let objects = vec![
            Object {
                num: 1,
                generation: 0,
                value: Value::Dict(Dict::new()),
            },
            Object {
                num: 2,
                generation: 0,
                value: Value::Dict(dict),
            },
        ];
        Document::from_objects(objects, Xref::new(), trailer, Default::default())
    }

    #[test]
    fn parses_document_info_dictionary() {
        let dict = Dict::new()
            .insert("Title", Value::String("My Document".into()))
            .insert("Author", Value::String("John Doe".into()))
            .insert("Subject", Value::String("Testing".into()))
            .insert("Keywords", Value::String("pdf, test".into()))
            .insert("Creator", Value::String("Test App".into()))
            .insert("Producer", Value::String("pdf_engine".into()))
            .insert("CreationDate", Value::String("D:20261027".into()))
            .insert("ModDate", Value::String("D:20261027".into()));
        let doc = make_doc_with_info(dict);
        let info = doc.document_info().expect("info should exist");
        assert!(matches!(info.title, Some(Value::String(_))));
        assert!(matches!(info.author, Some(Value::String(_))));
        assert!(matches!(info.subject, Some(Value::String(_))));
        assert!(matches!(info.keywords, Some(Value::String(_))));
        assert!(matches!(info.creator, Some(Value::String(_))));
        assert!(matches!(info.producer, Some(Value::String(_))));
        assert!(matches!(info.creation_date, Some(Value::String(_))));
        assert!(matches!(info.mod_date, Some(Value::String(_))));
    }

    #[test]
    fn document_info_resolves_indirect_reference() {
        let dict = Dict::new().insert("Title", Value::String("Test".into()));
        let doc = make_doc_with_info(dict);
        let info = doc.document_info().expect("info should exist");
        assert!(info.title.is_some());
    }

    #[test]
    fn document_info_returns_none_when_absent() {
        let trailer = crate::core::types::Trailer::new();
        let objects = vec![Object {
            num: 1,
            generation: 0,
            value: Value::Dict(Dict::new()),
        }];
        let doc = Document::from_objects(objects, Xref::new(), trailer, Default::default());
        assert!(doc.document_info().is_none());
    }

    #[test]
    fn parses_file_identifier() {
        let mut trailer = crate::core::types::Trailer::new();
        let id_array = Value::Array(Array(vec![
            Value::HexString("01010101010101010101010101010101".into()),
            Value::HexString("02020202020202020202020202020202".into()),
        ]));
        trailer.dict = trailer
            .dict
            .clone()
            .insert("ID", id_array)
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(2));
        let objects = vec![Object {
            num: 1,
            generation: 0,
            value: Value::Dict(Dict::new()),
        }];
        let doc = Document::from_objects(objects, Xref::new(), trailer, Default::default());
        let (perm, upd) = doc.file_identifier().expect("ID should exist");
        assert_eq!(perm, [1u8; 16]);
        assert_eq!(upd, [2u8; 16]);
    }

    #[test]
    fn document_info_parses_typed_dates_trapped_and_custom_entries() {
        let dict = Dict::new()
            .insert(
                "CreationDate",
                Value::String("D:20261029120000+08'00'".into()),
            )
            .insert("ModDate", Value::String("D:20261301".into()))
            .insert("Trapped", Value::Name("Unknown".into()))
            .insert("Custom", Value::String("custom value".into()));
        let doc = make_doc_with_info(dict);
        let info = doc.document_info().expect("info should exist");

        let created = info.creation_date_parsed().expect("/CreationDate parses");
        assert_eq!(created.year(), 2026);
        assert_eq!(created.month(), Some(10));
        assert_eq!(created.day(), Some(29));
        assert_eq!(created.hour(), Some(12));
        assert_eq!(created.minute(), Some(0));
        assert_eq!(created.second(), Some(0));
        assert_eq!(created.to_pdf_string(), "D:20261029120000+08'00'");

        // 13 月不是合法日期：原始值仍保留，类型化解析为 None。
        assert!(info.mod_date.is_some());
        assert_eq!(info.mod_date_parsed(), None);

        assert_eq!(info.trapped_state(), Some(TrappedState::Unknown));
        assert_eq!(
            info.dict.get("Custom"),
            Some(&Value::String("custom value".into()))
        );
    }

    #[test]
    fn document_info_typed_accessors_require_well_formed_values() {
        let dict = Dict::new()
            .insert("CreationDate", Value::Integer(20261029))
            .insert("ModDate", Value::String("20261029".into()))
            .insert("Trapped", Value::Bool(true));
        let doc = make_doc_with_info(dict);
        let info = doc.document_info().expect("info should exist");
        // 非文本串日期、缺 `D:` 前缀的日期、布尔 `/Trapped` 均不类型化。
        assert_eq!(info.creation_date_parsed(), None);
        assert_eq!(info.mod_date_parsed(), None);
        assert!(info.trapped.is_some());
        assert_eq!(info.trapped_state(), None);
    }

    #[test]
    fn file_identifier_returns_none_when_absent() {
        let trailer = crate::core::types::Trailer::new();
        let objects = vec![Object {
            num: 1,
            generation: 0,
            value: Value::Dict(Dict::new()),
        }];
        let doc = Document::from_objects(objects, Xref::new(), trailer, Default::default());
        assert!(doc.file_identifier().is_none());
    }

    #[test]
    fn file_identifier_rejects_wrong_length() {
        let mut trailer = crate::core::types::Trailer::new();
        let id_array = Value::Array(Array(vec![
            Value::HexString("0101010101010101010101010101".into()),
            Value::HexString("02020202020202020202020202020202".into()),
        ]));
        trailer.dict = trailer
            .dict
            .clone()
            .insert("ID", id_array)
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(2));
        let objects = vec![Object {
            num: 1,
            generation: 0,
            value: Value::Dict(Dict::new()),
        }];
        let doc = Document::from_objects(objects, Xref::new(), trailer, Default::default());
        assert!(doc.file_identifier().is_none());
    }
}
