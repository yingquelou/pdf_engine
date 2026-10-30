//! 3D 背景字典（Table 306）。
//!
//! 目前规范只定义单色不透明背景：`/Subtype /SC`、`/CS /DeviceRGB`、
//! `/C [1 1 1]`、`/EA false`。缺省值不物化；`/Subtype` 的未知名保留为
//! `Other`，`/CS` `/C` 按原值保留（规范要求消费者宽容未来取值）。

use crate::core::types::{Dict, Document, Value};

use super::{ThreeDError, check_type, optional_bool, optional_name, resolve_value};

/// 背景类型（`/Subtype`，缺省 `SC`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDBackgroundType {
    /// `SC`：单色不透明背景（缺省）。
    SolidColor,
    /// 未识别的背景类型（原样保留）。
    Other(String),
}

/// 3D 背景字典（Table 306）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDBackground {
    /// `/Subtype` 背景类型；`None` 表示缺省 `SC`。
    pub subtype: Option<ThreeDBackgroundType>,
    /// `/CS` 颜色空间原值（名字或数组）；`None` 表示缺省 `DeviceRGB`。
    pub color_space: Option<Value>,
    /// `/C` 颜色原值；`None` 表示缺省 `[1 1 1]`。
    pub color: Option<Value>,
    /// `/EA` 背景是否覆盖整个注解（缺省 false）。
    pub entire_annotation: Option<bool>,
}

/// 解析已解引用的 3D 背景字典。
pub(super) fn parse_background(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDBackground, ThreeDError> {
    check_type(dict, "3DBG", context)?;
    let subtype = optional_name(document, dict, "Subtype", context)?.map(|name| {
        if name == "SC" {
            ThreeDBackgroundType::SolidColor
        } else {
            ThreeDBackgroundType::Other(name)
        }
    });
    let color_space = match dict.get("CS") {
        None => None,
        Some(value) => Some(resolve_value(document, value)?),
    };
    let color = match dict.get("C") {
        None => None,
        Some(value) => Some(resolve_value(document, value)?),
    };
    let entire_annotation = optional_bool(document, dict, "EA", context)?;
    Ok(ThreeDBackground {
        subtype,
        color_space,
        color,
        entire_annotation,
    })
}
