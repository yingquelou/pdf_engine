//! 3D 渲染模式字典（Table 307/308）。
//!
//! `/Subtype` 必填，合法值为 `Solid` `SolidWireframe` `Transparent`
//! `TransparentWireframe` `BoundingBox` `TransparentBoundingBox`
//! `TransparentBoundingBoxOutline` `Wireframe` `ShadedWireframe`
//! `HiddenWireframe` `Vertices` `ShadedVertices` `Illustration` `SolidOutline`
//! `ShadedIllustration`；其他名字保留为 [`ThreeDRenderModeType::Other`]，规范
//! 要求消费者忽略未知渲染模式。`/AC` `/FC` `/O` `/CV` 的缺省值
//! （`[/DeviceRGB 0 0 0]`、`BG`、0.5、45）不物化；读端不额外钳制 `/O` `/CV`
//! 的取值，只要求有限数值。

use crate::core::types::{Dict, Document, Value};

use super::color::{ThreeDColor, parse_color, rgb_array};
use super::{ThreeDError, check_type, malformed, optional_name, optional_number, resolve_value};

/// 渲染模式子类型（`/Subtype`，Table 308）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDRenderModeType {
    /// `Solid`：带纹理与光照的几何面板（`/AC` 被忽略）。
    Solid,
    /// `SolidWireframe`：实体加线框。
    SolidWireframe,
    /// `Transparent`：无阴影的半透明面。
    Transparent,
    /// `TransparentWireframe`：透明面加线框。
    TransparentWireframe,
    /// `BoundingBox`：包围盒线框。
    BoundingBox,
    /// `TransparentBoundingBox`：半透明包围盒面。
    TransparentBoundingBox,
    /// `TransparentBoundingBoxOutline`：半透明包围盒面加轮廓。
    TransparentBoundingBoxOutline,
    /// `Wireframe`：面边线框。
    Wireframe,
    /// `ShadedWireframe`：线框加逐顶点着色。
    ShadedWireframe,
    /// `HiddenWireframe`：隐藏背面边的线框。
    HiddenWireframe,
    /// `Vertices`：仅顶点。
    Vertices,
    /// `ShadedVertices`：仅顶点加着色。
    ShadedVertices,
    /// `Illustration`：插画式着色。
    Illustration,
    /// `SolidOutline`：实体加轮廓。
    SolidOutline,
    /// `ShadedIllustration`：插画式着色加轮廓。
    ShadedIllustration,
    /// 未识别的渲染模式（原样保留；规范要求忽略）。
    Other(String),
}

impl ThreeDRenderModeType {
    /// 按 `/Subtype` 名字构造；未识别名字落 `Other`。
    pub(crate) fn from_name(name: &str) -> Self {
        match name {
            "Solid" => Self::Solid,
            "SolidWireframe" => Self::SolidWireframe,
            "Transparent" => Self::Transparent,
            "TransparentWireframe" => Self::TransparentWireframe,
            "BoundingBox" => Self::BoundingBox,
            "TransparentBoundingBox" => Self::TransparentBoundingBox,
            "TransparentBoundingBoxOutline" => Self::TransparentBoundingBoxOutline,
            "Wireframe" => Self::Wireframe,
            "ShadedWireframe" => Self::ShadedWireframe,
            "HiddenWireframe" => Self::HiddenWireframe,
            "Vertices" => Self::Vertices,
            "ShadedVertices" => Self::ShadedVertices,
            "Illustration" => Self::Illustration,
            "SolidOutline" => Self::SolidOutline,
            "ShadedIllustration" => Self::ShadedIllustration,
            _ => Self::Other(name.to_string()),
        }
    }

    /// 写回 `/Subtype` 的名字拼写。
    pub fn as_name(&self) -> &str {
        match self {
            Self::Solid => "Solid",
            Self::SolidWireframe => "SolidWireframe",
            Self::Transparent => "Transparent",
            Self::TransparentWireframe => "TransparentWireframe",
            Self::BoundingBox => "BoundingBox",
            Self::TransparentBoundingBox => "TransparentBoundingBox",
            Self::TransparentBoundingBoxOutline => "TransparentBoundingBoxOutline",
            Self::Wireframe => "Wireframe",
            Self::ShadedWireframe => "ShadedWireframe",
            Self::HiddenWireframe => "HiddenWireframe",
            Self::Vertices => "Vertices",
            Self::ShadedVertices => "ShadedVertices",
            Self::Illustration => "Illustration",
            Self::SolidOutline => "SolidOutline",
            Self::ShadedIllustration => "ShadedIllustration",
            Self::Other(name) => name,
        }
    }
}

/// 面的颜色（`/FC`）：名字 `BG`（当前背景色，缺省）或 `[/DeviceRGB r g b]`。
#[derive(Debug, Clone, PartialEq)]
pub enum ThreeDFaceColor {
    /// `BG`：取当前背景色。
    Background,
    /// `[/DeviceRGB r g b]`。
    DeviceRgb([f64; 3]),
    /// 未识别的名字或其他颜色数组（原值保留）。
    Other(Value),
}

impl ThreeDFaceColor {
    /// 写成名字 `BG`、`[/DeviceRGB r g b]` 数组或原值。
    pub fn to_value(&self) -> Value {
        match self {
            Self::Background => Value::Name("BG".into()),
            Self::DeviceRgb(values) => rgb_array(*values),
            Self::Other(value) => value.clone(),
        }
    }
}

/// 3D 渲染模式字典（Table 307）。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDRenderMode {
    /// `/Subtype` 渲染模式。
    pub subtype: ThreeDRenderModeType,
    /// `/AC` 线框与顶点颜色；`None` 表示缺省黑色。
    pub auxiliary_color: Option<ThreeDColor>,
    /// `/FC` 面颜色（仅 `Illustration` 有意义）；`None` 表示缺省 `BG`。
    pub face_color: Option<ThreeDFaceColor>,
    /// `/O` 不透明度；`None` 表示缺省 0.5。
    pub opacity: Option<f64>,
    /// `/CV` 折痕角（度）；`None` 表示缺省 45。
    pub crease_value: Option<f64>,
}

/// 解析已解引用的渲染模式字典。
pub(super) fn parse_render_mode(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDRenderMode, ThreeDError> {
    check_type(dict, "3DRenderMode", context)?;
    let Some(name) = optional_name(document, dict, "Subtype", context)? else {
        return Err(malformed(format!("{context} is missing required /Subtype")));
    };
    let subtype = ThreeDRenderModeType::from_name(&name);
    let auxiliary_color = match dict.get("AC") {
        None => None,
        Some(value) => Some(parse_color(document, value, &format!("{context} /AC"))?),
    };
    let face_color = match dict.get("FC") {
        None => None,
        Some(value) => Some(parse_face_color(
            document,
            value,
            &format!("{context} /FC"),
        )?),
    };
    let opacity = optional_number(document, dict, "O", context)?;
    let crease_value = optional_number(document, dict, "CV", context)?;
    Ok(ThreeDRenderMode {
        subtype,
        auxiliary_color,
        face_color,
        opacity,
        crease_value,
    })
}

/// `/FC`：名字 `BG`、其他名字或颜色数组。
fn parse_face_color(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<ThreeDFaceColor, ThreeDError> {
    let resolved = resolve_value(document, value)?;
    match resolved {
        Value::Name(name) if name == "BG" => Ok(ThreeDFaceColor::Background),
        Value::Name(name) => Ok(ThreeDFaceColor::Other(Value::Name(name))),
        Value::Array(_) => match parse_color(document, &resolved, context)? {
            ThreeDColor::DeviceRgb(values) => Ok(ThreeDFaceColor::DeviceRgb(values)),
            ThreeDColor::Other(value) => Ok(ThreeDFaceColor::Other(value)),
        },
        _ => Err(malformed(format!("{context} must be a name or an array"))),
    }
}
