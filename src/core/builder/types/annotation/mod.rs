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
//! 的 `/OverlayText` `/DA` `/Q` `/Repeat` `/RO`）；`sound` 覆盖 Sound 注解的
//! `/Sound` 声音对象引用与 `/Name` 图标名（Table 185）；`three_d` 覆盖
//! 3D 注解的 `/3DD` `/3DV` `/3DA` `/3DI` `/3DB`（Table 298）；`measure` 覆盖 12.9 的
//! `/Measure` 测量字典（Table 261–263）；`mk` 覆盖外观特性字典 `/MK`（Table 189）
//! 与图标适配字典 `/IF`（Table 247）；`watermark` 覆盖 Watermark 的固定打印
//! 字典 `/FixedPrint`（Table 190/191）；`exdata` 覆盖 3D 标记注解的外部数据
//! 字典 `/ExData`（13.6.6 Table 313）；`markup` 覆盖标记类注解的 Table 170
//! 附加条目 `/Subj` `/IRT` `/RT`；`printer_mark` 覆盖 Printer's mark
//! 注解的 `/MN` 与 `/F`/外观契约（14.11.3 Table 362）；`trap_net` 覆盖
//! Trap 网络注解的跟踪条目与 `/F`/外观契约（14.11.6 Table 366），子类型
//! 归属守卫集中在 `guard`。

mod border;
mod exdata;
mod free_text;
mod geometry;
mod guard;
mod kind;
mod link;
mod markup;
mod measure;
mod mk;
mod movie;
mod printer_mark;
mod redact;
mod sound;
mod subtype;
mod three_d;
mod trap_net;
mod watermark;

mod action_entries;
mod appearance_entries;
mod border_entries;
mod color_entries;
mod object_refs;
mod quad_popup;
mod text_entries;
mod validation;

pub use border::BorderArray;
pub use exdata::Markup3D;
use free_text::FreeTextEntries;
pub use free_text::{BorderEffect, BorderEffectStyle, CalloutLine, FreeTextIntent};
use geometry::GeometryEntries;
pub use geometry::PolygonIntent;
pub use kind::AnnotationKind;
use link::LinkEntries;
pub use link::LinkHighlightMode;
pub use markup::ReplyType;
pub use measure::{Measure, NumberFormat, NumberFraction, NumberLabelOrder};
pub use mk::{AppearanceCharacteristics, IconFit, IconScaleWhen, IconScaling};
use printer_mark::PrinterMarkEntries;
use redact::RedactEntries;
use subtype::LineEntries;
pub(crate) use subtype::validate_name_value;
pub use subtype::{CaptionPosition, LineEndStyle, LineIntent, StampIcon};
use trap_net::TrapNetworkEntries;
pub use watermark::FixedPrint;

use super::Action;
use super::movie::{Movie, MoviePlayback};
use super::three_d::{ThreeDActivation, ThreeDViewSelection};
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
    printer_mark: Option<PrinterMarkEntries>,
    trap_network: Option<TrapNetworkEntries>,
    modified_date: Option<String>,
    creation_date: Option<String>,
    subject: Option<String>,
    in_reply_to: Option<usize>,
    reply_type: Option<ReplyType>,
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
    characteristics: Option<AppearanceCharacteristics>,
    fixed_print: Option<FixedPrint>,
    movie: Option<Movie>,
    movie_playback: Option<MoviePlayback>,
    measure: Option<Measure>,
    ex_data: Option<Markup3D>,
    sound: Option<usize>,
    three_d: Option<usize>,
    three_d_view: Option<ThreeDViewSelection>,
    three_d_activation: Option<ThreeDActivation>,
    three_d_interactive: Option<bool>,
    three_d_view_box: Option<Vec<f64>>,
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
            printer_mark: None,
            trap_network: None,
            modified_date: None,
            creation_date: None,
            subject: None,
            in_reply_to: None,
            reply_type: None,
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
            characteristics: None,
            fixed_print: None,
            movie: None,
            movie_playback: None,
            measure: None,
            ex_data: None,
            sound: None,
            three_d: None,
            three_d_view: None,
            three_d_activation: None,
            three_d_interactive: None,
            three_d_view_box: None,
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

    /// 子类型。
    pub fn kind(&self) -> AnnotationKind {
        self.kind
    }

    /// 矩形区域。
    pub fn rect(&self) -> [f64; 4] {
        self.rect
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
