//! 3D artwork 读取模型（ISO 32000-1 13.6，PDF 1.6）。
//!
//! 覆盖 3D 注解专有条目（Table 298）、3D 激活字典（Table 299）、3D 流字典
//! （Table 300）、动画样式字典（Table 301/302）、3D 引用字典（Table 303）、
//! 3D 视图字典（Table 304）、投影字典（Table 305）与背景字典（Table 306）。
//! PDF 1.7 的视图负载条目也按表类型化：渲染模式（Table 307/308）、光照方案
//! （Table 309/310）、截面（Table 311）与节点（Table 312）。本切片只做语法层
//! 类型化：U3D 字节不解码。
//!
//! 入口 [`parse_three_d_entries`] 由注解读端调用：`/Subtype` 为 `3D` 或
//! 字典中出现任一 `/3D*` 键时解析，非 3D 子类型带 `/3DD` 也照常解析
//! （读端宽容）。声明为 3D 子类型却缺少必填 `/3DD` 时报错。
//!
//! 多个注解可经同一个 3D 引用字典共享运行实例：[`ThreeDArtwork::reference`]
//! 保存引用字典的对象号，调用方可据此判等；[`ThreeDArtwork::source`] 始终是
//! 实际 3D 流的引用（直接内联时为 `None`），仅凭流引用相等不能推出实例共享。

use std::fmt;

use crate::core::reader::annotation::AnnotationSubtype;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Array, Dict, Document, Value};

mod activation;
mod animation;
mod background;
mod color;
mod cross_section;
mod lighting;
mod node;
mod projection;
mod render_mode;
mod resources;
mod stream;
#[cfg(test)]
mod tests;
mod view;

pub use activation::{
    ThreeDActivation, ThreeDActivationPolicy, ThreeDArtworkState, ThreeDDeactivationPolicy,
};
pub use animation::{ThreeDAnimationStyle, ThreeDAnimationStyleType};
pub use background::{ThreeDBackground, ThreeDBackgroundType};
pub use color::ThreeDColor;
pub use cross_section::ThreeDCrossSection;
pub use lighting::{ThreeDLightingScheme, ThreeDLightingSchemeType};
pub use node::ThreeDNode;
pub use projection::{
    ThreeDClipping, ThreeDProjection, ThreeDProjectionBinding, ThreeDProjectionScale,
    ThreeDProjectionType,
};
pub use render_mode::{ThreeDFaceColor, ThreeDRenderMode, ThreeDRenderModeType};
pub use stream::{ThreeDStream, ThreeDStreamFormat};
pub(crate) use view::parse_view;
pub use view::{ThreeDMatrix, ThreeDMatrixSource, ThreeDView, ThreeDViewSelection};

/// `/VA` 视图数组上限（防护不可信文件）。
const MAX_VIEWS: usize = 1024;
/// `/U3DPath` 文本串数组上限（防护不可信文件）。
const MAX_PATH_SEGMENTS: usize = 64;
/// 视图 `/SA` 截面数组上限（防护不可信文件）。
const MAX_CROSS_SECTIONS: usize = 1024;
/// 视图 `/NA` 节点数组上限（防护不可信文件）。
const MAX_NODES: usize = 1024;

/// 3D 对象解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDError {
    /// 条目缺失、类型或取值非法，或间接引用无法解析。
    Malformed(String),
    /// 超出视图或路径数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for ThreeDError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => f.write_str(message),
            Self::LimitExceeded(what) => write!(f, "3D limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for ThreeDError {}

pub(crate) fn malformed(message: impl Into<String>) -> ThreeDError {
    ThreeDError::Malformed(message.into())
}

/// 解引用间接引用，`NavigationError` 统一折进 [`ThreeDError::Malformed`]。
pub(super) fn resolve_value(document: &Document, value: &Value) -> Result<Value, ThreeDError> {
    resolve(document, value).map_err(|error| malformed(error.to_string()))
}

/// 条目间接引用必须解析为数组；返回克隆的数组。
pub(super) fn array_value(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Array, ThreeDError> {
    let resolved = resolve_value(document, value)?;
    resolved
        .as_array()
        .cloned()
        .ok_or_else(|| malformed(format!("{context} is not an array")))
}

/// 校验字典 `/Type`（缺失视为合法，存在则必须匹配）。
pub(super) fn check_type(dict: &Dict, expected: &str, context: &str) -> Result<(), ThreeDError> {
    match dict.get("Type") {
        None => Ok(()),
        Some(Value::Name(name)) if name == expected => Ok(()),
        Some(Value::Name(other)) => Err(malformed(format!(
            "{context} /Type is {other}, expected {expected}"
        ))),
        Some(_) => Err(malformed(format!("{context} /Type is not a name"))),
    }
}

/// 有限数值（整数或浮点）；非数值或 NaN/Inf 报错。
pub(super) fn number_value(value: &Value, context: &str) -> Result<f64, ThreeDError> {
    let number = match value {
        Value::Integer(number) => *number as f64,
        Value::Float(number) => *number,
        _ => return Err(malformed(format!("{context} must be a number"))),
    };
    if !number.is_finite() {
        return Err(malformed(format!("{context} must be finite")));
    }
    Ok(number)
}

/// 可选数值条目；存在即解析（间接引用先解引用）。
pub(super) fn optional_number(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<f64>, ThreeDError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let resolved = resolve_value(document, value)?;
    Ok(Some(number_value(&resolved, &format!("{context} /{key}"))?))
}

/// 可选布尔条目；非布尔报错。
pub(super) fn optional_bool(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<bool>, ThreeDError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    match resolve_value(document, value)? {
        Value::Bool(flag) => Ok(Some(flag)),
        _ => Err(malformed(format!("{context} /{key} must be a boolean"))),
    }
}

/// 可选名字条目；非名字报错，未知名字由调用方落 `Other`。
pub(super) fn optional_name(
    document: &Document,
    dict: &Dict,
    key: &str,
    context: &str,
) -> Result<Option<String>, ThreeDError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    match resolve_value(document, value)? {
        Value::Name(name) => Ok(Some(name)),
        _ => Err(malformed(format!("{context} /{key} must be a name"))),
    }
}

/// 解析独立的 3D 视图值（Go-To-3D-View 动作 `/V`，12.6.4.15 Table 216）：
/// 间接引用先解引用，值必须是视图字典（Table 304）。
pub(crate) fn parse_view_value(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<ThreeDView, ThreeDError> {
    let resolved = resolve_value(document, value)?;
    let Value::Dict(dict) = resolved else {
        return Err(malformed(format!("{context} is not a 3D view dictionary")));
    };
    view::parse_view(document, &dict, context)
}

/// 文本串（字面串或十六进制串），返回原值以便调用方保留编码。
pub(super) fn text_value(value: &Value, context: &str) -> Result<Value, ThreeDError> {
    match value {
        Value::String(_) | Value::HexString(_) => Ok(value.clone()),
        _ => Err(malformed(format!("{context} must be a text string"))),
    }
}

/// 判定文本串值是否为空（十六进制串忽略 ASCII 空白）。
pub(super) fn text_value_is_empty(value: &Value) -> bool {
    match value {
        Value::String(text) => text.is_empty(),
        Value::HexString(hex) => hex.chars().all(|ch| ch.is_ascii_whitespace()),
        _ => true,
    }
}

/// 3D 注解专有条目（Table 298）的读取结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDEntries {
    /// `/3DD` 3D 流或 3D 引用字典；声明 3D 子类型时必填。
    pub artwork: Option<ThreeDArtwork>,
    /// `/3DV` 初始视图选择；缺省表示使用 3D 流内的默认视图。
    pub view: Option<ThreeDViewSelection>,
    /// `/3DA` 激活字典；缺省不物化。
    pub activation: Option<ThreeDActivation>,
    /// `/3DI` 是否供交互使用（缺省 true 由呈现端应用）。
    pub interactive: Option<bool>,
    /// `/3DB` 视图框（注解目标坐标系，原点为 `/Rect` 中心）；
    /// 缺省 `[-w/2 -h/2 w/2 h/2]` 由呈现端计算。
    pub view_box: Option<[f64; 4]>,
}

/// 3D artwork：实际 3D 流及其实例共享信息。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDArtwork {
    /// 3D 流对象的间接引用；`/3DD` 直接内联流或引用字典内联流时为 `None`。
    pub source: Option<(i64, i64)>,
    /// 承载 `/3DD` 的 3D 引用字典对象号；直接引用流时为 `None`。
    ///
    /// 两个注解的 `reference` 相等表示它们共享同一运行实例；仅 `source`
    /// 相等（直接引用同一流）不构成共享。
    pub reference: Option<(i64, i64)>,
    /// 3D 流字典（Table 300）。
    pub stream: ThreeDStream,
}

/// 解析 3D 注解专有条目（Table 298）。
///
/// 仅当 `/Subtype` 是 `3D` 或字典中出现任一 `/3D*` 键时解析；其余情况
/// 返回 `None`。声明为 3D 却缺少 `/3DD`、流/引用字典/嵌套字典损坏时返回
/// 类型化错误。
pub(crate) fn parse_three_d_entries(
    document: &Document,
    dict: &Dict,
    subtype: &AnnotationSubtype,
) -> Result<Option<ThreeDEntries>, ThreeDError> {
    let declared = *subtype == AnnotationSubtype::ThreeD;
    let has_entries = dict.0.keys().any(|key| key.starts_with("3D"));
    if !declared && !has_entries {
        return Ok(None);
    }

    let (artwork, view_count) = match dict.get("3DD") {
        Some(value) => {
            let (artwork, view_count) = parse_artwork(document, value)?;
            (Some(artwork), view_count)
        }
        None if declared => {
            return Err(malformed("3D annotation is missing required /3DD"));
        }
        None => (None, None),
    };

    let view = match dict.get("3DV") {
        None => None,
        Some(value) => Some(view::parse_view_selection(
            document,
            value,
            view_count,
            "3D annotation /3DV",
        )?),
    };

    let activation = match dict.get("3DA") {
        None => None,
        Some(value) => {
            let resolved = resolve_value(document, value)?;
            let dict = resolved
                .as_dict()
                .ok_or_else(|| malformed("3D annotation /3DA is not a dictionary"))?;
            Some(activation::parse_activation(document, dict)?)
        }
    };

    let interactive = optional_bool(document, dict, "3DI", "3D annotation")?;
    let view_box = match dict.get("3DB") {
        None => None,
        Some(value) => Some(parse_view_box(document, value)?),
    };

    Ok(Some(ThreeDEntries {
        artwork,
        view,
        activation,
        interactive,
        view_box,
    }))
}

/// `/3DD`：直接 3D 流或 3D 引用字典。
///
/// 返回 artwork 与 `/VA` 是否存在（`Some(视图数)` 表示存在，供 `/3DV`
/// 下标越界校验）。
fn parse_artwork(
    document: &Document,
    value: &Value,
) -> Result<(ThreeDArtwork, Option<usize>), ThreeDError> {
    let reference = value.as_ref();
    let resolved = resolve_value(document, value)?;
    match resolved {
        Value::Stream(dict, _) => {
            let parsed = stream::parse_stream(document, &dict, "3D stream")?;
            Ok((
                ThreeDArtwork {
                    source: reference,
                    reference: None,
                    stream: parsed.stream,
                },
                parsed.view_count,
            ))
        }
        Value::Dict(dict) => {
            check_type(&dict, "3DRef", "3D reference dictionary")?;
            let inner = dict
                .get("3DD")
                .ok_or_else(|| malformed("3D reference dictionary is missing required /3DD"))?;
            let source = inner.as_ref();
            let resolved = resolve_value(document, inner)?;
            let Value::Stream(stream_dict, _) = resolved else {
                return Err(malformed("3D reference dictionary /3DD is not a 3D stream"));
            };
            let parsed = stream::parse_stream(document, &stream_dict, "3D stream")?;
            Ok((
                ThreeDArtwork {
                    source,
                    reference,
                    stream: parsed.stream,
                },
                parsed.view_count,
            ))
        }
        _ => Err(malformed(
            "3D annotation /3DD is not a 3D stream or 3D reference dictionary",
        )),
    }
}

/// `/3DB`：四个有限数值组成的视图框。
fn parse_view_box(document: &Document, value: &Value) -> Result<[f64; 4], ThreeDError> {
    let array = array_value(document, value, "3D annotation /3DB")?;
    if array.len() != 4 {
        return Err(malformed("3D annotation /3DB must hold four numbers"));
    }
    let mut out = [0.0; 4];
    for (index, item) in array.iter().enumerate() {
        let resolved = resolve_value(document, item)?;
        out[index] = number_value(&resolved, "3D annotation /3DB entry")?;
    }
    Ok(out)
}
