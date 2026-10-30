//! 3D 流字典（Table 300）。
//!
//! `/Subtype` 必填，唯一合法值为 `U3D`；未知格式保留为
//! [`ThreeDStreamFormat::Other`]（规范要求消费者宽容）。`/VA` 为 3D 视图
//! 字典数组（上限 1024），`/DV` 的解析与 `/VA` 存在时的缺省下标 0 见
//! [`ThreeDStream::default_view`]。`/Resources` `/OnInstantiate` 保留原值，
//! 不类型化名称树与 JavaScript 流；`/AN` 动画样式字典（PDF 1.7，
//! Table 301/302）解析为 [`ThreeDAnimationStyle`]。

use crate::core::types::{Dict, Document, Value};

use super::animation::{ThreeDAnimationStyle, parse_animation_style};
use super::view::{ThreeDView, ThreeDViewSelection, parse_view, parse_view_selection};
use super::{MAX_VIEWS, ThreeDError, array_value, check_type, malformed, resolve_value};

/// 3D 流数据格式（`/Subtype`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDStreamFormat {
    /// `U3D`：Universal 3D 文件格式。
    U3d,
    /// 未识别的格式名（原样保留，字节不解码）。
    Other(String),
}

/// 3D 流字典（Table 300）。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDStream {
    /// `/Subtype` 数据格式。
    pub format: ThreeDStreamFormat,
    /// `/VA` 预设视图数组（可为空）。
    pub views: Vec<ThreeDView>,
    /// `/DV` 默认视图；`/VA` 存在且非空而 `/DV` 缺失时为 `Index(0)`，
    /// 否则 `None` 表示默认视图由 3D 数据内部指定。
    pub default_view: Option<ThreeDViewSelection>,
    /// `/Resources` 名称树（原值保留）。
    pub resources: Option<crate::core::types::Value>,
    /// `/OnInstantiate` 实例化时执行的 JavaScript 流（原值保留）。
    pub on_instantiate: Option<crate::core::types::Value>,
    /// `/AN` 动画样式字典（PDF 1.7，Table 301/302）；缺省不物化。
    pub animation: Option<ThreeDAnimationStyle>,
}

/// 3D 流解析结果：流模型 + `/VA` 是否存在（`Some(视图数)`）。
#[derive(Debug)]
pub(super) struct ParsedStream {
    pub(super) stream: ThreeDStream,
    pub(super) view_count: Option<usize>,
}

/// 解析已解引用的 3D 流字典；`context` 用于错误消息。
pub(super) fn parse_stream(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ParsedStream, ThreeDError> {
    check_type(dict, "3D", context)?;
    let format = match dict.get("Subtype") {
        None => {
            return Err(malformed(format!("{context} is missing required /Subtype")));
        }
        Some(value) => {
            let resolved = resolve_value(document, value)?;
            match resolved {
                crate::core::types::Value::Name(name) if name == "U3D" => ThreeDStreamFormat::U3d,
                crate::core::types::Value::Name(name) => ThreeDStreamFormat::Other(name),
                _ => {
                    return Err(malformed(format!("{context} /Subtype must be a name")));
                }
            }
        }
    };

    let mut views_present = false;
    let mut views = Vec::new();
    if let Some(value) = dict.get("VA") {
        views_present = true;
        let array = array_value(document, value, &format!("{context} /VA"))?;
        if array.len() > MAX_VIEWS {
            return Err(ThreeDError::LimitExceeded("3D stream /VA views"));
        }
        views = Vec::with_capacity(array.len());
        for (index, entry) in array.iter().enumerate() {
            let resolved = resolve_value(document, entry)?;
            let view = resolved
                .as_dict()
                .ok_or_else(|| malformed(format!("{context} /VA[{index}] is not a dictionary")))?;
            views.push(parse_view(
                document,
                view,
                &format!("{context} /VA[{index}]"),
            )?);
        }
    }

    let view_count = if views_present {
        Some(views.len())
    } else {
        None
    };
    let default_view = match dict.get("DV") {
        Some(value) => Some(parse_view_selection(
            document,
            value,
            view_count,
            &format!("{context} /DV"),
        )?),
        None if views_present && !views.is_empty() => Some(ThreeDViewSelection::Index(0)),
        None => None,
    };

    let animation = match dict.get("AN") {
        None => None,
        Some(value) => {
            let context = format!("{context} /AN");
            let resolved = resolve_value(document, value)?;
            let Value::Dict(animation) = resolved else {
                return Err(malformed(format!("{context} is not a dictionary")));
            };
            Some(parse_animation_style(document, &animation, &context)?)
        }
    };

    Ok(ParsedStream {
        stream: ThreeDStream {
            format,
            views,
            default_view,
            resources: dict.get("Resources").cloned(),
            on_instantiate: dict.get("OnInstantiate").cloned(),
            animation,
        },
        view_count,
    })
}
