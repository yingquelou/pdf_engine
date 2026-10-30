//! 3D 投影字典（Table 305）。
//!
//! `/Subtype` 必填且只接受 `O`（正交）与 `P`（透视）；透视投影要求
//! `/N > 0` 与 `0 <= /FOV <= 180`，正交投影的 `/N` 可选且须 ≥0。
//! `/CS` 缺省 `ANF`（自动近/远裁剪），`/PS` 缺省 `W`，`/OS` 缺省 1，
//! `/OB` 缺省 `Absolute`；这些缺省值不物化。`/PS` `/OB` 的未知名字落
//! `Other` 保留原文。

use crate::core::types::{Dict, Document, Value};

use super::{ThreeDError, malformed, number_value, optional_name, optional_number, resolve_value};

/// 投影类型（`/Subtype`，必填）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDProjectionType {
    /// `O`：正交投影。
    Orthographic,
    /// `P`：透视投影。
    Perspective,
}

/// 裁剪方式（`/CS`，缺省 `ANF`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDClipping {
    /// `XNF`：由 `/N` `/F` 显式给出近/远裁剪距离。
    Explicit,
    /// `ANF`：根据 artwork 内容自动确定近/远平面（缺省）。
    Automatic,
}

/// 透视投影的缩放（`/PS`，缺省 `W`）。
#[derive(Debug, Clone, PartialEq)]
pub enum ThreeDProjectionScale {
    /// 正数：近平面与视锥交圆在目标坐标系中的直径。
    Diameter(f64),
    /// `W`：取 3D 视图框宽度（缺省）。
    Width,
    /// `H`：取 3D 视图框高度。
    Height,
    /// `Min`：取宽、高较小者。
    Min,
    /// `Max`：取宽、高较大者。
    Max,
    /// 未识别的名字（原样保留）。
    Other(String),
}

/// 正交投影的绑定策略（`/OB`，缺省 `Absolute`，PDF 1.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDProjectionBinding {
    /// `W`：缩放到注解宽度。
    Width,
    /// `H`：缩放到注解高度。
    Height,
    /// `Min`：缩放到宽、高较小者。
    Min,
    /// `Max`：缩放到宽、高较大者。
    Max,
    /// `Absolute`：不因绑定而缩放（缺省）。
    Absolute,
    /// 未识别的名字（原样保留）。
    Other(String),
}

/// 3D 投影字典（Table 305）。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDProjection {
    /// `/Subtype` 投影类型。
    pub subtype: ThreeDProjectionType,
    /// `/CS` 裁剪方式；`None` 表示缺省 `ANF`。
    pub clipping: Option<ThreeDClipping>,
    /// `/F` 远裁剪距离（仅 `/CS /XNF` 有意义）；`None` 表示无远裁剪。
    pub far: Option<f64>,
    /// `/N` 近裁剪距离；正交投影的缺省值 0 不物化。
    pub near: Option<f64>,
    /// `/FOV` 视场角（度，仅透视投影）；缺省值 90 只在未声明 `/P` 时适用。
    pub field_of_view: Option<f64>,
    /// `/PS` 透视投影缩放；`None` 表示缺省 `W`。
    pub scale: Option<ThreeDProjectionScale>,
    /// `/OS` 正交投影缩放因子；`None` 表示缺省 1。
    pub orthographic_scale: Option<f64>,
    /// `/OB` 正交投影绑定策略；`None` 表示缺省 `Absolute`。
    pub binding: Option<ThreeDProjectionBinding>,
}

/// 解析已解引用的投影字典。
pub(super) fn parse_projection(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDProjection, ThreeDError> {
    let subtype = match optional_name(document, dict, "Subtype", context)? {
        Some(name) => match name.as_str() {
            "O" => ThreeDProjectionType::Orthographic,
            "P" => ThreeDProjectionType::Perspective,
            _ => {
                return Err(malformed(format!("{context} /Subtype must be O or P")));
            }
        },
        None => {
            return Err(malformed(format!("{context} is missing required /Subtype")));
        }
    };
    let clipping = match optional_name(document, dict, "CS", context)? {
        None => None,
        Some(name) => match name.as_str() {
            "XNF" => Some(ThreeDClipping::Explicit),
            "ANF" => Some(ThreeDClipping::Automatic),
            _ => {
                return Err(malformed(format!("{context} /CS must be XNF or ANF")));
            }
        },
    };
    let far = optional_number(document, dict, "F", context)?;
    let near = optional_number(document, dict, "N", context)?;

    let (near, field_of_view, scale, orthographic_scale, binding) = match subtype {
        ThreeDProjectionType::Perspective => {
            let near = near.ok_or_else(|| {
                malformed(format!("{context} perspective projection requires /N"))
            })?;
            if near <= 0.0 {
                return Err(malformed(format!(
                    "{context} perspective /N must be positive"
                )));
            }
            let fov = optional_number(document, dict, "FOV", context)?.ok_or_else(|| {
                malformed(format!("{context} perspective projection requires /FOV"))
            })?;
            if !(0.0..=180.0).contains(&fov) {
                return Err(malformed(format!("{context} /FOV must be within 0..=180")));
            }
            // `/OS` `/OB` 对透视投影无意义，按规范忽略。
            (
                Some(near),
                Some(fov),
                parse_scale(document, dict, context)?,
                None,
                None,
            )
        }
        ThreeDProjectionType::Orthographic => {
            if let Some(near) = near
                && near < 0.0
            {
                return Err(malformed(format!(
                    "{context} orthographic /N must not be negative"
                )));
            }
            let orthographic_scale = optional_number(document, dict, "OS", context)?;
            if let Some(scale) = orthographic_scale
                && scale <= 0.0
            {
                return Err(malformed(format!("{context} /OS must be positive")));
            }
            let binding =
                optional_name(document, dict, "OB", context)?.map(|name| match name.as_str() {
                    "W" => ThreeDProjectionBinding::Width,
                    "H" => ThreeDProjectionBinding::Height,
                    "Min" => ThreeDProjectionBinding::Min,
                    "Max" => ThreeDProjectionBinding::Max,
                    "Absolute" => ThreeDProjectionBinding::Absolute,
                    _ => ThreeDProjectionBinding::Other(name),
                });
            // `/FOV` 与 `/PS` 对正交投影无意义，按规范忽略。
            (near, None, None, orthographic_scale, binding)
        }
    };

    Ok(ThreeDProjection {
        subtype,
        clipping,
        far,
        near,
        field_of_view,
        scale,
        orthographic_scale,
        binding,
    })
}

/// `/PS`：正数直径或名字 `W`/`H`/`Min`/`Max`；缺失时返回 `None`（缺省 `W`）。
fn parse_scale(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<Option<ThreeDProjectionScale>, ThreeDError> {
    let Some(value) = dict.get("PS") else {
        return Ok(None);
    };
    let resolved = resolve_value(document, value)?;
    match resolved {
        Value::Integer(_) | Value::Float(_) => {
            let number = number_value(&resolved, &format!("{context} /PS"))?;
            if number <= 0.0 {
                return Err(malformed(format!("{context} /PS must be positive")));
            }
            Ok(Some(ThreeDProjectionScale::Diameter(number)))
        }
        Value::Name(name) => Ok(Some(match name.as_str() {
            "W" => ThreeDProjectionScale::Width,
            "H" => ThreeDProjectionScale::Height,
            "Min" => ThreeDProjectionScale::Min,
            "Max" => ThreeDProjectionScale::Max,
            _ => ThreeDProjectionScale::Other(name),
        })),
        _ => Err(malformed(format!(
            "{context} /PS must be a positive number or a name"
        ))),
    }
}
