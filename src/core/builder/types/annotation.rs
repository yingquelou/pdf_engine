//! 注解字典写端（ISO 32000-1 12.5.1–12.5.5）。
//!
//! [`PdfAnnotation`] 以链式构造覆盖注解字典的通用条目（Table 165/166/169）：
//! 子类型、`/Rect`、标志、文本条目、颜色、透明度、边框样式（`/BS`）、
//! `/QuadPoints`、动作 `/A`、目标 `/Dest`、外观流 `/AP`（N/D/R）与
//! `/Popup` 句柄引用。
//! 经 [`PdfBuilder::add_annotation`](super::super::PdfBuilder::add_annotation)
//! 绑定到页面后，由 assemble 写为独立间接对象并登记进页面 `/Annots`。

use super::Action;
use crate::core::reader::navigation::Destination;
use crate::core::types::PdfVersion;

/// 注解子类型（Table 169 写端支持子集）。
///
/// `Sound`/`Movie`/`Screen`/`Redaction` 暂不建模：Widget 由 AcroForm 写端
/// 的 [`FormField`](super::FormField) 合并字段与 Widget 字典写出（E4），
/// 其余按需增补。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    /// 文本批注（`/Text`，PDF 1.0）。
    Text,
    /// 链接（`/Link`，PDF 1.0）。
    Link,
    /// 自由文本（`/FreeText`，PDF 1.0）。
    FreeText,
    /// 直线（`/Line`，PDF 1.0）。
    Line,
    /// 矩形（`/Square`，PDF 1.0）。
    Square,
    /// 椭圆（`/Circle`，PDF 1.0）。
    Circle,
    /// 多边形（`/Polygon`，PDF 1.0）。
    Polygon,
    /// 折线（`/PolyLine`，PDF 1.0）。
    PolyLine,
    /// 高亮（`/Highlight`，PDF 1.3）。
    Highlight,
    /// 下划线（`/Underline`，PDF 1.3）。
    Underline,
    /// 波浪线（`/Squiggly`，PDF 1.4）。
    Squiggly,
    /// 删除线（`/StrikeOut`，PDF 1.3）。
    StrikeOut,
    /// 插入点（`/Caret`，PDF 1.3）。
    Caret,
    /// 墨迹（`/Ink`，PDF 1.4）。
    Ink,
    /// 弹出窗口（`/Popup`，PDF 1.3）。
    Popup,
    /// 文件附件（`/FileAttachment`，PDF 1.3）。
    FileAttachment,
    /// 图章（`/Stamp`，PDF 1.3）。
    Stamp,
}

impl AnnotationKind {
    /// 标准子类型名（`/Subtype` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Link => "Link",
            Self::FreeText => "FreeText",
            Self::Line => "Line",
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Polygon => "Polygon",
            Self::PolyLine => "PolyLine",
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::Squiggly => "Squiggly",
            Self::StrikeOut => "StrikeOut",
            Self::Caret => "Caret",
            Self::Ink => "Ink",
            Self::Popup => "Popup",
            Self::FileAttachment => "FileAttachment",
            Self::Stamp => "Stamp",
        }
    }

    /// 该子类型引入的最低 PDF 版本。
    pub fn min_version(self) -> PdfVersion {
        match self {
            Self::Text
            | Self::Link
            | Self::FreeText
            | Self::Line
            | Self::Square
            | Self::Circle
            | Self::Polygon
            | Self::PolyLine => PdfVersion::V1_0,
            Self::Squiggly | Self::Ink => PdfVersion::V1_4,
            _ => PdfVersion::V1_3,
        }
    }

    /// 是否为标记类注解（Table 172，可携带 `/QuadPoints`；PDF 1.3 引入）。
    pub fn is_markup(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::FreeText
                | Self::Line
                | Self::Square
                | Self::Circle
                | Self::Polygon
                | Self::PolyLine
                | Self::Highlight
                | Self::Underline
                | Self::Squiggly
                | Self::StrikeOut
                | Self::Caret
                | Self::Ink
                | Self::Stamp
        )
    }
}

/// 注解边框样式（`/BS` 字典 `/S`，Table 162）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationBorderStyle {
    /// 实线。
    Solid,
    /// 虚线。
    Dashed,
    /// 斜角。
    Beveled,
    /// 内凹。
    Inset,
    /// 双线。
    Doubled,
}

impl AnnotationBorderStyle {
    /// 样式名（`/S` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "S",
            Self::Dashed => "D",
            Self::Beveled => "B",
            Self::Inset => "I",
            Self::Doubled => "C",
        }
    }
}

/// 注解目标（`/Dest`）：显式目标数组或命名目标。
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationDestination {
    /// 显式目标；页号在 assemble 时解析为 Page 字典引用。
    Explicit(Destination),
    /// 命名目标（写入 Names 树后由阅读器解析）。
    Named(String),
}

/// 注解字典的构建态（尚未分配对象号）。
#[derive(Debug, Clone)]
pub struct PdfAnnotation {
    kind: AnnotationKind,
    rect: [f64; 4],
    flags: u32,
    contents: Option<String>,
    title: Option<String>,
    name: Option<String>,
    modified_date: Option<String>,
    creation_date: Option<String>,
    color: Option<Vec<f64>>,
    interior_color: Option<Vec<f64>>,
    opacity: Option<f64>,
    blend_mode: Option<String>,
    border_width: Option<f64>,
    border_style: Option<AnnotationBorderStyle>,
    border_dash: Vec<f64>,
    quad_points: Option<Vec<f64>>,
    action: Option<Action>,
    destination: Option<AnnotationDestination>,
    popup: Option<usize>,
    appearance_normal: Option<usize>,
    appearance_down: Option<usize>,
    appearance_rollover: Option<usize>,
}

/// `/C`、`/IC` 颜色数组允许的分量数。
const ALLOWED_COLOR_LENGTHS: [usize; 3] = [1, 3, 4];

impl PdfAnnotation {
    /// 新建注解；`rect` 为 `[左下 x, 左下 y, 右上 x, 右上 y]`。
    pub fn new(kind: AnnotationKind, rect: [f64; 4]) -> Self {
        Self {
            kind,
            rect,
            flags: 0,
            contents: None,
            title: None,
            name: None,
            modified_date: None,
            creation_date: None,
            color: None,
            interior_color: None,
            opacity: None,
            blend_mode: None,
            border_width: None,
            border_style: None,
            border_dash: Vec::new(),
            quad_points: None,
            action: None,
            destination: None,
            popup: None,
            appearance_normal: None,
            appearance_down: None,
            appearance_rollover: None,
        }
    }

    /// 注解标志位（Table 165；如 1=无缩放、4=打印）。
    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    /// 正文内容 `/Contents`。
    pub fn with_contents(mut self, contents: impl Into<String>) -> Self {
        self.contents = Some(contents.into());
        self
    }

    /// 弹出窗口标题 `/T`。
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 注解名 `/NM`。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// 修改日期 `/M`（日期字符串，如 `D:20261027120000`）。
    pub fn with_modified_date(mut self, date: impl Into<String>) -> Self {
        self.modified_date = Some(date.into());
        self
    }

    /// 创建日期 `/CreationDate`（扩展条目；Widget/标注工具使用）。
    pub fn with_creation_date(mut self, date: impl Into<String>) -> Self {
        self.creation_date = Some(date.into());
        self
    }

    /// 边框/背景颜色 `/C`；分量数限 1/3/4，各分量须为有限数。
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = Some(color);
        self
    }

    /// 填充颜色 `/IC`（仅对几何与标记类注解有意义）。
    pub fn with_interior_color(mut self, color: Vec<f64>) -> Self {
        self.interior_color = Some(color);
        self
    }

    /// 注解不透明度 `/CA`，取值 `0.0..=1.0`（PDF 1.4）。
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// 混合模式 `/BM`（PDF 1.4 名称，如 `Multiply`）。
    pub fn with_blend_mode(mut self, mode: impl Into<String>) -> Self {
        self.blend_mode = Some(mode.into());
        self
    }

    /// 边框宽度（`/BS` 字典 `/W`，非负）。
    pub fn with_border_width(mut self, width: f64) -> Self {
        self.border_width = Some(width);
        self
    }

    /// 边框样式（`/BS` 字典 `/S`）。
    pub fn with_border_style(mut self, style: AnnotationBorderStyle) -> Self {
        self.border_style = Some(style);
        self
    }

    /// 虚线模式（`/BS` 字典 `/D`；至少两项）。
    pub fn with_border_dash(mut self, dash: Vec<f64>) -> Self {
        self.border_dash = dash;
        self
    }

    /// 高亮区域四边形 `/QuadPoints`；每四边形 8 个数，总长须为 8 的倍数。
    pub fn with_quad_points(mut self, quad_points: Vec<f64>) -> Self {
        self.quad_points = Some(quad_points);
        self
    }

    /// 触发动作 `/A`（内联动作字典；`GoTo` 的页号在 assemble 时解析）。
    pub fn with_action(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }

    /// 跳转目标 `/Dest`（与 `/A` 可同时存在；Link 注解二者取其一使用）。
    pub fn with_destination(mut self, destination: AnnotationDestination) -> Self {
        self.destination = Some(destination);
        self
    }

    /// 弹出窗口引用 `/Popup`；参数为另一注解的注册句柄。
    pub fn with_popup(mut self, handle: usize) -> Self {
        self.popup = Some(handle);
        self
    }

    /// 常态外观流（`/AP` 字典 `/N`）；参数为 [`FormXObject`](super::FormXObject) 句柄。
    pub fn with_appearance(mut self, handle: usize) -> Self {
        self.appearance_normal = Some(handle);
        self
    }

    /// 按下态外观流（`/AP` 字典 `/D`）；参数为 Form XObject 句柄。
    pub fn with_appearance_down(mut self, handle: usize) -> Self {
        self.appearance_down = Some(handle);
        self
    }

    /// 悬停态外观流（`/AP` 字典 `/R`）；参数为 Form XObject 句柄。
    pub fn with_appearance_rollover(mut self, handle: usize) -> Self {
        self.appearance_rollover = Some(handle);
        self
    }

    /// 子类型。
    pub fn kind(&self) -> AnnotationKind {
        self.kind
    }

    /// 矩形区域。
    pub fn rect(&self) -> [f64; 4] {
        self.rect
    }

    /// 标志位。
    pub(crate) fn flags(&self) -> u32 {
        self.flags
    }

    pub(crate) fn contents(&self) -> Option<&str> {
        self.contents.as_deref()
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) fn modified_date(&self) -> Option<&str> {
        self.modified_date.as_deref()
    }

    pub(crate) fn creation_date(&self) -> Option<&str> {
        self.creation_date.as_deref()
    }

    pub(crate) fn color(&self) -> Option<&[f64]> {
        self.color.as_deref()
    }

    pub(crate) fn interior_color(&self) -> Option<&[f64]> {
        self.interior_color.as_deref()
    }

    pub(crate) fn opacity(&self) -> Option<f64> {
        self.opacity
    }

    pub(crate) fn blend_mode(&self) -> Option<&str> {
        self.blend_mode.as_deref()
    }

    pub(crate) fn border_width(&self) -> Option<f64> {
        self.border_width
    }

    pub(crate) fn border_style(&self) -> Option<AnnotationBorderStyle> {
        self.border_style
    }

    pub(crate) fn border_dash(&self) -> &[f64] {
        &self.border_dash
    }

    pub(crate) fn quad_points(&self) -> Option<&[f64]> {
        self.quad_points.as_deref()
    }

    pub(crate) fn action(&self) -> Option<&Action> {
        self.action.as_ref()
    }

    pub(crate) fn destination(&self) -> Option<&AnnotationDestination> {
        self.destination.as_ref()
    }

    pub(crate) fn popup(&self) -> Option<usize> {
        self.popup
    }

    pub(crate) fn appearance_normal(&self) -> Option<usize> {
        self.appearance_normal
    }

    pub(crate) fn appearance_down(&self) -> Option<usize> {
        self.appearance_down
    }

    pub(crate) fn appearance_rollover(&self) -> Option<usize> {
        self.appearance_rollover
    }

    /// 是否声明了任意 `/AP` 外观流。
    pub(crate) fn has_appearance(&self) -> bool {
        self.appearance_normal.is_some()
            || self.appearance_down.is_some()
            || self.appearance_rollover.is_some()
    }

    /// 校验字典一致性；错误串直接用于 [`WriteError`](crate::core::writer::WriteError)。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.rect.iter().any(|v| !v.is_finite()) {
            return Err("annotation /Rect must contain four finite numbers".into());
        }
        if let Some(color) = &self.color {
            validate_color(color, "/C")?;
        }
        if let Some(color) = &self.interior_color {
            validate_color(color, "/IC")?;
        }
        if let Some(opacity) = self.opacity
            && !(0.0..=1.0).contains(&opacity)
        {
            return Err(format!(
                "annotation /CA must be within 0..=1, got {opacity}"
            ));
        }
        if let Some(mode) = &self.blend_mode
            && mode.is_empty()
        {
            return Err("annotation /BM must not be empty".into());
        }
        if let Some(width) = self.border_width
            && (!width.is_finite() || width < 0.0)
        {
            return Err("annotation /BS /W must be a finite nonnegative number".into());
        }
        if !self.border_dash.is_empty() {
            if self.border_dash.len() < 2 {
                return Err("annotation /BS /D dash pattern needs at least two entries".into());
            }
            if self.border_dash.iter().any(|v| !v.is_finite() || *v < 0.0) {
                return Err("annotation /BS /D entries must be finite nonnegative".into());
            }
        }
        if let Some(quad) = &self.quad_points {
            if quad.len() % 8 != 0 {
                return Err("annotation /QuadPoints length must be a multiple of 8".into());
            }
            if quad.iter().any(|v| !v.is_finite()) {
                return Err("annotation /QuadPoints entries must be finite".into());
            }
        }
        if let Some(AnnotationDestination::Named(name)) = &self.destination
            && name.is_empty()
        {
            return Err("annotation /Dest name must not be empty".into());
        }
        Ok(())
    }
}

fn validate_color(color: &[f64], key: &str) -> Result<(), String> {
    if !ALLOWED_COLOR_LENGTHS.contains(&color.len()) {
        return Err(format!(
            "annotation {key} must have 1, 3 or 4 components, got {}",
            color.len()
        ));
    }
    if color.iter().any(|v| !v.is_finite()) {
        return Err(format!("annotation {key} components must be finite"));
    }
    Ok(())
}
