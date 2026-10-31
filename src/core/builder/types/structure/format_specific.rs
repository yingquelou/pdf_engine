//! 格式专用属性所有者（ISO 32000-1 14.8.5.2 Table 341）。
//!
//! 提供 XML-1.00、HTML-3.20、HTML-4.01、OEB-1.00、RTF-1.05、CSS-1.00、
//! CSS-2.00 七种格式专用所有者的属性对象构造器，经
//! [`StructureAttribute::format_specific`] 生成 `/O` 为该所有者的属性
//! 字典。Table 341 的条目集合是开放式的（规范只列出常见条目并允许扩展），
//! 因此写端不校验条目类型，调用方通过 [`FormatSpecificAttributes::with_entry`]
//! 自由补充；条目值原样写出。
//!
//! `/O` 由构造器最后写入，调用方 `entries` 中同名的 `O` 会被覆盖，保证
//! 属性对象所有者始终正确。版本门禁由 `builder/navigation/structure_standard.rs`
//! 统一按 PDF 1.4 检查（§14.8.5.2 标准结构属性）。

use crate::core::types::{Dict, Value};

use super::StructureAttribute;

/// 格式专用属性所有者（Table 341 `/O`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatSpecificOwner {
    /// `/XML-1.00`：XML 1.00 格式专用属性。
    Xml1_00,
    /// `/HTML-3.20`：HTML 3.20 格式专用属性。
    Html3_20,
    /// `/HTML-4.01`：HTML 4.01 格式专用属性。
    Html4_01,
    /// `/OEB-1.00`：OEB 1.00 格式专用属性。
    Oeb1_00,
    /// `/RTF-1.05`：RTF 1.05 格式专用属性。
    Rtf1_05,
    /// `/CSS-1.00`：CSS 1.00 格式专用属性。
    Css1_00,
    /// `/CSS-2.00`：CSS 2.00 格式专用属性。
    Css2_00,
}

impl FormatSpecificOwner {
    /// `/O` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &'static str {
        match self {
            Self::Xml1_00 => "XML-1.00",
            Self::Html3_20 => "HTML-3.20",
            Self::Html4_01 => "HTML-4.01",
            Self::Oeb1_00 => "OEB-1.00",
            Self::Rtf1_05 => "RTF-1.05",
            Self::Css1_00 => "CSS-1.00",
            Self::Css2_00 => "CSS-2.00",
        }
    }

    /// 由 `/O` 名字识别格式专用所有者。
    ///
    /// 校验器以此作为名字表的唯一来源，避免与 [`Self::name`] 漂移。
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "XML-1.00" => Some(Self::Xml1_00),
            "HTML-3.20" => Some(Self::Html3_20),
            "HTML-4.01" => Some(Self::Html4_01),
            "OEB-1.00" => Some(Self::Oeb1_00),
            "RTF-1.05" => Some(Self::Rtf1_05),
            "CSS-1.00" => Some(Self::Css1_00),
            "CSS-2.00" => Some(Self::Css2_00),
            _ => None,
        }
    }
}

/// 格式专用标准属性（Table 341，`/O` 为七种格式专用所有者之一）。
///
/// 条目集合开放式：除 `/O` 外不预置任何条目，值形态交由调用方决定。
#[derive(Debug, Clone, PartialEq)]
pub struct FormatSpecificAttributes {
    /// `/O`：格式专用所有者。
    pub owner: FormatSpecificOwner,
    /// 其余条目（`/O` 之外的键值），按字典顺序写出。
    pub entries: Dict,
}

impl FormatSpecificAttributes {
    /// 指定所有者的空属性集合。
    pub fn new(owner: FormatSpecificOwner) -> Self {
        Self {
            owner,
            entries: Dict::new(),
        }
    }

    /// 追加一个条目；同名条目后写覆盖先写。
    pub fn with_entry(mut self, key: impl Into<String>, value: Value) -> Self {
        self.entries.0.insert(key.into(), value);
        self
    }
}

impl StructureAttribute {
    /// 格式专用属性对象（Table 341）。
    ///
    /// `/O` 在最后写入并覆盖 `attributes.entries` 中可能存在的同名键，
    /// 保证所有者恒为 [`FormatSpecificAttributes::owner`]。
    pub fn format_specific(attributes: FormatSpecificAttributes) -> Self {
        let mut entries = attributes.entries.clone();
        entries.0.insert(
            "O".to_owned(),
            Value::Name(attributes.owner.name().to_owned()),
        );
        Self::new(Value::Dict(entries))
    }
}
