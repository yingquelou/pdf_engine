//! 简单字体 `/Encoding` 的写端配置。

use crate::core::reader::font::{
    PredefinedEncoding, code_for_char, default_encoding, predefined_encoding,
};
use crate::core::types::{Array, Dict, Value};
use std::collections::BTreeMap;

/// 简单字体 `/Encoding` 的写端配置。
///
/// 默认取 [`FontEncoding::Named`] 的 `WinAnsiEncoding`，与既有输出一致。
/// [`FontEncoding::BuiltIn`] 省略 `/Encoding`，让字体程序的内建编码生效；
/// [`FontEncoding::Custom`] 写入 `/BaseEncoding` 与 `/Differences` 覆盖。
///
/// `text` 会按此配置把文本转成单字节字符码：五个预定义编码名可识别，
/// 其余名称无法转码时退回原始 UTF-8 字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontEncoding {
    /// 省略 `/Encoding`（使用字体内建编码，如 Standard 14 的 Standard）。
    BuiltIn,
    /// 预定义编码名（不含前导 `/`），如 `MacRomanEncoding`。
    Named(String),
    /// 编码字典。
    Custom {
        /// `/BaseEncoding` 名（不含前导 `/`）；`None` 表示省略。
        base: Option<String>,
        /// 字符码 → glyph 名覆盖项，写出时按字符码升序排列。
        differences: BTreeMap<u8, String>,
    },
}

impl Default for FontEncoding {
    fn default() -> Self {
        Self::Named("WinAnsiEncoding".to_string())
    }
}

impl FontEncoding {
    /// 指定预定义编码名。
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// 带 `/Differences` 的编码字典；`base` 为 `None` 时省略 `/BaseEncoding`。
    ///
    /// 同一字符码重复出现时，后定义者覆盖先定义者。
    pub fn with_differences(base: Option<&str>, differences: Vec<(u8, String)>) -> Self {
        Self::Custom {
            base: base.map(str::to_string),
            differences: differences.into_iter().collect(),
        }
    }

    /// 解析基础预定义编码；编码名无法识别或 `/BaseEncoding` 无效时返回 `None`。
    pub(super) fn base_encoding(&self, base_font: &str) -> Option<PredefinedEncoding> {
        match self {
            Self::BuiltIn => Some(default_encoding(base_font)),
            Self::Named(name) => predefined_encoding(name).ok(),
            Self::Custom {
                base: Some(name), ..
            } => predefined_encoding(name).ok(),
            Self::Custom { base: None, .. } => Some(default_encoding(base_font)),
        }
    }

    /// 把文本按编码转成单字节字符码。
    ///
    /// 无法表示的字符替换为 `?`（0x3F）；基础编码无法识别时返回 `None`，
    /// 调用方回退为原始 UTF-8 字节。
    pub(crate) fn encode_text(&self, base_font: &str, text: &str) -> Option<Vec<u8>> {
        let base = self.base_encoding(base_font)?;
        let empty = BTreeMap::new();
        let differences = match self {
            Self::Custom { differences, .. } => differences,
            _ => &empty,
        };
        Some(
            text.chars()
                .map(|ch| code_for_char(base, differences, ch).unwrap_or(b'?'))
                .collect(),
        )
    }

    /// 转换为字体字典的 `/Encoding` 值；`BuiltIn` 无对应值。
    pub(crate) fn to_value(&self) -> Option<Value> {
        match self {
            Self::BuiltIn => None,
            Self::Named(name) => Some(Value::Name(name.clone())),
            Self::Custom { base, differences } => {
                let mut dict = Dict::new();
                if let Some(base) = base {
                    dict = dict.insert("BaseEncoding", Value::Name(base.clone()));
                }
                if !differences.is_empty() {
                    let mut arr = Array::new();
                    for (code, name) in differences {
                        arr = arr
                            .push(Value::Integer(i64::from(*code)))
                            .push(Value::Name(name.clone()));
                    }
                    dict = dict.insert("Differences", Value::Array(arr));
                }
                Some(Value::Dict(dict))
            }
        }
    }
}
