//! Redact 注解专有条目：叠加文本 `/OverlayText`、外观串 `/DA`、对齐 `/Q`、
//! 平铺 `/Repeat` 与叠加外观 `/RO` 的构建态与校验（12.5.6.23，Table 192）。
//!
//! Table 192 的 `/QuadPoints`、`/IC` 与其他子类型共用通用条目，仍由
//! [`PdfAnnotation::with_quad_points`] 与 [`PdfAnnotation::with_interior_color`]
//! 设置；本模块只对 Redact 上的取值增补约束（`/IC` 须是 0.0–1.0 的三分量
//! DeviceRGB）。子类型自 PDF 1.7 引入，条目随 [`AnnotationKind::Redact`]
//! 的版本门禁一并生效。

use super::{AnnotationKind, PdfAnnotation};

/// Redact 注解专有条目（Table 192）的构建态。
#[derive(Debug, Clone, Default)]
pub(crate) struct RedactEntries {
    /// `/OverlayText`：涂消后叠加的文本。
    overlay_text: Option<String>,
    /// `/DA`：叠加文本的外观串（`/OverlayText` 存在时必填）。
    appearance_string: Option<String>,
    /// `/Q`：叠加文本对齐方式（0 左、1 居中、2 右）。
    quadding: Option<u8>,
    /// `/Repeat`：叠加文本是否平铺填满涂消区。
    repeat: Option<bool>,
    /// `/RO`：叠加外观 Form XObject 句柄。
    overlay_form: Option<usize>,
}

impl RedactEntries {
    /// 叠加文本 `/OverlayText`。
    pub(crate) fn overlay_text(&self) -> Option<&str> {
        self.overlay_text.as_deref()
    }

    /// 外观串 `/DA`。
    pub(crate) fn appearance_string(&self) -> Option<&str> {
        self.appearance_string.as_deref()
    }

    /// 对齐方式 `/Q`。
    pub(crate) fn quadding(&self) -> Option<u8> {
        self.quadding
    }

    /// 平铺标志 `/Repeat`。
    pub(crate) fn repeat(&self) -> Option<bool> {
        self.repeat
    }

    /// 叠加外观 Form XObject 句柄 `/RO`。
    pub(crate) fn overlay_form(&self) -> Option<usize> {
        self.overlay_form
    }

    /// 是否声明了 `/RO` 之外的任一条目（`/RO` 存在时它们按 Table 192 被忽略）。
    fn has_text_entries(&self) -> bool {
        self.overlay_text.is_some() || self.appearance_string.is_some() || self.repeat.is_some()
    }

    /// 校验取值：`/OverlayText` 必带 `/DA`（Table 192），`/Q` 限 0–2，
    /// `/RO` 与按 Table 192 被它忽略的条目互斥。
    fn validate(&self) -> Result<(), String> {
        if self.overlay_text.is_some() && self.appearance_string.is_none() {
            return Err("Redact annotation /OverlayText requires /DA (Table 192)".into());
        }
        if let Some(quadding) = self.quadding
            && !(0..=2).contains(&quadding)
        {
            return Err(format!(
                "Redact annotation /Q must be 0, 1 or 2, got {quadding}"
            ));
        }
        if self.overlay_form.is_some() && self.has_text_entries() {
            return Err(
                "Redact annotation /RO takes precedence over /OverlayText, /DA, /Q and /Repeat \
                 (Table 192)"
                    .into(),
            );
        }
        Ok(())
    }
}

impl PdfAnnotation {
    /// Redact 叠加文本 `/OverlayText`（须同时给出
    /// [`with_redact_appearance_string`](Self::with_redact_appearance_string)）。
    pub fn with_redact_overlay_text(mut self, text: impl Into<String>) -> Self {
        self.redact.overlay_text = Some(text.into());
        self
    }

    /// Redact 叠加文本外观串 `/DA`（12.7.3.3 的变量文本格式）。
    pub fn with_redact_appearance_string(mut self, appearance: impl Into<String>) -> Self {
        self.redact.appearance_string = Some(appearance.into());
        self
    }

    /// Redact 叠加文本对齐 `/Q`（0 左、1 居中、2 右；缺省 0）。
    pub fn with_redact_quadding(mut self, quadding: u8) -> Self {
        self.redact.quadding = Some(quadding);
        self
    }

    /// Redact 叠加文本是否平铺填满涂消区 `/Repeat`（缺省 false）。
    pub fn with_redact_overlay_repeated(mut self, repeat: bool) -> Self {
        self.redact.repeat = Some(repeat);
        self
    }

    /// Redact 叠加外观 `/RO`：引用 [`PdfBuilder::form_xobject`](super::super::PdfBuilder)
    /// 注册的 Form XObject 句柄。
    pub fn with_redact_overlay_appearance(mut self, form: usize) -> Self {
        self.redact.overlay_form = Some(form);
        self
    }
}

impl PdfAnnotation {
    /// Redact 注解专有条目（Table 192）。
    pub(crate) fn redact(&self) -> &RedactEntries {
        &self.redact
    }

    /// Redact 专有条目的子类型归属与取值校验；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_redact_entries(&self) -> Result<(), String> {
        let declared = self.redact.has_text_entries()
            || self.redact.quadding.is_some()
            || self.redact.overlay_form.is_some();
        if declared {
            self.require_kind(
                AnnotationKind::Redact,
                "Redact annotation /OverlayText, /DA, /Q, /Repeat and /RO",
            )?;
        }
        if self.kind == AnnotationKind::Redact
            && let Some(color) = &self.interior_color
        {
            // Table 192 把 Redact 的 `/IC` 限定为 DeviceRGB 三分量、各在 0.0–1.0。
            if color.len() != 3 {
                return Err(format!(
                    "Redact annotation /IC must have 3 DeviceRGB components, got {}",
                    color.len()
                ));
            }
            if color.iter().any(|v| !(0.0..=1.0).contains(v)) {
                return Err("Redact annotation /IC components must be within 0.0..=1.0".into());
            }
        }
        self.redact.validate()
    }
}
