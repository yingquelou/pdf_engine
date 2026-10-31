//! Span 标记内容属性表（`/Alt` `/ActualText` `/E` `/Lang`）的逻辑配置。
//!
//! 对应 ISO 32000-1 14.9.3–14.9.5 与 14.9.2.1：`Span` 是标记内容序列的
//! 标准标签，供辅助技术以类型化条目描述一段文本。构建器把属性表写成
//! `BDC` 的内联字典（`/Span << ... >> BDC`）。
//!
//! 版本约束：`/Lang` 与 `/E` 自 PDF 1.4、`/Alt` 与 `/ActualText` 自
//! PDF 1.5 引入；门禁在组装阶段完成（见 [`PdfBuilder::begin_span`]）。
//!
//! [`PdfBuilder::begin_span`]: super::super::PdfBuilder::begin_span

use crate::core::reader::accessibility::LanguageIdentifier;
use crate::core::types::Dict;

use super::optional_content::text_string_value;

/// Span 标记内容属性表（ISO 32000-1 14.9.3–14.9.5 与 14.9.2.1）。
///
/// 全部条目可选；空属性表仍可写出（`/Span << >> BDC`），只是不携带
/// 无障碍语义。文本条目写出时统一编码为 PDF 文本串（PDFDocEncoding 或
/// UTF-16BE 的大写十六进制形式）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpanProperties {
    /// `/Alt`：替代描述（14.9.3），PDF 1.5。
    alternate_text: Option<String>,
    /// `/ActualText`：替换文本（14.9.4），PDF 1.5。
    actual_text: Option<String>,
    /// `/E`：缩写展开（14.9.5），PDF 1.4。
    expansion: Option<String>,
    /// `/Lang`：这段内容的自然语言（14.9.2.1），PDF 1.4。
    language: Option<LanguageIdentifier>,
}

impl SpanProperties {
    /// 新建空属性表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `/Alt` 替代描述（14.9.3，PDF 1.5 引入）。
    pub fn with_alternate_text(mut self, text: impl Into<String>) -> Self {
        self.alternate_text = Some(text.into());
        self
    }

    /// 设置 `/ActualText` 替换文本（14.9.4，PDF 1.5 引入）。
    pub fn with_actual_text(mut self, text: impl Into<String>) -> Self {
        self.actual_text = Some(text.into());
        self
    }

    /// 设置 `/E` 缩写展开（14.9.5，PDF 1.4 引入）。
    pub fn with_expansion(mut self, text: impl Into<String>) -> Self {
        self.expansion = Some(text.into());
        self
    }

    /// 设置 `/Lang` 自然语言（14.9.2.1，PDF 1.4 引入）。
    pub fn with_language(mut self, language: LanguageIdentifier) -> Self {
        self.language = Some(language);
        self
    }

    /// 属性表是否不含任何条目。
    pub fn is_empty(&self) -> bool {
        self.alternate_text.is_none()
            && self.actual_text.is_none()
            && self.expansion.is_none()
            && self.language.is_none()
    }

    /// 构造内联属性字典；插入顺序为 `/Alt` `/ActualText` `/E` `/Lang`，
    /// 文本条目统一用 PDF 文本串（大写十六进制）写出。
    pub fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(text) = &self.alternate_text {
            dict = dict.insert("Alt", text_string_value(text));
        }
        if let Some(text) = &self.actual_text {
            dict = dict.insert("ActualText", text_string_value(text));
        }
        if let Some(text) = &self.expansion {
            dict = dict.insert("E", text_string_value(text));
        }
        if let Some(language) = &self.language {
            dict = dict.insert("Lang", text_string_value(language.as_str()));
        }
        dict
    }
}
