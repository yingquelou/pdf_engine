//! 3D 光照方案字典（Table 309/310）。
//!
//! `/Subtype` 必填，合法值为 `Artwork` `None` `White` `Day` `Night` `Hard`
//! `Primary` `Blue` `Red` `Cube` `CAD` `Headlamp`；其他名字保留为
//! [`ThreeDLightingSchemeType::Other`]，规范要求消费者忽略未知方案。
//! 读端只做语法层类型化，不物化任何光照参数。

use crate::core::types::{Dict, Document};

use super::{ThreeDError, check_type, malformed, optional_name};

/// 光照方案（`/Subtype`，Table 310）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDLightingSchemeType {
    /// `Artwork`：沿用 artwork 内定义的光照。
    Artwork,
    /// `None`：不做任何光照。
    None,
    /// `White`：三蓝灰无限光源，无环境光。
    White,
    /// `Day`：三浅灰无限光源，无环境光。
    Day,
    /// `Night`：黄/青/蓝三无限光源，无环境光。
    Night,
    /// `Hard`：三灰无限光源 + 中等环境光。
    Hard,
    /// `Primary`：红/绿/蓝三无限光源，无环境光。
    Primary,
    /// `Blue`：三蓝无限光源，无环境光。
    Blue,
    /// `Red`：三红无限光源，无环境光。
    Red,
    /// `Cube`：六灰无限光源，与主轴对齐，无环境光。
    Cube,
    /// `CAD`：三灰无限光源 + 相机跟随光，无环境光。
    Cad,
    /// `Headlamp`：跟随相机的头灯。
    Headlamp,
    /// 未识别的光照方案（原样保留；规范要求忽略）。
    Other(String),
}

impl ThreeDLightingSchemeType {
    /// 按 `/Subtype` 名字构造；未知名字落 `Other`。
    pub(crate) fn from_name(name: &str) -> Self {
        match name {
            "Artwork" => Self::Artwork,
            "None" => Self::None,
            "White" => Self::White,
            "Day" => Self::Day,
            "Night" => Self::Night,
            "Hard" => Self::Hard,
            "Primary" => Self::Primary,
            "Blue" => Self::Blue,
            "Red" => Self::Red,
            "Cube" => Self::Cube,
            "CAD" => Self::Cad,
            "Headlamp" => Self::Headlamp,
            _ => Self::Other(name.to_string()),
        }
    }

    /// 写回 `/Subtype` 的名字拼写（`Cad` 写作 `CAD`）。
    pub fn as_name(&self) -> &str {
        match self {
            Self::Artwork => "Artwork",
            Self::None => "None",
            Self::White => "White",
            Self::Day => "Day",
            Self::Night => "Night",
            Self::Hard => "Hard",
            Self::Primary => "Primary",
            Self::Blue => "Blue",
            Self::Red => "Red",
            Self::Cube => "Cube",
            Self::Cad => "CAD",
            Self::Headlamp => "Headlamp",
            Self::Other(name) => name,
        }
    }
}

/// 3D 光照方案字典（Table 309）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreeDLightingScheme {
    /// `/Subtype` 光照方案。
    pub subtype: ThreeDLightingSchemeType,
}

/// 解析已解引用的光照方案字典。
pub(super) fn parse_lighting_scheme(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDLightingScheme, ThreeDError> {
    check_type(dict, "3DLightingScheme", context)?;
    let Some(name) = optional_name(document, dict, "Subtype", context)? else {
        return Err(malformed(format!("{context} is missing required /Subtype")));
    };
    Ok(ThreeDLightingScheme {
        subtype: ThreeDLightingSchemeType::from_name(&name),
    })
}
