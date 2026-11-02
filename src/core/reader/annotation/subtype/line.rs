//! Line 与 Polygon/PolyLine 子类型专有条目（Table 175/176/178）：端点样式、
//! 意图、题注位置类型与 `/LE`、`/Vertices` 解析。

use super::super::{Result, entries::parse_number_array, malformed};
use super::MAX_VERTEX_VALUES;
use crate::core::types::{Dict, Value};

/// Line 注解端点样式（`/LE` 数组元素，Table 176）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEndStyle {
    /// 方头（`/Square`）。
    Square,
    /// 圆头（`/Circle`）。
    Circle,
    /// 菱形（`/Diamond`）。
    Diamond,
    /// 开口箭头（`/OpenArrow`）。
    OpenArrow,
    /// 闭合箭头（`/ClosedArrow`）。
    ClosedArrow,
    /// 无端点（`/None`，Table 175 缺省）。
    None,
    /// 平头（`/Butt`，PDF 1.5）。
    Butt,
    /// 末端反向开口箭头（`/ROpenArrow`，PDF 1.5）。
    ROpenArrow,
    /// 末端反向闭合箭头（`/RClosedArrow`，PDF 1.5）。
    RClosedArrow,
    /// 斜线（`/Slash`，PDF 1.6）。
    Slash,
    /// 非标准名（原样保留）。
    Other(String),
}

impl LineEndStyle {
    /// 由 `/LE` 名字解析；未知名返回 [`LineEndStyle::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Square" => Self::Square,
            "Circle" => Self::Circle,
            "Diamond" => Self::Diamond,
            "OpenArrow" => Self::OpenArrow,
            "ClosedArrow" => Self::ClosedArrow,
            "None" => Self::None,
            "Butt" => Self::Butt,
            "ROpenArrow" => Self::ROpenArrow,
            "RClosedArrow" => Self::RClosedArrow,
            "Slash" => Self::Slash,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准样式名；[`LineEndStyle::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Diamond => "Diamond",
            Self::OpenArrow => "OpenArrow",
            Self::ClosedArrow => "ClosedArrow",
            Self::None => "None",
            Self::Butt => "Butt",
            Self::ROpenArrow => "ROpenArrow",
            Self::RClosedArrow => "RClosedArrow",
            Self::Slash => "Slash",
            Self::Other(other) => other,
        }
    }
}

/// Line 注解意图（`/IT`，Table 175，PDF 1.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineIntent {
    /// 带箭头装饰的线（`/LineArrow`，缺省）。
    LineArrow,
    /// 线性标注，题注给出测量长度（`/LineDimension`）。
    LineDimension,
    /// 非标准名（原样保留）。
    Other(String),
}

impl LineIntent {
    /// 由 `/IT` 名字解析；未知名返回 [`LineIntent::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "LineArrow" => Self::LineArrow,
            "LineDimension" => Self::LineDimension,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准意图名；[`LineIntent::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::LineArrow => "LineArrow",
            Self::LineDimension => "LineDimension",
            Self::Other(other) => other,
        }
    }
}

/// Line 注解题注位置（`/CP`，仅 `/Cap` 为真时有意义，PDF 1.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptionPosition {
    /// 题注内联于连线延长线（`/Inline`，缺省）。
    Inline,
    /// 题注居中置于连线上方（`/Top`）。
    Top,
    /// 非标准名（原样保留）。
    Other(String),
}

impl CaptionPosition {
    /// 由 `/CP` 名字解析；未知名返回 [`CaptionPosition::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Inline" => Self::Inline,
            "Top" => Self::Top,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准位置名；[`CaptionPosition::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Inline => "Inline",
            Self::Top => "Top",
            Self::Other(other) => other,
        }
    }
}

/// Polygon/PolyLine 注解意图（`/IT`，Table 178）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolygonIntent {
    /// 云状多边形（`/PolygonCloud`，PDF 1.6）。
    PolygonCloud,
    /// 折线标注，题注给出测量长度（`/PolyLineDimension`，PDF 1.7）。
    PolyLineDimension,
    /// 多边形标注，题注给出测量长度（`/PolygonDimension`，PDF 1.7）。
    PolygonDimension,
    /// 非标准名（原样保留）。
    Other(String),
}

impl PolygonIntent {
    /// 由 `/IT` 名字解析；未知名返回 [`PolygonIntent::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "PolygonCloud" => Self::PolygonCloud,
            "PolyLineDimension" => Self::PolyLineDimension,
            "PolygonDimension" => Self::PolygonDimension,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准意图名；[`PolygonIntent::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::PolygonCloud => "PolygonCloud",
            Self::PolyLineDimension => "PolyLineDimension",
            Self::PolygonDimension => "PolygonDimension",
            Self::Other(other) => other,
        }
    }
}

/// Polygon/PolyLine `/Vertices`：x/y 交替的偶数个有限数值（至少两个顶点）。
pub(super) fn parse_vertices(dict: &Dict) -> Result<Option<Vec<f64>>> {
    let Some(values) = parse_number_array(dict, "Vertices", false)? else {
        return Ok(None);
    };
    if values.len() % 2 != 0 || values.len() < 4 {
        return Err(malformed(
            "annotation /Vertices must hold an even number of values for at least two vertices",
        ));
    }
    if values.len() > MAX_VERTEX_VALUES {
        return Err(malformed("annotation /Vertices has too many values"));
    }
    Ok(Some(values))
}

/// FreeText 的单名字 `/LE`（引线端点样式，Table 174）；数组形态归 Line。
pub(super) fn parse_bare_line_ending(dict: &Dict) -> Result<Option<LineEndStyle>> {
    let Some(name) = dict.get("LE").and_then(Value::as_name) else {
        return Ok(None);
    };
    Ok(Some(LineEndStyle::from_name(name)))
}

/// Line/Polygon/PolyLine `/LE`：一到两个端点样式名，缺省的一端按 `/None` 处理；
/// 单个名字的 FreeText 形态由 [`parse_bare_line_ending`] 承接，此处返回 `None`。
pub(super) fn parse_line_endings(dict: &Dict) -> Result<Option<[LineEndStyle; 2]>> {
    let Some(value) = dict.get("LE") else {
        return Ok(None);
    };
    if value.as_name().is_some() {
        return Ok(None);
    }
    let array = value
        .as_array()
        .ok_or_else(|| malformed("annotation /LE is not an array"))?;
    if array.0.is_empty() || array.0.len() > 2 {
        return Err(malformed("annotation /LE must hold one or two names"));
    }
    let mut endings = [LineEndStyle::None, LineEndStyle::None];
    for (slot, item) in endings.iter_mut().zip(array.0.iter()) {
        let name = item
            .as_name()
            .ok_or_else(|| malformed("annotation /LE entries must be names"))?;
        *slot = LineEndStyle::from_name(name);
    }
    Ok(Some(endings))
}
