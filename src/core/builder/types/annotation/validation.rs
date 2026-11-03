//! 注解字典通用条目的构建期校验（Table 165/166/169）：`/Rect`、`/C` 与
//! `/IC` 颜色、`/CA` 透明度、`/BM` 混合模式、`/BS` 与 `/Border` 边框、
//! `/QuadPoints`、`/AP` 状态子字典与 `/AS`、`/A` 与 `/Dest` 互斥、
//! FileAttachment 的 `/FS` 必填、`/ExData` 归属；子类型专有条目与打印机
//! 标记、Trap 网络条目交由各子模块继续校验。

use super::{AnnotationDestination, AnnotationKind, PdfAnnotation};
use super::{printer_mark, subtype, trap_net, validate_color};

impl PdfAnnotation {
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
        // `/ExData` 列在标记类注解的附加条目（Table 170）；目标 3D 注解的
        // 归属与范围由 assemble 阶段跨注解校验。
        if let Some(markup) = &self.ex_data {
            if !self.kind.is_markup() {
                return Err(
                    "annotation /ExData is only permitted on markup annotations (Table 170)".into(),
                );
            }
            markup.validate()?;
        }
        self.validate_subtype_entries()?;
        self.validate_markup_universal_entries()?;
        printer_mark::validate(self)?;
        trap_net::validate(self)?;
        Ok(())
    }
}
