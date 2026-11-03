//! 框样式与框颜色信息的类型建模（Table 360/361，缺省值不物化）。

use crate::core::types::Dict;

/// 框样式 `/S`（Table 361）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxStyleName {
    /// `S`：实线。
    Solid,
    /// `D`：虚线。
    Dashed,
    /// 扩展定义样式名（Table 361 允许未来定义），原样保留。
    Other(String),
}

impl BoxStyleName {
    /// 按 `/S` 名称解析；未知名称保留为 [`BoxStyleName::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "S" => Self::Solid,
            "D" => Self::Dashed,
            other => Self::Other(other.to_owned()),
        }
    }

    /// `/S` 名称（标准名称或扩展原文）。
    pub fn name(&self) -> &str {
        match self {
            Self::Solid => "S",
            Self::Dashed => "D",
            Self::Other(name) => name,
        }
    }
}

/// 框样式字典（Table 361）。
///
/// 各字段对应表内声明条目；缺省值不物化（`/C` 缺省 `[0 0 0]`、`/W`
/// 缺省 1、`/S` 缺省 `S`、`/D` 缺省 `[3]`）。`dict` 保留未建模条目。
#[derive(Debug, Clone, PartialEq)]
pub struct BoxStyle {
    /// `/C`：3 个 0.0–1.0 的 DeviceRGB 分量。
    pub color: Option<[f64; 3]>,
    /// `/W`：导线宽度（默认用户空间单位）。
    pub width: Option<f64>,
    /// `/S`：样式名称。
    pub style: Option<BoxStyleName>,
    /// `/D`：虚线数组（默认用户空间单位，同 8.4.3.6 线 dash 格式，相位 0）。
    pub dash: Option<Vec<f64>>,
    /// 原始样式字典（保留未建模条目）。
    pub dict: Dict,
}

impl BoxStyle {
    /// 空样式（全部条目取表内缺省）。
    pub fn new() -> Self {
        Self {
            color: None,
            width: None,
            style: None,
            dash: None,
            dict: Dict::new(),
        }
    }

    /// 设置 `/C`。
    pub fn with_color(mut self, color: [f64; 3]) -> Self {
        self.color = Some(color);
        self
    }

    /// 设置 `/W`。
    pub fn with_width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    /// 设置 `/S`。
    pub fn with_style(mut self, style: BoxStyleName) -> Self {
        self.style = Some(style);
        self
    }

    /// 设置 `/D`。
    pub fn with_dash(mut self, dash: Vec<f64>) -> Self {
        self.dash = Some(dash);
        self
    }
}

impl Default for BoxStyle {
    fn default() -> Self {
        Self::new()
    }
}

/// 框颜色信息字典 `/BoxColorInfo`（Table 360）。
///
/// 四个框样式条目均可选；页面对象未定义对应框时该条目被忽略（Table 360
/// 注），读端不强制。
#[derive(Debug, Clone, PartialEq)]
pub struct BoxColorInfo {
    /// `/CropBox` 框样式。
    pub crop: Option<BoxStyle>,
    /// `/BleedBox` 框样式。
    pub bleed: Option<BoxStyle>,
    /// `/TrimBox` 框样式。
    pub trim: Option<BoxStyle>,
    /// `/ArtBox` 框样式。
    pub art: Option<BoxStyle>,
    /// 原始条目字典（保留未建模条目）。
    pub dict: Dict,
}

impl BoxColorInfo {
    /// 空框颜色信息（无任何框样式）。
    pub fn new() -> Self {
        Self {
            crop: None,
            bleed: None,
            trim: None,
            art: None,
            dict: Dict::new(),
        }
    }

    /// 设置 `/CropBox` 框样式。
    pub fn with_crop(mut self, style: BoxStyle) -> Self {
        self.crop = Some(style);
        self
    }

    /// 设置 `/BleedBox` 框样式。
    pub fn with_bleed(mut self, style: BoxStyle) -> Self {
        self.bleed = Some(style);
        self
    }

    /// 设置 `/TrimBox` 框样式。
    pub fn with_trim(mut self, style: BoxStyle) -> Self {
        self.trim = Some(style);
        self
    }

    /// 设置 `/ArtBox` 框样式。
    pub fn with_art(mut self, style: BoxStyle) -> Self {
        self.art = Some(style);
        self
    }
}

impl Default for BoxColorInfo {
    fn default() -> Self {
        Self::new()
    }
}
