//! Layout 取值形态枚举（ISO 32000-1 14.8.5.4 Table 342–346）：同一条目
//! 的「单值 / four-entry（before/after/start/end）/ 数字或名字」两式建模。
//! 名字与数值到 `Value` 的序列化由 `values` 模块完成。

use super::enums::BorderStyle;

/// `/BorderStyle` `/TBorderStyle` 的单值或四边形态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderStyles {
    /// 四个边统一使用同一样式。
    Uniform(BorderStyle),
    /// before/after/start/end 四边；`None` 表示该边不绘制。
    Edges([Option<BorderStyle>; 4]),
}

/// `/BorderColor` 的单值或四边形态。
#[derive(Debug, Clone, PartialEq)]
pub enum BorderColor {
    /// 四个边统一使用同一 RGB 颜色（分量 0–1）。
    Uniform([f64; 3]),
    /// before/after/start/end 四边；`None` 表示该边不绘制。
    Edges([Option<[f64; 3]>; 4]),
}

/// `/Padding` `/TPadding` 的单值或四数字数组形态。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrFour {
    /// 四边统一使用同一数值。
    Uniform(f64),
    /// before/after/start/end 四边的数值。
    Edges([f64; 4]),
}

/// `/BorderThickness` 的单值或四边形态。
#[derive(Debug, Clone, PartialEq)]
pub enum BorderThickness {
    /// 四边统一使用同一厚度（非负）。
    Uniform(f64),
    /// before/after/start/end 四边；`None` 表示该边不绘制。
    Edges([Option<f64>; 4]),
}

/// `/Width` `/Height` 的数值或 `Auto` 形态。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrAuto {
    /// 具体数值。
    Number(f64),
    /// `/Auto`：不施加尺寸约束（缺省）。
    Auto,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl NumberOrAuto {
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Auto => "Auto",
            Self::Other(name) => name,
            Self::Number(_) => "Auto",
        }
    }
}

/// 行高（Table 345 `/LineHeight`）：数值、`Normal`、`Auto` 或扩展名字。
#[derive(Debug, Clone, PartialEq)]
pub enum LineHeight {
    /// 具体数值。
    Number(f64),
    /// `/Normal`：按字号调整行高，并计入 `BaselineShift`（缺省）。
    Normal,
    /// `/Auto`：按字号调整行高，不计入 `BaselineShift`。
    Auto,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl LineHeight {
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Normal => "Normal",
            Self::Auto => "Auto",
            Self::Other(name) => name,
            Self::Number(_) => "Normal",
        }
    }
}

/// 竖排字形方向（Table 345 `/GlyphOrientationVertical`）。
#[derive(Debug, Clone, PartialEq)]
pub enum GlyphOrientationVertical {
    /// `/Auto`：按字形宽度自动选择（缺省）。
    Auto,
    /// 顺时针旋转角度，须为 90 的倍数且在 −180 到 360 之间。
    Angle(f64),
    /// 未知名字，按原名字写出。
    Other(String),
}

impl GlyphOrientationVertical {
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Auto => "Auto",
            Self::Other(name) => name,
            Self::Angle(_) => "Auto",
        }
    }
}

/// `/ColumnGap` `/ColumnWidths` 的单值或数字数组形态。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrNumbers {
    /// 所有列使用同一数值。
    Number(f64),
    /// 按列给出的数值数组（须非空）。
    Array(Vec<f64>),
}
