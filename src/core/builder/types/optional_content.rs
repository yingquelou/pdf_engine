//! 可选内容组（`/OCG`）的逻辑配置。
//!
//! 对应 ISO 32000-1 §8.11.2.3 Table 99 与 §8.11.4 Table 101。构建器把组
//! 写成文档级 `/OCG` 字典，在目录 `/OCProperties` 的默认配置中登记初始
//! 可见性，页面内容用 `/OC /OC<n> BDC ... EMC` 标记。

use crate::core::types::{Array, Dict, PdfTextString, Value};

/// 可选内容组（Table 99）。
///
/// 通过 [`PdfBuilder::optional_content_group`] 注册取得句柄，再用
/// [`PdfBuilder::begin_optional_content`] / [`PdfBuilder::end_optional_content`]
/// 在页面上标记内容。只有实际标记过的组才会写出 `/OCG` 对象与目录配置。
///
/// [`PdfBuilder::optional_content_group`]: super::super::PdfBuilder::optional_content_group
/// [`PdfBuilder::begin_optional_content`]: super::super::PdfBuilder::begin_optional_content
/// [`PdfBuilder::end_optional_content`]: super::super::PdfBuilder::end_optional_content
#[derive(Debug, Clone, PartialEq)]
pub struct OptionalContentGroup {
    /// `/Name`：用户界面显示名（任意 Unicode 文本）。
    pub name: String,
    /// 默认配置中的初始可见性：`true` 写入 `/ON`，`false` 写入 `/OFF`。
    pub default_visible: bool,
    /// `/Intent` 用途名称；空表示规范默认 `/View`，写出时省略。
    pub intent: Vec<String>,
}

impl OptionalContentGroup {
    /// 构造默认可见、无 `/Intent` 的组。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default_visible: true,
            intent: Vec::new(),
        }
    }

    /// 设置默认配置中的初始可见性。
    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.default_visible = visible;
        self
    }

    /// 设置单个 `/Intent` 用途名称。
    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = vec![intent.into()];
        self
    }

    /// 设置多个 `/Intent` 用途名称；空列表等价于省略 `/Intent`。
    pub fn with_intents(mut self, intents: &[&str]) -> Self {
        self.intent = intents.iter().map(|intent| (*intent).to_string()).collect();
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("group name must not be empty".into());
        }
        for intent in &self.intent {
            if !is_valid_pdf_name(intent) {
                return Err(format!("/Intent {intent:?} must be a valid PDF name"));
            }
        }
        Ok(())
    }

    /// 校验并构造 `/OCG` 字典（Table 99）。
    pub(crate) fn to_dict(&self) -> Result<Dict, String> {
        self.validate()?;
        let mut dict = Dict::new()
            .insert("Type", Value::Name("OCG".into()))
            .insert("Name", text_string_value(&self.name));
        if !self.intent.is_empty() {
            let value = if self.intent.len() == 1 {
                Value::Name(self.intent[0].clone())
            } else {
                Value::Array(self.intent.iter().fold(Array::new(), |array, intent| {
                    array.push(Value::Name(intent.clone()))
                }))
            };
            dict = dict.insert("Intent", value);
        }
        Ok(dict)
    }
}

/// 文本字符串统一用 UTF-16BE/PDFDocEncoding 编码后的大写十六进制写出，
/// 避免非 ASCII 名称按 UTF-8 直写违反规范。
pub(crate) fn text_string_value(text: &str) -> Value {
    let bytes = PdfTextString::from(text).encode();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    Value::HexString(hex)
}

fn is_valid_pdf_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            (0x21..=0x7E).contains(&byte)
                && !matches!(
                    byte,
                    b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' | b'#'
                )
        })
}
