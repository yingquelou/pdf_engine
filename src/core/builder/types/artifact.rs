//! Artifact 标记内容属性表（`/Type` `/BBox` `/Attached` `/Subtype`）的逻辑配置。
//!
//! 对应 ISO 32000-1 14.8.2.2.2 与 Table 330：`Artifact` 标签用于标记
//! 不属于页面真实内容的装饰元素（页眉页脚、水印、版面分隔线、背景
//! 图形等）。构建器把属性表写成 `BDC` 的内联字典
//! （`/Artifact << ... >> BDC`）。
//!
//! 条目约束（Table 330）：
//! - `/Type`：可选，取 `Pagination` `Layout` `Page`，PDF 1.7 起另有
//!   `Background`；
//! - `/BBox`：可选矩形（`left bottom right top`），背景 Artifact 必填；
//! - `/Attached`：可选名字数组，仅用于分页或背景 Artifact，1–4 个
//!   `Top` `Bottom` `Left` `Right`，不得重复；
//! - `/Subtype`：可选（PDF 1.7），仅当 `/Type` 为 `Pagination`；标准名
//!   `Header` `Footer` `Watermark` 之外允许 Annex E 合规的自定义名。
//!
//! 版本约束：Artifact 属性表自 PDF 1.4 引入，`Background` 与 `/Subtype`
//! 自 PDF 1.7 引入；门禁在组装阶段完成（见
//! [`PdfBuilder::begin_artifact`]）。
//!
//! [`PdfBuilder::begin_artifact`]: super::super::PdfBuilder::begin_artifact

use std::collections::BTreeSet;

use crate::core::types::{Array, Dict, Value};

/// Artifact 的 `/Type`（Table 330）。
///
/// 写端只接受这四个标准名字；未知名字在 [`validate_artifact_dict`] 中
/// 被拒绝。`Background` 自 PDF 1.7 引入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactType {
    /// `/Pagination`：分页伪影（页眉、页脚、页码等）。
    Pagination,
    /// `/Layout`：版面伪影（分隔线、装饰框等）。
    Layout,
    /// `/Page`：页面伪影（裁切标记、套准标记等）。
    Page,
    /// `/Background`：背景伪影（PDF 1.7 引入，必须带 `/BBox`）。
    Background,
}

impl ArtifactType {
    /// `/Type` 的名字形式。
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::Pagination => "Pagination",
            Self::Layout => "Layout",
            Self::Page => "Page",
            Self::Background => "Background",
        }
    }
}

/// `/Attached` 数组元素（Table 330）：伪影附着于页面的哪条边。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactAttachment {
    /// `/Top`：上边。
    Top,
    /// `/Bottom`：下边。
    Bottom,
    /// `/Left`：左边。
    Left,
    /// `/Right`：右边。
    Right,
}

impl ArtifactAttachment {
    /// `/Attached` 数组元素的名字形式。
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

/// Artifact 的 `/Subtype`（Table 330，PDF 1.7）。
///
/// 仅当 `/Type` 为 `Pagination` 时允许；标准名之外可以用
/// [`ArtifactSubtype::Other`] 给出 Annex E 合规的自定义名字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactSubtype {
    /// `/Header`：页眉。
    Header,
    /// `/Footer`：页脚。
    Footer,
    /// `/Watermark`：水印。
    Watermark,
    /// 自定义子类型名（须为合法 PDF 名字）。
    Other(String),
}

impl ArtifactSubtype {
    /// `/Subtype` 的名字形式。
    pub fn as_name(&self) -> &str {
        match self {
            Self::Header => "Header",
            Self::Footer => "Footer",
            Self::Watermark => "Watermark",
            Self::Other(name) => name,
        }
    }
}

/// Artifact 标记内容属性表（ISO 32000-1 14.8.2.2.2 与 Table 330）。
///
/// `/Type` 必填，其余条目可选。构建器把属性表写成 `BDC` 的内联字典；
/// `Type` 与 `Subtype` 写成名字，`BBox` 写成四个浮点数，`Attached`
/// 按调用顺序写成名字数组（重复的附着边只保留首次出现）。
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactProperties {
    artifact_type: ArtifactType,
    bounding_box: Option<[f64; 4]>,
    attached: Vec<ArtifactAttachment>,
    subtype: Option<ArtifactSubtype>,
}

impl ArtifactProperties {
    /// 以必填的 `/Type` 新建属性表。
    pub fn new(artifact_type: ArtifactType) -> Self {
        Self {
            artifact_type,
            bounding_box: None,
            attached: Vec::new(),
            subtype: None,
        }
    }

    /// 设置 `/BBox` 矩形（`left bottom right top`）；背景 Artifact 必填。
    pub fn with_bounding_box(mut self, bounding_box: [f64; 4]) -> Self {
        self.bounding_box = Some(bounding_box);
        self
    }

    /// 追加一个 `/Attached` 附着边，保持调用顺序；重复的边被忽略。
    pub fn with_attachment(mut self, attachment: ArtifactAttachment) -> Self {
        if !self.attached.contains(&attachment) {
            self.attached.push(attachment);
        }
        self
    }

    /// 设置 `/Subtype`（仅 `Pagination` 允许，PDF 1.7）。
    pub fn with_subtype(mut self, subtype: ArtifactSubtype) -> Self {
        self.subtype = Some(subtype);
        self
    }

    /// 构造内联属性字典；键按字典的 `BTreeMap` 顺序写出
    /// （`Attached` `BBox` `Subtype` `Type`）。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert(
            "Type",
            Value::Name(self.artifact_type.as_name().to_string()),
        );
        if let Some(bounding_box) = self.bounding_box {
            dict = dict.insert(
                "BBox",
                Value::Array(Array(
                    bounding_box
                        .into_iter()
                        .map(Value::Float)
                        .collect::<Vec<_>>(),
                )),
            );
        }
        if !self.attached.is_empty() {
            dict = dict.insert(
                "Attached",
                Value::Array(Array(
                    self.attached
                        .iter()
                        .map(|attachment| Value::Name(attachment.as_name().to_string()))
                        .collect::<Vec<_>>(),
                )),
            );
        }
        if let Some(subtype) = &self.subtype {
            dict = dict.insert("Subtype", Value::Name(subtype.as_name().to_string()));
        }
        dict
    }
}

/// 原始 Artifact 属性字典的校验结果。
///
/// `pdf17` 在 `/Type` 为 `Background` 或出现 `/Subtype` 时为 `true`；
/// `background` / `subtype` 分别记录这两个 PDF 1.7 条目的出现情况，
/// 供组装阶段聚合特征画像。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ArtifactFlags {
    /// 是否含 PDF 1.7 才引入的条目。
    pub(crate) pdf17: bool,
    /// 是否为背景 Artifact。
    pub(crate) background: bool,
    /// 是否带 `/Subtype`。
    pub(crate) subtype: bool,
}

/// 校验一段内容流中的原始 `Artifact` 属性字典（Table 330）。
///
/// 供组装阶段检查用户通过通用 `begin_marked_content_properties` 注入的
/// 属性表，保证写出的 Artifact 语法符合 Table 330：
/// - `/Type` 若出现必须是四个已知名字之一；
/// - `/Subtype` 若出现必须是合法 PDF 名字，且仅当 `/Type` 为
///   `Pagination`；
/// - `/BBox` 若出现必须是恰好四个有限数字，背景 Artifact 必填；
/// - `/Attached` 若出现必须是 1–4 个 `Top` `Bottom` `Left` `Right`，
///   不得重复，且仅用于分页或背景 Artifact。
pub(crate) fn validate_artifact_dict(dict: &Dict) -> Result<ArtifactFlags, String> {
    let artifact_type = match dict.get("Type") {
        Some(Value::Name(name)) => match name.as_str() {
            "Pagination" => Some(ArtifactType::Pagination),
            "Layout" => Some(ArtifactType::Layout),
            "Page" => Some(ArtifactType::Page),
            "Background" => Some(ArtifactType::Background),
            other => {
                return Err(format!(
                    "Artifact /Type must be one of Pagination, Layout, Page or Background, got {other:?}"
                ));
            }
        },
        Some(other) => {
            return Err(format!(
                "Artifact /Type must be a name, found {}",
                other.type_name()
            ));
        }
        None => None,
    };

    let background = artifact_type == Some(ArtifactType::Background);

    if let Some(value) = dict.get("Subtype") {
        let Value::Name(name) = value else {
            return Err(format!(
                "Artifact /Subtype must be a name, found {}",
                value.type_name()
            ));
        };
        if !is_valid_pdf_name(name) {
            return Err(format!(
                "Artifact /Subtype {name:?} is not a valid PDF name"
            ));
        }
        if !matches!(artifact_type, Some(ArtifactType::Pagination)) {
            return Err("Artifact /Subtype is only allowed when /Type is Pagination".to_string());
        }
    }

    match dict.get("BBox") {
        Some(Value::Array(values)) => {
            if values.0.len() != 4 {
                return Err(format!(
                    "Artifact /BBox must contain exactly 4 numbers, got {}",
                    values.0.len()
                ));
            }
            for value in &values.0 {
                match value {
                    Value::Integer(_) => {}
                    Value::Float(number) if number.is_finite() => {}
                    other => {
                        return Err(format!(
                            "Artifact /BBox entries must be finite numbers, found {}",
                            other.type_name()
                        ));
                    }
                }
            }
        }
        Some(other) => {
            return Err(format!(
                "Artifact /BBox must be an array of 4 numbers, found {}",
                other.type_name()
            ));
        }
        None if background => {
            return Err("Artifact Background requires /BBox".to_string());
        }
        None => {}
    }

    if let Some(value) = dict.get("Attached") {
        if !matches!(
            artifact_type,
            Some(ArtifactType::Pagination | ArtifactType::Background)
        ) {
            return Err(
                "Artifact /Attached is only allowed for Pagination or Background artifacts"
                    .to_string(),
            );
        }
        let Value::Array(values) = value else {
            return Err(format!(
                "Artifact /Attached must be an array of 1 to 4 names, found {}",
                value.type_name()
            ));
        };
        if values.0.is_empty() || values.0.len() > 4 {
            return Err(format!(
                "Artifact /Attached must contain 1 to 4 names, got {}",
                values.0.len()
            ));
        }
        let mut seen = BTreeSet::new();
        for value in &values.0 {
            let Value::Name(name) = value else {
                return Err(format!(
                    "Artifact /Attached entries must be names, found {}",
                    value.type_name()
                ));
            };
            if !matches!(name.as_str(), "Top" | "Bottom" | "Left" | "Right") {
                return Err(format!(
                    "Artifact /Attached name must be one of Top, Bottom, Left or Right, got {name:?}"
                ));
            }
            if !seen.insert(name.as_str()) {
                return Err(format!("Artifact /Attached must not repeat {name:?}"));
            }
        }
    }

    let subtype = dict.get("Subtype").is_some();
    Ok(ArtifactFlags {
        pdf17: background || subtype,
        background,
        subtype,
    })
}

/// 是否为合法的 PDF 名字（Annex E）。
///
/// 名字非空，且每个字节都是可打印 ASCII（`!`–`~`）并避开定界符
/// `()<>[]{}/%` 与转义引导符 `#`。
fn is_valid_pdf_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| (b'!'..=b'~').contains(&byte) && !b"()<>[]{}/%#".contains(&byte))
}
