//! FreeText 注解专有条目：构建态、构造方法与构建期校验（12.5.6.6，Table 174）。
//!
//! `/DA` 是 Table 174 的必填条目；`/Q` PDF 1.4，`/RC` `/DS` PDF 1.5，
//! `/CL` `/IT` `/BE` `/RD` `/LE` PDF 1.6（`/BE` 结构见 Table 167）。
//! `/RC` 在表中可为文本串或文本流，写端只提供文本串形态。

use super::subtype::LineEndStyle;
use super::{AnnotationKind, PdfAnnotation};

/// FreeText 注解意图（`/IT`，Table 174，PDF 1.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeTextIntent {
    /// 普通文本框（`/FreeText`，缺省）。
    FreeText,
    /// 带引线标注的文本框（`/FreeTextCallout`，`/CL` 有意义）。
    FreeTextCallout,
    /// 点击即输/打字机式，不画引线（`/FreeTextTypeWriter`）。
    FreeTextTypeWriter,
}

impl FreeTextIntent {
    /// 意图名（`/IT` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::FreeText => "FreeText",
            Self::FreeTextCallout => "FreeTextCallout",
            Self::FreeTextTypeWriter => "FreeTextTypeWriter",
        }
    }
}

/// FreeText 引线（`/CL`，Table 174）：始终是 4 或 6 个坐标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalloutLine {
    /// 四点 `[x1, y1, x2, y2]`：起点与终点。
    Straight([f64; 4]),
    /// 六点 `[x1, y1, x2, y2, x3, y3]`：起点、拐点与终点。
    Knee([f64; 6]),
}

impl CalloutLine {
    /// `/CL` 数组的数值序列。
    pub fn values(&self) -> &[f64] {
        match self {
            Self::Straight(values) => values,
            Self::Knee(values) => values,
        }
    }
}

/// 边框效果样式（`/BE /S`，Table 167）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderEffectStyle {
    /// 无效果（`/S`，缺省）。
    NoEffect,
    /// 云状边框（`/C`），`/I` 仅在此样式下有效。
    Cloudy,
}

impl BorderEffectStyle {
    /// 样式名（`/BE /S` 值）。
    pub fn name(self) -> &'static str {
        match self {
            Self::NoEffect => "S",
            Self::Cloudy => "C",
        }
    }
}

/// 边框效果字典（`/BE`，Table 167）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderEffect {
    /// `/S` 样式。
    pub style: BorderEffectStyle,
    /// `/I` 强度（0–2），仅 [`BorderEffectStyle::Cloudy`] 可带。
    pub intensity: Option<f64>,
}

impl BorderEffect {
    /// 按样式构造；`NoEffect` 不带强度。
    pub fn new(style: BorderEffectStyle) -> Self {
        Self {
            style,
            intensity: None,
        }
    }

    /// 云状边框与强度（0–2）。
    pub fn cloudy(intensity: f64) -> Self {
        Self {
            style: BorderEffectStyle::Cloudy,
            intensity: Some(intensity),
        }
    }
}

/// FreeText 注解专有条目（12.5.6.6，Table 174）的构建态。
#[derive(Debug, Clone, Default)]
pub(crate) struct FreeTextEntries {
    /// `/DA`：默认外观串（必填）。
    default_appearance: Option<String>,
    /// `/RC`：富文本串（PDF 1.5）。
    rich_content: Option<String>,
    /// `/Q`：对齐方式（0 左、1 居中、2 右，PDF 1.4）。
    quadding: Option<u8>,
    /// `/DS`：富文本默认样式串（PDF 1.5）。
    default_style: Option<String>,
    /// `/CL`：引线（PDF 1.6）。
    callout_line: Option<CalloutLine>,
    /// `/IT`：意图（PDF 1.6）。
    intent: Option<FreeTextIntent>,
    /// `/BE`：边框效果（PDF 1.6）。
    border_effect: Option<BorderEffect>,
    /// `/RD`：内部文本矩形与 `/Rect` 的四边差值（PDF 1.6）。
    text_rectangle: Option<[f64; 4]>,
    /// `/LE`：引线端点样式（单个名字，PDF 1.6，需 `/CL`）。
    callout_ending: Option<LineEndStyle>,
}

impl FreeTextEntries {
    /// 默认外观串 `/DA`。
    pub(crate) fn default_appearance(&self) -> Option<&str> {
        self.default_appearance.as_deref()
    }

    /// 富文本串 `/RC`。
    pub(crate) fn rich_content(&self) -> Option<&str> {
        self.rich_content.as_deref()
    }

    /// 对齐方式 `/Q`。
    pub(crate) fn quadding(&self) -> Option<u8> {
        self.quadding
    }

    /// 富文本默认样式串 `/DS`。
    pub(crate) fn default_style(&self) -> Option<&str> {
        self.default_style.as_deref()
    }

    /// 引线 `/CL`。
    pub(crate) fn callout_line(&self) -> Option<CalloutLine> {
        self.callout_line
    }

    /// 意图 `/IT`。
    pub(crate) fn intent(&self) -> Option<FreeTextIntent> {
        self.intent
    }

    /// 边框效果 `/BE`。
    pub(crate) fn border_effect(&self) -> Option<BorderEffect> {
        self.border_effect
    }

    /// 内部文本矩形差值 `/RD`。
    pub(crate) fn text_rectangle(&self) -> Option<[f64; 4]> {
        self.text_rectangle
    }

    /// 引线端点样式 `/LE`。
    pub(crate) fn callout_ending(&self) -> Option<LineEndStyle> {
        self.callout_ending
    }

    /// 是否声明了 `/Q`（PDF 1.4）。
    pub(crate) fn has_quadding(&self) -> bool {
        self.quadding.is_some()
    }

    /// 是否声明了任一富文本条目（`/RC`、`/DS`，PDF 1.5）。
    pub(crate) fn has_rich_content(&self) -> bool {
        self.rich_content.is_some() || self.default_style.is_some()
    }

    /// 是否声明了 FreeText 独有的 PDF 1.6 条目（`/CL`、`/IT`、`/LE`）。
    pub(crate) fn has_callout_entries(&self) -> bool {
        self.callout_line.is_some() || self.intent.is_some() || self.callout_ending.is_some()
    }

    /// 是否声明了 FreeText 独有条目（`/DA` `/RC` `/Q` `/DS` `/CL` `/IT` `/LE`）。
    /// `/BE` 与 `/RD` 与几何类注解共用，不在此判定。
    fn has_free_text_entries(&self) -> bool {
        self.default_appearance.is_some()
            || self.has_quadding()
            || self.has_rich_content()
            || self.callout_line.is_some()
            || self.intent.is_some()
            || self.callout_ending.is_some()
    }

    /// 校验取值与相互依赖；`/DA` 的必填性由子类型归属处判断。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(da) = &self.default_appearance
            && da.trim().is_empty()
        {
            return Err("FreeText annotation /DA must not be empty".into());
        }
        if let Some(quadding) = self.quadding
            && quadding > 2
        {
            return Err(format!(
                "FreeText annotation /Q must be 0, 1 or 2, got {quadding}"
            ));
        }
        for (text, key) in [(&self.rich_content, "/RC"), (&self.default_style, "/DS")] {
            if let Some(value) = text
                && value.is_empty()
            {
                return Err(format!("FreeText annotation {key} must not be empty"));
            }
        }
        if let Some(line) = &self.callout_line
            && !line.values().iter().all(|v| v.is_finite())
        {
            return Err("FreeText annotation /CL must hold finite numbers".into());
        }
        if let Some(diff) = self.text_rectangle
            && (!diff.iter().all(|v| v.is_finite()) || diff.iter().any(|v| *v < 0.0))
        {
            return Err("annotation /RD must hold four finite nonnegative numbers".into());
        }
        if let Some(effect) = &self.border_effect
            && let Some(intensity) = effect.intensity
        {
            if !intensity.is_finite() || !(0.0..=2.0).contains(&intensity) {
                return Err(format!(
                    "annotation /BE /I must be within 0.0–2.0, got {intensity}"
                ));
            }
            if effect.style != BorderEffectStyle::Cloudy {
                return Err("annotation /BE /I is meaningful only with the cloudy style".into());
            }
        }
        if self.callout_ending.is_some() && self.callout_line.is_none() {
            return Err("FreeText annotation /LE requires /CL (callout line)".into());
        }
        Ok(())
    }
}

impl PdfAnnotation {
    /// FreeText 默认外观串 `/DA`（Table 174 必填，如 `"/F1 12 Tf 0 g"`）。
    pub fn with_default_appearance(mut self, default_appearance: impl Into<String>) -> Self {
        self.free_text.default_appearance = Some(default_appearance.into());
        self
    }

    /// FreeText 富文本串 `/RC`（PDF 1.5；由调用方提供 XML 文本）。
    pub fn with_rich_content(mut self, rich_content: impl Into<String>) -> Self {
        self.free_text.rich_content = Some(rich_content.into());
        self
    }

    /// FreeText 对齐方式 `/Q`（0 左、1 居中、2 右，PDF 1.4；缺省 0）。
    pub fn with_quadding(mut self, quadding: u8) -> Self {
        self.free_text.quadding = Some(quadding);
        self
    }

    /// FreeText 富文本默认样式串 `/DS`（PDF 1.5）。
    pub fn with_default_style(mut self, default_style: impl Into<String>) -> Self {
        self.free_text.default_style = Some(default_style.into());
        self
    }

    /// FreeText 引线 `/CL`（PDF 1.6；仅意图为 `FreeTextCallout` 时有意义）。
    pub fn with_callout_line(mut self, callout_line: CalloutLine) -> Self {
        self.free_text.callout_line = Some(callout_line);
        self
    }

    /// FreeText 意图 `/IT`（PDF 1.6；缺省 `FreeText`）。
    pub fn with_free_text_intent(mut self, intent: FreeTextIntent) -> Self {
        self.free_text.intent = Some(intent);
        self
    }

    /// FreeText 边框效果 `/BE`（PDF 1.6；结构见 Table 167）。
    pub fn with_border_effect(mut self, border_effect: BorderEffect) -> Self {
        self.free_text.border_effect = Some(border_effect);
        self
    }

    /// FreeText 内部文本矩形差值 `/RD`（PDF 1.6；左、上、右、下四边各 ≥0）。
    pub fn with_text_rectangle(mut self, text_rectangle: [f64; 4]) -> Self {
        self.free_text.text_rectangle = Some(text_rectangle);
        self
    }

    /// FreeText 引线端点样式 `/LE`（单个名字，PDF 1.6；须提供 `/CL`）。
    pub fn with_callout_ending(mut self, callout_ending: LineEndStyle) -> Self {
        self.free_text.callout_ending = Some(callout_ending);
        self
    }
}

impl PdfAnnotation {
    /// FreeText 注解专有条目（Table 174）。
    pub(crate) fn free_text(&self) -> &FreeTextEntries {
        &self.free_text
    }

    /// FreeText 独有条目与共享边框条目的子类型归属、`/DA` 必填项校验；
    /// 由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_free_text_entries(&self) -> Result<(), String> {
        if self.free_text.has_free_text_entries() {
            self.require_kind(AnnotationKind::FreeText, "FreeText annotation entries")?;
        }
        if self.free_text.border_effect().is_some() {
            self.require_kind_in(
                &[
                    AnnotationKind::FreeText,
                    AnnotationKind::Square,
                    AnnotationKind::Circle,
                    AnnotationKind::Polygon,
                ],
                "annotation /BE (border effect)",
            )?;
        }
        if self.free_text.text_rectangle().is_some() {
            self.require_kind_in(
                &[
                    AnnotationKind::FreeText,
                    AnnotationKind::Square,
                    AnnotationKind::Circle,
                ],
                "annotation /RD (rect differences)",
            )?;
        }
        if self.kind == AnnotationKind::FreeText && self.free_text.default_appearance().is_none() {
            return Err("FreeText annotation requires /DA (default appearance)".into());
        }
        self.free_text.validate()
    }
}
