//! 注解字典写端（ISO 32000-1 12.5.1–12.5.5）。
//!
//! [`PdfAnnotation`] 以链式构造覆盖注解字典的通用条目（Table 165/166/169）：
//! 子类型、`/Rect`、标志、文本条目、颜色、透明度、边框样式（`/BS`）、
//! `/QuadPoints`、动作 `/A`、目标 `/Dest`、外观流 `/AP`（N/D/R）、
//! `/Popup` 句柄引用与 Popup 父注解 `/Parent` 句柄引用（Table 183）。
//! 经 [`PdfBuilder::add_annotation`](super::super::PdfBuilder::add_annotation)
//! 绑定到页面后，由 assemble 写为独立间接对象并登记进页面 `/Annots`。
//! 子类型专有条目（12.5.6）的构造方法与构建期校验在 `subtype`（Text/Caret/
//! Ink/Line）、`free_text`（FreeText，Table 174）、`geometry`
//! （Polygon/PolyLine，Table 178；`/BE` `/RD` `/LE` 的跨子类型归属也在此校准）
//! 、`link`（Link，Table 173 的 `/H` `/PA`）与 `redact`（Redact，Table 192
//! 的 `/OverlayText` `/DA` `/Q` `/Repeat` `/RO`）；`measure` 覆盖 12.9 的
//! `/Measure` 测量字典（Table 261–263）。

mod border;
mod free_text;
mod geometry;
mod kind;
mod link;
mod measure;
mod redact;
mod subtype;

pub use border::BorderArray;
use free_text::FreeTextEntries;
pub use free_text::{BorderEffect, BorderEffectStyle, CalloutLine, FreeTextIntent};
use geometry::GeometryEntries;
pub use geometry::PolygonIntent;
pub use kind::AnnotationKind;
use link::LinkEntries;
pub use link::LinkHighlightMode;
pub use measure::{Measure, NumberFormat, NumberFraction, NumberLabelOrder};
use redact::RedactEntries;
use subtype::LineEntries;
pub(crate) use subtype::validate_name_value;
pub use subtype::{CaptionPosition, LineEndStyle, LineIntent};

use super::Action;
use crate::core::reader::navigation::Destination;

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
    border_array: Option<BorderArray>,
    quad_points: Option<Vec<f64>>,
    icon_name: Option<String>,
    is_open: Option<bool>,
    state: Option<String>,
    state_model: Option<String>,
    symbol: Option<String>,
    ink_paths: Vec<Vec<f64>>,
    line: LineEntries,
    free_text: FreeTextEntries,
    geometry: GeometryEntries,
    link: LinkEntries,
    redact: RedactEntries,
    measure: Option<Measure>,
    action: Option<Action>,
    destination: Option<AnnotationDestination>,
    popup: Option<usize>,
    popup_parent: Option<usize>,
    appearance_normal: Option<usize>,
    appearance_down: Option<usize>,
    appearance_rollover: Option<usize>,
    appearance_states: Vec<(String, usize)>,
    appearance_state: Option<String>,
    filespec: Option<usize>,
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
            border_array: None,
            quad_points: None,
            icon_name: None,
            is_open: None,
            state: None,
            state_model: None,
            symbol: None,
            ink_paths: Vec::new(),
            line: LineEntries::default(),
            free_text: FreeTextEntries::default(),
            geometry: GeometryEntries::default(),
            link: LinkEntries::default(),
            redact: RedactEntries::default(),
            measure: None,
            action: None,
            destination: None,
            popup: None,
            popup_parent: None,
            appearance_normal: None,
            appearance_down: None,
            appearance_rollover: None,
            appearance_states: Vec::new(),
            appearance_state: None,
            filespec: None,
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

    /// 旧式边框数组（`/Border`，Table 164）；与 [`Self::with_border_width`]、
    /// [`Self::with_border_style`]、[`Self::with_border_dash`] 互斥。
    pub fn with_border_array(mut self, border: BorderArray) -> Self {
        self.border_array = Some(border);
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

    /// 跳转目标 `/Dest`；与 `/A` 互斥（Table 173 禁止 `/A` 存在时再写 `/Dest`）。
    pub fn with_destination(mut self, destination: AnnotationDestination) -> Self {
        self.destination = Some(destination);
        self
    }

    /// 弹出窗口引用 `/Popup`；参数为另一注解的注册句柄，该注解须是
    /// `Popup` 子类型。只声明本端即可：弹出框的 `/Parent` 由写出阶段自动
    /// 补出（12.5.2、Table 163/183）。
    pub fn with_popup(mut self, handle: usize) -> Self {
        self.popup = Some(handle);
        self
    }

    /// Popup 父注解 `/Parent`（Table 183，仅 Popup 子类型）；参数为另一注解的注册句柄。
    /// 只声明本端即可：父注解的 `/Popup` 由写出阶段自动补出。
    pub fn with_popup_parent(mut self, handle: usize) -> Self {
        self.popup_parent = Some(handle);
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

    /// 常态外观状态子字典 `/AP /N << /State … >>` 的一项（PDF 1.2）；
    /// 参数为状态名与该状态的 Form XObject 句柄。与 [`Self::with_appearance`]
    /// 互斥（`/N` 不能同时是流与字典）。
    pub fn with_appearance_state(mut self, state: impl Into<String>, handle: usize) -> Self {
        self.appearance_states.push((state.into(), handle));
        self
    }

    /// 选中的外观状态 `/AS`；须已在 `/AP /N` 状态子字典注册。
    pub fn with_selected_appearance_state(mut self, state: impl Into<String>) -> Self {
        self.appearance_state = Some(state.into());
        self
    }

    /// 附件文件规格 `/FS`（12.5.6.14，PDF 1.3）；参数为
    /// [`PdfBuilder::embed_file`](crate::core::builder::PdfBuilder::embed_file)
    /// 注册的嵌入文件句柄。FileAttachment 子类型必须提供。
    pub fn with_filespec(mut self, handle: usize) -> Self {
        self.filespec = Some(handle);
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

    pub(crate) fn border_array(&self) -> Option<&BorderArray> {
        self.border_array.as_ref()
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

    pub(crate) fn popup_parent(&self) -> Option<usize> {
        self.popup_parent
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

    pub(crate) fn appearance_states(&self) -> &[(String, usize)] {
        &self.appearance_states
    }

    pub(crate) fn appearance_state(&self) -> Option<&str> {
        self.appearance_state.as_deref()
    }

    pub(crate) fn filespec(&self) -> Option<usize> {
        self.filespec
    }

    /// 是否声明了任意 `/AP` 外观流。
    pub(crate) fn has_appearance(&self) -> bool {
        self.appearance_normal.is_some()
            || self.appearance_down.is_some()
            || self.appearance_rollover.is_some()
            || !self.appearance_states.is_empty()
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
        if let Some(border) = &self.border_array {
            if self.border_width.is_some()
                || self.border_style.is_some()
                || !self.border_dash.is_empty()
            {
                return Err(
                    "annotation /Border and /BS are mutually exclusive (/BS supersedes /Border)"
                        .into(),
                );
            }
            border.validate()?;
        }
        if let Some(quad) = &self.quad_points {
            if quad.len() % 8 != 0 {
                return Err("annotation /QuadPoints length must be a multiple of 8".into());
            }
            if quad.iter().any(|v| !v.is_finite()) {
                return Err("annotation /QuadPoints entries must be finite".into());
            }
            // `/QuadPoints` 属文本标记注解（Table 179）、Link（Table 173）
            // 与 Redact（Table 192）。
            self.require_kind_in(
                &[
                    AnnotationKind::Highlight,
                    AnnotationKind::Underline,
                    AnnotationKind::Squiggly,
                    AnnotationKind::StrikeOut,
                    AnnotationKind::Link,
                    AnnotationKind::Redact,
                ],
                "annotation /QuadPoints",
            )?;
        }
        // `/AP /N` 只能是流或状态子字典之一，状态名须唯一且合法。
        if !self.appearance_states.is_empty() {
            if self.appearance_normal.is_some() {
                return Err(
                    "annotation /AP /N cannot be both a stream and a state sub-dictionary".into(),
                );
            }
            for (index, (state, _)) in self.appearance_states.iter().enumerate() {
                subtype::validate_name_value(state, "annotation appearance state")?;
                if self.appearance_states[..index]
                    .iter()
                    .any(|(existing, _)| existing == state)
                {
                    return Err(format!(
                        "annotation appearance state {state} is registered twice"
                    ));
                }
            }
        }
        if let Some(state) = &self.appearance_state {
            if self.appearance_states.is_empty() {
                return Err("annotation /AS requires an /AP /N state sub-dictionary".into());
            }
            if !self.appearance_states.iter().any(|(name, _)| name == state) {
                return Err(format!(
                    "annotation /AS names unregistered appearance state {state}"
                ));
            }
        }
        if self.action.is_some() && self.destination.is_some() {
            return Err("annotation /Dest is not permitted when /A is present".into());
        }
        if let Some(AnnotationDestination::Named(name)) = &self.destination
            && name.is_empty()
        {
            return Err("annotation /Dest name must not be empty".into());
        }
        if self.kind == AnnotationKind::FileAttachment && self.filespec.is_none() {
            return Err("FileAttachment annotation requires /FS (embedded file handle)".into());
        }
        self.validate_subtype_entries()?;
        Ok(())
    }

    fn require_kind(&self, expected: AnnotationKind, entries: &str) -> Result<(), String> {
        if self.kind == expected {
            return Ok(());
        }
        Err(format!(
            "{entries} are specific to {} annotations, got {}",
            expected.name(),
            self.kind.name()
        ))
    }

    fn require_kind_in(&self, expected: &[AnnotationKind], entries: &str) -> Result<(), String> {
        if expected.contains(&self.kind) {
            return Ok(());
        }
        let names: Vec<&str> = expected.iter().map(|kind| kind.name()).collect();
        Err(format!(
            "{entries} are specific to {} annotations, got {}",
            names.join(" or "),
            self.kind.name()
        ))
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
