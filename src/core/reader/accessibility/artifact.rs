//! 内容流 Artifact 属性表（ISO 32000-1 14.8.2.2.2、Table 330）。
//!
//! artifact 以 `/Artifact <<…>> BDC … EMC` 的形式包围，属性表（Table 330）
//! 可携带四个条目：
//!
//! - `/Type`：artifact 的类型（Pagination、Layout、Page，PDF 1.7 起还有
//!   Background）；
//! - `/BBox`：artifact 在默认用户空间中的矩形（左、下、右、上）；
//! - `/Attached`：分页或整页背景 artifact 依附的页边（Top/Bottom/Left/Right）；
//! - `/Subtype`：`/Type` 为 `Pagination` 时的细分（Header/Footer/Watermark）。
//!
//! 读端对规范中跨条目的**建议性**约束保持宽容：不要求 `Background` 必须
//! 携带 `/BBox`，不限制 `/Attached` 只出现在分页/背景 artifact 上，也不
//! 限制 `/Subtype` 只出现在 `Pagination` 上。对每个条目自身的类型与形态
//! 则严格：`/Type`、`/Subtype` 必须是名字，`/BBox` 必须恰为 4 个有限
//! 数字，`/Attached` 必须是 1–4 个互不重复的页边名字。未知名字保留原始
//! 拼写（[`ArtifactType::Other`] / [`ArtifactSubtype::Other`]），未建模的
//! 键忽略。内容流的加载器把内联数字统一解析为 [`Value::Float`]，因此
//! 数值同时接受 [`Value::Integer`] 与 [`Value::Float`]。

use std::fmt;

use crate::core::reader::content::MarkedSection;
use crate::core::types::{Dict, Value};

/// artifact 的属性类别（Table 330 的 `/Type`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactType {
    /// `Pagination`：分页 artifact，如页眉、页脚、水印。
    Pagination,
    /// `Layout`：版面 artifact，如装饰性边框或栏线。
    Layout,
    /// `Page`：页面级 artifact，如印刷厂裁切标记。
    Page,
    /// `Background`：背景 artifact（PDF 1.7）。
    Background,
    /// 其他名字，保留原始拼写。
    Other(String),
}

impl ArtifactType {
    /// 按名字映射类型；未知名字保留原始拼写。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Pagination" => Self::Pagination,
            "Layout" => Self::Layout,
            "Page" => Self::Page,
            "Background" => Self::Background,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 类型的名字；`Other` 返回原始拼写。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pagination => "Pagination",
            Self::Layout => "Layout",
            Self::Page => "Page",
            Self::Background => "Background",
            Self::Other(name) => name,
        }
    }
}

/// `/Attached` 中的一个页边（Table 330）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactAttachment {
    /// 上页边。
    Top,
    /// 下页边。
    Bottom,
    /// 左页边。
    Left,
    /// 右页边。
    Right,
}

/// 分页 artifact 的细分（Table 330 的 `/Subtype`，PDF 1.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSubtype {
    /// `Header`：页眉。
    Header,
    /// `Footer`：页脚。
    Footer,
    /// `Watermark`：水印。
    Watermark,
    /// 其他 Annex E 允许的名字，保留原始拼写。
    Other(String),
}

impl ArtifactSubtype {
    /// 按名字映射细分；未知名字保留原始拼写。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Header" => Self::Header,
            "Footer" => Self::Footer,
            "Watermark" => Self::Watermark,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 细分的名字；`Other` 返回原始拼写。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Header => "Header",
            Self::Footer => "Footer",
            Self::Watermark => "Watermark",
            Self::Other(name) => name,
        }
    }
}

/// 读取 Artifact 属性表时的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactError {
    /// 条目的类型或形态不符合 Table 330。
    Malformed(String),
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => {
                write!(f, "invalid artifact property list: {message}")
            }
        }
    }
}

impl std::error::Error for ArtifactError {}

/// 构造 [`ArtifactError::Malformed`] 的简写。
fn malformed(message: impl Into<String>) -> ArtifactError {
    ArtifactError::Malformed(message.into())
}

/// 一个 `Artifact` 标记内容属性表的类型化视图（14.8.2.2.2、Table 330）。
///
/// 四个条目均可缺省；缺省时对应字段为 `None` / 空 [`Vec`]。
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactProperties {
    /// `/Type`：artifact 类型。
    pub artifact_type: Option<ArtifactType>,
    /// `/BBox`：默认用户空间中的矩形（左、下、右、上）。
    pub bounding_box: Option<[f64; 4]>,
    /// `/Attached`：依附的页边，按属性表中的顺序保存。
    pub attached: Vec<ArtifactAttachment>,
    /// `/Subtype`：分页 artifact 的细分。
    pub subtype: Option<ArtifactSubtype>,
}

impl ArtifactProperties {
    /// 解析 `tag` 标记内容的属性表。
    ///
    /// 标签不是精确的 `Artifact`、或属性表缺失时返回 `Ok(None)`；空的
    /// Artifact 属性表返回全为缺省的 `Some`。`/Type` 与 `/Subtype` 不是
    /// 名字、`/BBox` 不是 4 个有限数字、`/Attached` 不是 1–4 个互不重复
    /// 的页边名字时返回 [`ArtifactError::Malformed`]；其余键忽略。
    pub fn parse(tag: &str, properties: Option<&Dict>) -> Result<Option<Self>, ArtifactError> {
        if tag != "Artifact" {
            return Ok(None);
        }
        let Some(properties) = properties else {
            return Ok(None);
        };
        Ok(Some(Self {
            artifact_type: type_entry(properties)?,
            bounding_box: bbox_entry(properties)?,
            attached: attached_entry(properties)?,
            subtype: subtype_entry(properties)?,
        }))
    }
}

impl MarkedSection {
    /// 按 [`ArtifactProperties::parse`] 读取本区段的 Artifact 属性表。
    ///
    /// 非 `Artifact` 区段或没有属性表时返回 `Ok(None)`；条目类型或形态
    /// 非法返回 [`ArtifactError`]。
    pub fn artifact_properties(&self) -> Result<Option<ArtifactProperties>, ArtifactError> {
        ArtifactProperties::parse(&self.tag, self.properties.as_ref())
    }
}

/// 读取 `/Type`；键缺失返回 `Ok(None)`。
fn type_entry(properties: &Dict) -> Result<Option<ArtifactType>, ArtifactError> {
    let Some(value) = properties.get("Type") else {
        return Ok(None);
    };
    match value {
        Value::Name(name) => Ok(Some(ArtifactType::from_name(name))),
        other => Err(malformed(format!(
            "property list /Type must be a name, found {}",
            other.type_name()
        ))),
    }
}

/// 读取 `/BBox`；键缺失返回 `Ok(None)`。
///
/// 必须是恰含 4 个数字的数组；数字接受整数或有限浮点数（内容流的加载器
/// 把内联数字解析为浮点数）。规范要求 `Background` artifact 必须带
/// `/BBox`，读端不强制该建议。
fn bbox_entry(properties: &Dict) -> Result<Option<[f64; 4]>, ArtifactError> {
    let Some(value) = properties.get("BBox") else {
        return Ok(None);
    };
    let Value::Array(array) = value else {
        return Err(malformed(format!(
            "property list /BBox must be an array of 4 numbers, found {}",
            value.type_name()
        )));
    };
    if array.len() != 4 {
        return Err(malformed(format!(
            "property list /BBox must contain exactly 4 numbers, found {}",
            array.len()
        )));
    }
    let mut coordinates = [0.0f64; 4];
    for (slot, value) in coordinates.iter_mut().zip(array.iter()) {
        let Some(number) = number_value(value) else {
            return Err(malformed(format!(
                "property list /BBox elements must be finite numbers, found {}",
                value.type_name()
            )));
        };
        *slot = number;
    }
    Ok(Some(coordinates))
}

/// 读取 `/Attached`；键缺失返回空 [`Vec`]。
///
/// 必须是 1–4 个名字元素，每个恰为 `Top`/`Bottom`/`Left`/`Right` 且互不
/// 重复。规范只允许分页与整页背景 artifact 使用该条目，读端不强制该建议。
fn attached_entry(properties: &Dict) -> Result<Vec<ArtifactAttachment>, ArtifactError> {
    let Some(value) = properties.get("Attached") else {
        return Ok(Vec::new());
    };
    let Value::Array(array) = value else {
        return Err(malformed(format!(
            "property list /Attached must be an array of edge names, found {}",
            value.type_name()
        )));
    };
    if array.is_empty() || array.len() > 4 {
        return Err(malformed(format!(
            "property list /Attached must contain 1 to 4 edge names, found {}",
            array.len()
        )));
    }
    let mut attached = Vec::with_capacity(array.len());
    for value in array.iter() {
        let Value::Name(name) = value else {
            return Err(malformed(format!(
                "property list /Attached elements must be names, found {}",
                value.type_name()
            )));
        };
        let Some(attachment) = attachment_from_name(name) else {
            return Err(malformed(format!(
                "property list /Attached has unknown edge name /{name}"
            )));
        };
        if attached.contains(&attachment) {
            return Err(malformed(format!(
                "property list /Attached repeats edge name /{name}"
            )));
        }
        attached.push(attachment);
    }
    Ok(attached)
}

/// 读取 `/Subtype`；键缺失返回 `Ok(None)`。
///
/// 规范建议该条目只出现在 `Pagination` artifact 上，读端不强制该建议。
fn subtype_entry(properties: &Dict) -> Result<Option<ArtifactSubtype>, ArtifactError> {
    let Some(value) = properties.get("Subtype") else {
        return Ok(None);
    };
    match value {
        Value::Name(name) => Ok(Some(ArtifactSubtype::from_name(name))),
        other => Err(malformed(format!(
            "property list /Subtype must be a name, found {}",
            other.type_name()
        ))),
    }
}

/// 把 `/Attached` 的名字映射为页边；标准名字之外返回 `None`。
fn attachment_from_name(name: &str) -> Option<ArtifactAttachment> {
    match name {
        "Top" => Some(ArtifactAttachment::Top),
        "Bottom" => Some(ArtifactAttachment::Bottom),
        "Left" => Some(ArtifactAttachment::Left),
        "Right" => Some(ArtifactAttachment::Right),
        _ => None,
    }
}

/// 把数值条目读成 `f64`；非数字或非有限浮点数返回 `None`。
///
/// 内容流的加载器把内联字典中的数字统一解析为 [`Value::Float`]，因此
/// 除 [`Value::Integer`] 外也接受 [`Value::Float`]。
fn number_value(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(number) => Some(*number as f64),
        Value::Float(number) if number.is_finite() => Some(*number),
        _ => None,
    }
}
