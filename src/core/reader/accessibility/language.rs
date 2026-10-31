//! 自然语言标识符（ISO 32000-1 14.9.2.2，RFC 3066）。

use std::fmt;
use std::hash::{Hash, Hasher};

use crate::core::reader::content::text_value;
use crate::core::types::Value;

/// 自然语言读取的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum LanguageError {
    /// 文本串无法解码、语言标识不符合语法或多语言数组形态非法。
    Malformed(String),
    /// 超出多语言文本数组条目上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for LanguageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "invalid natural language: {message}"),
            Self::LimitExceeded(what) => {
                write!(f, "natural language limit exceeded: {what}")
            }
        }
    }
}

impl std::error::Error for LanguageError {}

/// 构造 [`LanguageError::Malformed`] 的简写。
pub(super) fn malformed(message: impl Into<String>) -> LanguageError {
    LanguageError::Malformed(message.into())
}

/// 已校验的自然语言标识符（RFC 3066 Language-Tag）。
///
/// 空串表示语言未知（14.9.2.2），是合法的显式声明。比较与哈希均
/// 大小写不敏感；[`as_str`](Self::as_str) 与 `Display` 保留原始拼写。
#[derive(Debug, Clone)]
pub struct LanguageIdentifier {
    /// 调用方传入的原始文本串。
    value: String,
}

impl LanguageIdentifier {
    /// 解析语言标识符。
    ///
    /// 空串合法（未知语言）；非空时按 `-` 分隔：主语言子标签为 1–8 个
    /// ASCII 字母，后续子标签为 1–8 个 ASCII 字母或数字。按 RFC 3066 的
    /// ABNF 校验，不做注册表校验，也不做空白裁剪。
    pub fn parse(value: &str) -> Result<Self, LanguageError> {
        if value.is_empty() {
            return Ok(Self {
                value: String::new(),
            });
        }
        let mut subtags = value.split('-');
        let primary = subtags.next().unwrap_or_default();
        if primary.is_empty()
            || primary.len() > 8
            || !primary.bytes().all(|byte| byte.is_ascii_alphabetic())
        {
            return Err(malformed(format!(
                "primary language subtag {primary:?} must be 1 to 8 ASCII letters"
            )));
        }
        for subtag in subtags {
            if subtag.is_empty()
                || subtag.len() > 8
                || !subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
            {
                return Err(malformed(format!(
                    "subtag {subtag:?} must be 1 to 8 ASCII letters or digits"
                )));
            }
        }
        Ok(Self {
            value: value.to_owned(),
        })
    }

    /// 语言未知（空标识符）。
    pub fn unknown() -> Self {
        Self {
            value: String::new(),
        }
    }

    /// 是否为表示未知语言的空标识符。
    pub fn is_unknown(&self) -> bool {
        self.value.is_empty()
    }

    /// 原始文本串（保留大小写与拼写）。
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl PartialEq for LanguageIdentifier {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq_ignore_ascii_case(&other.value)
    }
}

impl Eq for LanguageIdentifier {}

impl Hash for LanguageIdentifier {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // 与 `PartialEq` 一致：统一按小写 ASCII 求哈希。
        self.value.to_ascii_lowercase().hash(state);
    }
}

impl fmt::Display for LanguageIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

/// 解码 PDF 文本串（PDFDocEncoding/UTF-16BE）；名字等非文本串返回 `None`。
///
/// `content::text_value` 自身也接受 `/Name`，但 14.9.2 的各 `/Lang` 与
/// 多语言文本条目只允许文本串，故此处先收紧类型。
pub(super) fn decode_text_string(value: &Value) -> Option<String> {
    match value {
        Value::String(_) | Value::HexString(_) => text_value(value),
        _ => None,
    }
}
