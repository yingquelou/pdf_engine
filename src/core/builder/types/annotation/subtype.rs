//! 注解子类型专有条目：构造方法、取值访问与构建期校验（12.5.6）。
//!
//! 字段本身声明在 [`PdfAnnotation`]（父模块），Line 专有条目按 [`LineEntries`]
//! 成组；子类型归属与取值面的约束集中在本文件，
//! 非法一律 `InvalidObject`，且全部校验发生在分配对象号之前。

use super::{AnnotationKind, PdfAnnotation};
use crate::core::types::PdfVersion;

/// Line 注解端点样式（`/LE`，Table 176）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndStyle {
    /// 方头（`/Square`）。
    Square,
    /// 圆头（`/Circle`）。
    Circle,
    /// 菱形（`/Diamond`）。
    Diamond,
    /// 开口箭头（`/OpenArrow`）。
    OpenArrow,
    /// 闭合箭头（`/ClosedArrow`）。
    ClosedArrow,
    /// 无端点（`/None`）。
    None,
    /// 平头（`/Butt`，PDF 1.5）。
    Butt,
    /// 末端反向开口箭头（`/ROpenArrow`，PDF 1.5）。
    ROpenArrow,
    /// 末端反向闭合箭头（`/RClosedArrow`，PDF 1.5）。
    RClosedArrow,
    /// 斜线（`/Slash`，PDF 1.6）。
    Slash,
}

impl LineEndStyle {
    /// 端点样式名（`/LE` 数组元素）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Diamond => "Diamond",
            Self::OpenArrow => "OpenArrow",
            Self::ClosedArrow => "ClosedArrow",
            Self::None => "None",
            Self::Butt => "Butt",
            Self::ROpenArrow => "ROpenArrow",
            Self::RClosedArrow => "RClosedArrow",
            Self::Slash => "Slash",
        }
    }

    /// 该样式引入的最低 PDF 版本（Table 176）。
    pub fn min_version(self) -> PdfVersion {
        match self {
            Self::Butt | Self::ROpenArrow | Self::RClosedArrow => PdfVersion::V1_5,
            Self::Slash => PdfVersion::V1_6,
            _ => PdfVersion::V1_4,
        }
    }
}

/// Line 注解意图（`/IT`，Table 175，PDF 1.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineIntent {
    /// 带箭头装饰的线（`/LineArrow`，缺省）。
    LineArrow,
    /// 线性标注，题注给出测量长度（`/LineDimension`）。
    LineDimension,
}

impl LineIntent {
    /// 意图名（`/IT` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::LineArrow => "LineArrow",
            Self::LineDimension => "LineDimension",
        }
    }
}

/// Line 注解题注位置（`/CP`，仅 `/Cap` 为真时有意义，PDF 1.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionPosition {
    /// 题注内联于端点连线的延长线上（`/Inline`，缺省）。
    Inline,
    /// 题注居中置于连线上方，与连线平行（`/Top`）。
    Top,
}

impl CaptionPosition {
    /// 位置名（`/CP` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Inline => "Inline",
            Self::Top => "Top",
        }
    }
}

/// Line 注解专有条目（12.5.6.13，Table 175）的构建态。
///
/// 端点 `/L` 是 Line 子类型的必填条目；`/IC` 填充色由通用条目
/// [`PdfAnnotation::with_interior_color`] 承担，不在此重复建模。
#[derive(Debug, Clone, Default)]
pub(crate) struct LineEntries {
    /// `/L`：`[x1, y1, x2, y2]`。
    endpoints: Option<[f64; 4]>,
    /// `/LE`：起点与终点端点样式（PDF 1.4）。
    endings: Option<[LineEndStyle; 2]>,
    /// `/LL`：引线长度（PDF 1.6）。
    leader_length: Option<f64>,
    /// `/LLE`：引线超出端点的延伸长度（PDF 1.6）。
    leader_extension: Option<f64>,
    /// `/Cap`：是否显示题注（PDF 1.6）。
    caption: Option<bool>,
    /// `/IT`：注解意图（PDF 1.6）。
    intent: Option<LineIntent>,
    /// `/LLO`：题注相对线中点的偏移（PDF 1.7）。
    leader_line_offset: Option<f64>,
    /// `/CP`：题注位置（PDF 1.7）。
    caption_position: Option<CaptionPosition>,
    /// `/CO`：题注相对 `/LLO` 位置的偏移（PDF 1.7）。
    caption_offset: Option<[f64; 2]>,
}

impl LineEntries {
    /// 端点 `/L`。
    pub(crate) fn endpoints(&self) -> Option<[f64; 4]> {
        self.endpoints
    }

    /// 端点样式 `/LE`。
    pub(crate) fn endings(&self) -> Option<[LineEndStyle; 2]> {
        self.endings
    }

    /// 引线长度 `/LL`。
    pub(crate) fn leader_length(&self) -> Option<f64> {
        self.leader_length
    }

    /// 引线延伸 `/LLE`。
    pub(crate) fn leader_extension(&self) -> Option<f64> {
        self.leader_extension
    }

    /// 题注开关 `/Cap`。
    pub(crate) fn caption(&self) -> Option<bool> {
        self.caption
    }

    /// 意图 `/IT`。
    pub(crate) fn intent(&self) -> Option<LineIntent> {
        self.intent
    }

    /// 题注偏移 `/LLO`。
    pub(crate) fn leader_line_offset(&self) -> Option<f64> {
        self.leader_line_offset
    }

    /// 题注位置 `/CP`。
    pub(crate) fn caption_position(&self) -> Option<CaptionPosition> {
        self.caption_position
    }

    /// 题注相对偏移 `/CO`。
    pub(crate) fn caption_offset(&self) -> Option<[f64; 2]> {
        self.caption_offset
    }

    /// 是否声明了端点条目（`/L`）。
    pub(crate) fn has_endpoints(&self) -> bool {
        self.endpoints.is_some()
    }

    /// 是否声明了任意 PDF 1.4 端点样式条目（`/LE`）。
    pub(crate) fn has_arrow_entries(&self) -> bool {
        self.endings.is_some()
    }

    /// 是否声明了任意 PDF 1.6 引线条目（`/LL`、`/LLE`、`/Cap`、`/IT`）。
    pub(crate) fn has_leader_entries(&self) -> bool {
        self.leader_length.is_some()
            || self.leader_extension.is_some()
            || self.caption.is_some()
            || self.intent.is_some()
    }

    /// 是否声明了任意 PDF 1.7 题注条目（`/LLO`、`/CP`、`/CO`）。
    pub(crate) fn has_caption_entries(&self) -> bool {
        self.leader_line_offset.is_some()
            || self.caption_position.is_some()
            || self.caption_offset.is_some()
    }

    /// 是否声明了任意 Line 独有条目（`/L`、引线与题注）；`/LE` 仅
    /// Line/PolyLine 可用，由 [`Self::has_arrow_entries`] 单独归属。
    fn has_line_only_entries(&self) -> bool {
        self.has_endpoints() || self.has_leader_entries() || self.has_caption_entries()
    }

    /// 校验取值与相互依赖（`/LLE` 需 `/LL`、题注条目需 `/Cap` 为真）。
    fn validate(&self) -> Result<(), String> {
        if let Some(endpoints) = self.endpoints
            && endpoints.iter().any(|v| !v.is_finite())
        {
            return Err("Line annotation /L must hold four finite numbers".into());
        }
        if let Some(length) = self.leader_length
            && (!length.is_finite() || length < 0.0)
        {
            return Err("Line annotation /LL must be a finite nonnegative number".into());
        }
        if self.leader_extension.is_some() && self.leader_length.is_none() {
            return Err("Line annotation /LLE requires /LL (leader length)".into());
        }
        if let Some(extension) = self.leader_extension
            && (!extension.is_finite() || extension < 0.0)
        {
            return Err("Line annotation /LLE must be a finite nonnegative number".into());
        }
        if let Some(offset) = self.leader_line_offset
            && (!offset.is_finite() || offset < 0.0)
        {
            return Err("Line annotation /LLO must be a finite nonnegative number".into());
        }
        if let Some(offset) = self.caption_offset
            && !offset.iter().all(|v| v.is_finite())
        {
            return Err("Line annotation /CO must hold two finite numbers".into());
        }
        let caption_on = self.caption == Some(true);
        if self.has_caption_entries() && !caption_on {
            return Err(
                "Line annotation /LLO, /CP and /CO are meaningful only when /Cap is true".into(),
            );
        }
        Ok(())
    }
}

impl PdfAnnotation {
    /// Text 注解图标名 `/Name`（仅 Text 子类型；取值由查看器定义，写成名字）。
    pub fn with_icon_name(mut self, name: impl Into<String>) -> Self {
        self.icon_name = Some(name.into());
        self
    }

    /// 气泡展开状态 `/Open`（Text 与 Popup 子类型；Table 172/183，缺省 false）。
    pub fn with_open(mut self, open: bool) -> Self {
        self.is_open = Some(open);
        self
    }

    /// Text 注解标记状态 `/State`（文本串；标准值随 `/StateModel` 而变，PDF 1.3）。
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// Text 注解状态模型 `/StateModel`（文本串，缺省 `Marked`，PDF 1.3）。
    pub fn with_state_model(mut self, model: impl Into<String>) -> Self {
        self.state_model = Some(model.into());
        self
    }

    /// Caret 注解符号 `/Symbol`（名字；标准值 `NewLine`、`Cross`）。
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Ink 注解路径 `/InkList`；每次调用追加一条 x/y 交替的路径（PDF 1.4）。
    pub fn with_ink_path(mut self, path: Vec<f64>) -> Self {
        self.ink_paths.push(path);
        self
    }

    /// Line 注解端点 `/L`（`[x1, y1, x2, y2]`；Line 子类型必填）。
    pub fn with_line_endpoints(mut self, endpoints: [f64; 4]) -> Self {
        self.line.endpoints = Some(endpoints);
        self
    }

    /// Line/PolyLine 注解两端端点样式（`/LE`，PDF 1.4；缺省 `[/None /None]`）。
    pub fn with_line_endings(mut self, start: LineEndStyle, end: LineEndStyle) -> Self {
        self.line.endings = Some([start, end]);
        self
    }

    /// Line 注解引线长度 `/LL`（PDF 1.6，非负；缺省 0）。
    pub fn with_leader_length(mut self, length: f64) -> Self {
        self.line.leader_length = Some(length);
        self
    }

    /// Line 注解引线超出端点的延伸长度 `/LLE`（PDF 1.6，非负；须提供 `/LL`）。
    pub fn with_leader_extension(mut self, extension: f64) -> Self {
        self.line.leader_extension = Some(extension);
        self
    }

    /// Line 注解是否显示题注 `/Cap`（PDF 1.6，缺省 false）。
    pub fn with_caption(mut self, caption: bool) -> Self {
        self.line.caption = Some(caption);
        self
    }

    /// Line 注解意图 `/IT`（PDF 1.6；缺省 `LineArrow`）。
    pub fn with_line_intent(mut self, intent: LineIntent) -> Self {
        self.line.intent = Some(intent);
        self
    }

    /// 题注相对线中点的偏移 `/LLO`（PDF 1.7，非负；仅 `/Cap` 为真时有意义）。
    pub fn with_leader_line_offset(mut self, offset: f64) -> Self {
        self.line.leader_line_offset = Some(offset);
        self
    }

    /// 题注位置 `/CP`（PDF 1.7；仅 `/Cap` 为真时有意义，缺省 `Inline`）。
    pub fn with_caption_position(mut self, position: CaptionPosition) -> Self {
        self.line.caption_position = Some(position);
        self
    }

    /// 题注相对 `/LLO` 位置的偏移 `/CO`（PDF 1.7；仅 `/Cap` 为真时有意义，缺省 `[0 0]`）。
    pub fn with_caption_offset(mut self, offset: [f64; 2]) -> Self {
        self.line.caption_offset = Some(offset);
        self
    }
}

impl PdfAnnotation {
    pub(crate) fn icon_name(&self) -> Option<&str> {
        self.icon_name.as_deref()
    }

    pub(crate) fn is_open(&self) -> Option<bool> {
        self.is_open
    }

    pub(crate) fn state(&self) -> Option<&str> {
        self.state.as_deref()
    }

    pub(crate) fn state_model(&self) -> Option<&str> {
        self.state_model.as_deref()
    }

    pub(crate) fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    pub(crate) fn ink_paths(&self) -> &[Vec<f64>] {
        &self.ink_paths
    }

    /// 是否声明了 Text 标记状态条目（`/State`、`/StateModel`，PDF 1.3）。
    pub(crate) fn has_markup_state(&self) -> bool {
        self.state.is_some() || self.state_model.is_some()
    }

    /// Line 注解专有条目（Table 175）。
    pub(crate) fn line(&self) -> &LineEntries {
        &self.line
    }

    /// 校验子类型专有条目的归属与取值；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_subtype_entries(&self) -> Result<(), String> {
        if self.icon_name.is_some() {
            self.require_kind(AnnotationKind::Text, "annotation /Name")?;
        }
        if self.is_open.is_some() {
            self.require_kind_in(
                &[AnnotationKind::Text, AnnotationKind::Popup],
                "annotation /Open",
            )?;
        }
        if self.popup_parent.is_some() {
            self.require_kind(AnnotationKind::Popup, "annotation /Parent")?;
        }
        if self.has_markup_state() {
            self.require_kind(AnnotationKind::Text, "/State and /StateModel")?;
        }
        if self.symbol.is_some() {
            self.require_kind(AnnotationKind::Caret, "/Symbol")?;
        }
        if !self.ink_paths.is_empty() {
            self.require_kind(AnnotationKind::Ink, "/InkList")?;
        }
        if self.line.has_line_only_entries() {
            self.require_kind(AnnotationKind::Line, "Line annotation entries")?;
        }
        if self.line.has_arrow_entries() {
            self.require_kind_in(
                &[AnnotationKind::Line, AnnotationKind::PolyLine],
                "annotation /LE (line endings)",
            )?;
        }
        if self.kind == AnnotationKind::Line && self.line.endpoints().is_none() {
            return Err("Line annotation requires /L (endpoint coordinates)".into());
        }
        self.line.validate()?;
        if let Some(icon) = &self.icon_name {
            validate_name_value(icon, "annotation /Name")?;
        }
        if let Some(symbol) = &self.symbol {
            validate_name_value(symbol, "annotation /Symbol")?;
        }
        for value in [&self.state, &self.state_model] {
            if let Some(text) = value
                && text.is_empty()
            {
                return Err("annotation /State and /StateModel must not be empty".into());
            }
        }
        for path in &self.ink_paths {
            if path.len() % 2 != 0 || path.is_empty() {
                return Err(
                    "annotation /InkList path must hold an even, nonzero number of values".into(),
                );
            }
            if path.iter().any(|v| !v.is_finite()) {
                return Err("annotation /InkList entries must be finite".into());
            }
        }
        self.validate_link_entries()?;
        self.validate_free_text_entries()?;
        self.validate_geometry_entries()?;
        self.validate_measure_entries()?;
        self.validate_redact_entries()?;
        self.validate_appearance_characteristics()?;
        // 文本标记注解的 `/QuadPoints` 是 Table 179 必填条目，空数组视同缺失。
        if matches!(
            self.kind,
            AnnotationKind::Highlight
                | AnnotationKind::Underline
                | AnnotationKind::Squiggly
                | AnnotationKind::StrikeOut
        ) && self.quad_points.as_ref().is_none_or(|quad| quad.is_empty())
        {
            return Err(format!(
                "{} annotation requires /QuadPoints (Table 179)",
                self.kind.name()
            ));
        }
        // 12.5.6.14：popup 注解不得带外观流或关联动作。
        if self.kind == AnnotationKind::Popup {
            if self.has_appearance() {
                return Err("Popup annotation shall not have an appearance stream (/AP)".into());
            }
            if self.action.is_some() {
                return Err("Popup annotation shall not have associated actions (/A)".into());
            }
        }
        Ok(())
    }
}

/// 名字型条目取值：非空且不含空白与 PDF 名字定界符。
pub(crate) fn validate_name_value(value: &str, context: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{context} must not be empty"));
    }
    if value
        .chars()
        .any(|c| c.is_whitespace() || "/[]()<>{}".contains(c) || c == '%')
    {
        return Err(format!("{context} contains name delimiters: {value}"));
    }
    Ok(())
}
