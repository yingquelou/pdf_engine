//! 元数据与文件标识写端（14.3.3、14.4）。
//!
//! - [`DocumentInfo`]：trailer `/Info` 文档信息字典的语义封装；
//! - [`FileIdentifier`]：trailer `/ID` 文件标识符（永久 ID + 更新 ID），
//!   含基于内容的确定性生成算法（供 E7 增量更新复用）。

use crate::core::reader::metadata::TrappedState;
use crate::core::types::{Array, Dict, PdfDate, PdfVersion, Value};
use crate::core::writer::WriteError;

/// 写端文档信息字典（14.3.3 Table 317）。
///
/// 全部字段可选；空字段不写出。文本串按 PDF 文本串规则编码
/// （非 ASCII 转 UTF-16BE 十六进制串）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentInfo {
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    keywords: Option<String>,
    creator: Option<String>,
    producer: Option<String>,
    creation_date: Option<String>,
    mod_date: Option<String>,
    trapped: Option<TrappedState>,
}

impl DocumentInfo {
    /// 新建空信息字典。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `/Title`。
    pub fn title(mut self, value: impl Into<String>) -> Self {
        self.title = Some(value.into());
        self
    }

    /// 设置 `/Author`。
    pub fn author(mut self, value: impl Into<String>) -> Self {
        self.author = Some(value.into());
        self
    }

    /// 设置 `/Subject`。
    pub fn subject(mut self, value: impl Into<String>) -> Self {
        self.subject = Some(value.into());
        self
    }

    /// 设置 `/Keywords`。
    pub fn keywords(mut self, value: impl Into<String>) -> Self {
        self.keywords = Some(value.into());
        self
    }

    /// 设置 `/Creator`。
    pub fn creator(mut self, value: impl Into<String>) -> Self {
        self.creator = Some(value.into());
        self
    }

    /// 设置 `/Producer`。
    pub fn producer(mut self, value: impl Into<String>) -> Self {
        self.producer = Some(value.into());
        self
    }

    /// 设置 `/CreationDate`（日期字符串按 `D:YYYYMMDDHHmmSS±HH'MM'` 原样写出）。
    pub fn creation_date(mut self, value: impl Into<String>) -> Self {
        self.creation_date = Some(value.into());
        self
    }

    /// 设置 `/ModDate`。
    pub fn mod_date(mut self, value: impl Into<String>) -> Self {
        self.mod_date = Some(value.into());
        self
    }

    /// 以类型化 PDF 日期设置 `/CreationDate`（按 7.9.4 规范形式写出）。
    pub fn creation_date_pdf(mut self, value: PdfDate) -> Self {
        self.creation_date = Some(value.to_pdf_string());
        self
    }

    /// 以类型化 PDF 日期设置 `/ModDate`。
    pub fn mod_date_pdf(mut self, value: PdfDate) -> Self {
        self.mod_date = Some(value.to_pdf_string());
        self
    }

    /// 设置 `/Trapped`（PDF 1.3，Table 317；以名字写出）。
    pub fn trapped(mut self, value: TrappedState) -> Self {
        self.trapped = Some(value);
        self
    }

    /// 是否所有字段均为空（空字典不写出）。
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.author.is_none()
            && self.subject.is_none()
            && self.keywords.is_none()
            && self.creator.is_none()
            && self.producer.is_none()
            && self.creation_date.is_none()
            && self.mod_date.is_none()
            && self.trapped.is_none()
    }

    /// 构造 `/Info` 字典；只写非空字段。
    ///
    /// `/Trapped` 自 PDF 1.3 引入（Table 317），目标版本更低时
    /// 返回 `UnsupportedVersion`。
    pub(crate) fn to_dict(&self, version: PdfVersion) -> Result<Dict, WriteError> {
        if self.trapped.is_some() && version < PdfVersion::V1_3 {
            return Err(WriteError::UnsupportedVersion(format!(
                "/Info /Trapped requires PDF {}, got {}",
                PdfVersion::V1_3.as_str(),
                version.as_str()
            )));
        }
        use crate::core::builder::types::text_string_value;
        let mut dict = Dict::new();
        for (key, value) in [
            ("Title", &self.title),
            ("Author", &self.author),
            ("Subject", &self.subject),
            ("Keywords", &self.keywords),
            ("Creator", &self.creator),
            ("Producer", &self.producer),
            ("CreationDate", &self.creation_date),
            ("ModDate", &self.mod_date),
        ] {
            if let Some(text) = value {
                dict = dict.insert(key, text_string_value(text));
            }
        }
        if let Some(trapped) = self.trapped {
            dict = dict.insert("Trapped", Value::Name(trapped.name().into()));
        }
        Ok(dict)
    }
}

/// 文件标识符（14.4）：trailer `/ID` 的两个 16 字节串。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIdentifier {
    /// 永久 ID：文件创建时确定，之后不再改变。
    pub permanent: [u8; 16],
    /// 更新 ID：每次修改文件时更新。
    pub update: [u8; 16],
}

impl FileIdentifier {
    /// 显式构造。
    pub fn new(permanent: [u8; 16], update: [u8; 16]) -> Self {
        Self { permanent, update }
    }

    /// 按内容确定性生成（14.4 推荐对创建时间、文件大小、前 4 KB 内容等
    /// 求哈希；此处以 FNV-1a 128 位变体对传入摘要材料计算，无第三方
    /// 依赖）。永久 ID 与更新 ID 初始相同；增量更新时由调用方更新
    /// `update` 字段（E7 复用本 API）。
    pub fn generate(material: &[u8]) -> Self {
        let digest = fnv128(material);
        Self {
            permanent: digest,
            update: digest,
        }
    }

    /// 构造 `/ID` 数组值（两个十六进制字符串）。
    pub(crate) fn to_value(self) -> Value {
        Value::Array(Array(vec![
            Value::HexString(hex_encode(&self.permanent)),
            Value::HexString(hex_encode(&self.update)),
        ]))
    }

    /// 校验读取到的 `/ID` 与本标识符的永久 ID 是否一致（E7 增量更新
    /// 前置检查：`/Prev` 链上所有版本的永久 ID 必须相同）。
    pub fn permanent_matches(&self, other: &[u8; 16]) -> bool {
        self.permanent == *other
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

/// FNV-1a 128 位变体：两个独立 64 位 FNV-1a 串联（不同初始向量），
/// 输出 16 字节。仅用于确定性 ID 生成，非密码学哈希。
fn fnv128(data: &[u8]) -> [u8; 16] {
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x9e37_79b9_7f4a_7c15;
    for &byte in data {
        h1 ^= byte as u64;
        h1 = h1.wrapping_mul(PRIME);
        h2 ^= byte as u64;
        h2 = h2.wrapping_mul(PRIME);
        h2 = h2.rotate_left(13);
    }
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&h1.to_le_bytes());
    out[8..].copy_from_slice(&h2.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_info_writes_only_nonempty_fields() {
        let info = DocumentInfo::new().title("T").author("A");
        let dict = info.to_dict(PdfVersion::V1_7).expect("1.7 accepts /Info");
        assert!(dict.get("Title").is_some());
        assert!(dict.get("Author").is_some());
        assert!(dict.get("Subject").is_none());
        assert!(dict.get("Trapped").is_none());
        assert!(!info.is_empty());
        assert!(DocumentInfo::new().is_empty());
    }

    #[test]
    fn document_info_writes_typed_dates_and_trapped() {
        let created = PdfDate::parse("D:20261029120000+08'00'").expect("date");
        let modified = PdfDate::parse("D:20261030093015Z").expect("date");
        let info = DocumentInfo::new()
            .creation_date_pdf(created)
            .mod_date_pdf(modified)
            .trapped(TrappedState::Unknown);
        let dict = info
            .to_dict(PdfVersion::V1_7)
            .expect("1.7 accepts /Trapped");
        let text = crate::core::builder::types::text_string_value;
        assert_eq!(
            dict.get("CreationDate"),
            Some(&text("D:20261029120000+08'00'"))
        );
        assert_eq!(dict.get("ModDate"), Some(&text("D:20261030093015Z")));
        assert_eq!(dict.get("Trapped"), Some(&Value::Name("Unknown".into())));
    }

    #[test]
    fn document_info_trapped_requires_pdf_1_3() {
        let info = DocumentInfo::new().trapped(TrappedState::Trapped);
        let error = info
            .to_dict(PdfVersion::V1_2)
            .expect_err("1.2 rejects /Trapped");
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "{error}"
        );
        assert!(
            info.to_dict(PdfVersion::V1_3).is_ok(),
            "1.3 accepts /Trapped"
        );
    }

    #[test]
    fn document_info_encodes_non_ascii_as_utf16_hex() {
        let info = DocumentInfo::new().title("标题");
        let dict = info.to_dict(PdfVersion::V1_7).expect("1.7 accepts /Info");
        match dict.get("Title") {
            Some(Value::HexString(hex)) => {
                assert!(hex.starts_with("FEFF"), "should have BOM: {hex}");
            }
            other => panic!("expected hex string, got {other:?}"),
        }
    }

    #[test]
    fn file_identifier_to_value_is_hex_array() {
        let id = FileIdentifier::new([0xAB; 16], [0xCD; 16]);
        let value = id.to_value();
        let array = value.as_array().expect("array");
        assert_eq!(array.len(), 2);
        match &array.0[0] {
            Value::HexString(hex) => assert_eq!(hex, &"AB".repeat(16)),
            other => panic!("expected hex string, got {other:?}"),
        }
    }

    #[test]
    fn generate_is_deterministic_and_16_bytes() {
        let a = FileIdentifier::generate(b"same material".as_slice());
        let b = FileIdentifier::generate(b"same material".as_slice());
        assert_eq!(a, b);
        assert_eq!(a.permanent, a.update);
        let c = FileIdentifier::generate(b"different".as_slice());
        assert_ne!(a.permanent, c.permanent);
    }

    #[test]
    fn permanent_matches_checks_permanent_id_only() {
        let id = FileIdentifier::new([1u8; 16], [2u8; 16]);
        assert!(id.permanent_matches(&[1u8; 16]));
        assert!(!id.permanent_matches(&[2u8; 16]));
    }
}
