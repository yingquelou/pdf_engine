//! `ContentStateRunner`：逐个算子把内容流操作施加到设备无关图形状态，
//! 覆盖 §8.4 图形状态与 CTM、§9.3/§9.4 文本状态与文本矩阵、§8.6.8 当前
//! 颜色以及 `gs` 的 ExtGState 应用。

use super::super::Op;
use super::MAX_SAVE_DEPTH;
use super::color::{ColorValue, push_device_components, set_device_color};
use super::error::{ContentStateError, invalid, require_enum, require_finite};
use super::model::{ContentGraphicsState, ExtGStateResolver, IDENTITY_MATRIX, concat_matrices};

use crate::core::reader::graphics_state::{BlendMode, ExtGState};

/// Rendering intent names accepted by the `ri` operator (Table 58 `/RI`).
const RENDERING_INTENTS: [&str; 5] = [
    "RelativeColorimetric",
    "AbsoluteColorimetric",
    "Saturation",
    "Perceptual",
    "SelectByIntent",
];

/// Incremental graphics state machine over a content stream operator list.
#[derive(Debug, Default)]
pub struct ContentStateRunner {
    state: ContentGraphicsState,
    stack: Vec<ContentGraphicsState>,
}

impl ContentStateRunner {
    /// A runner positioned at the initial graphics state (Table 57).
    pub fn new() -> Self {
        Self::default()
    }

    /// The state after all operators applied so far.
    pub fn state(&self) -> &ContentGraphicsState {
        &self.state
    }

    /// Apply one operator, validating parameter ranges.
    pub fn apply(
        &mut self,
        op: &Op,
        resolve_ext_g_state: ExtGStateResolver<'_>,
    ) -> std::result::Result<(), ContentStateError> {
        match op {
            Op::SaveState => {
                if self.stack.len() >= MAX_SAVE_DEPTH {
                    return Err(ContentStateError::SaveStackTooDeep {
                        limit: MAX_SAVE_DEPTH,
                    });
                }
                self.stack.push(self.state.clone());
            }
            // ISO 32000-1 Table 56: Q with an empty stack has no effect.
            Op::RestoreState => {
                if let Some(saved) = self.stack.pop() {
                    self.state = saved;
                }
            }
            Op::ConcatMatrix(m) => {
                require_finite("cm", m)?;
                self.state.ctm = concat_matrices(m, &self.state.ctm);
            }
            Op::SetLineWidth(w) => {
                require_finite("w", &[*w])?;
                if *w < 0.0 {
                    return Err(invalid("w", "line width must not be negative"));
                }
                self.state.line_width = *w;
            }
            Op::SetLineCap(c) => {
                self.state.line_cap = require_enum("J", *c, 2)?;
            }
            Op::SetLineJoin(j) => {
                self.state.line_join = require_enum("j", *j, 3)?;
            }
            Op::SetMiterLimit(m) => {
                require_finite("M", &[*m])?;
                if *m < 1.0 {
                    return Err(invalid("M", "miter limit must be at least 1.0"));
                }
                self.state.miter_limit = *m;
            }
            Op::SetDashPattern(array, phase) => {
                require_finite("d", array)?;
                require_finite("d", &[*phase])?;
                if array.iter().any(|v| *v < 0.0) || *phase < 0.0 {
                    return Err(invalid("d", "dash lengths and phase must be non-negative"));
                }
                self.state.dash_pattern = array.clone();
                self.state.dash_phase = *phase;
            }
            Op::SetRenderingIntent(name) => {
                if !RENDERING_INTENTS.contains(&name.as_str()) {
                    return Err(invalid("ri", &format!("unknown rendering intent '{name}'")));
                }
                self.state.rendering_intent = name.clone();
            }
            Op::SetFlatness(f) => {
                require_finite("i", &[*f])?;
                if *f < 0.0 || *f > 100.0 {
                    return Err(invalid("i", "flatness must be between 0.0 and 100.0"));
                }
                self.state.flatness = *f;
            }
            Op::SetExtGState(name) => {
                let ext = resolve_ext_g_state(name)?
                    .ok_or_else(|| ContentStateError::UnknownExtGState { name: name.clone() })?;
                self.apply_ext_g_state(&ext)?;
            }

            // §9.4.1: BT resets the text and line matrices, not the text state.
            Op::BeginText => {
                self.state.text_matrix = IDENTITY_MATRIX;
                self.state.text_line_matrix = IDENTITY_MATRIX;
            }
            Op::EndText => {}

            Op::SetCharSpacing(v) => {
                require_finite("Tc", &[*v])?;
                self.state.char_spacing = *v;
            }
            Op::SetWordSpacing(v) => {
                require_finite("Tw", &[*v])?;
                self.state.word_spacing = *v;
            }
            Op::SetHorizontalScaling(v) => {
                require_finite("Tz", &[*v])?;
                if *v < 0.0 {
                    return Err(invalid("Tz", "horizontal scaling must not be negative"));
                }
                self.state.horizontal_scaling = *v;
            }
            Op::SetTextLeading(v) => {
                require_finite("TL", &[*v])?;
                self.state.leading = *v;
            }
            Op::SetFont(name, size) => {
                require_finite("Tf", &[*size])?;
                if *size <= 0.0 {
                    return Err(invalid("Tf", "font size must be positive"));
                }
                self.state.font = Some((name.clone(), *size));
            }
            Op::SetTextRenderingMode(mode) => {
                self.state.text_rendering_mode = require_enum("Tr", *mode, 7)?;
            }
            Op::SetTextRise(v) => {
                require_finite("Ts", &[*v])?;
                self.state.text_rise = *v;
            }

            Op::MoveTextPosition(tx, ty) => self.move_text_line(*tx, *ty)?,
            Op::MoveTextPositionAndSetLeading(tx, ty) => {
                require_finite("TD", &[*tx, *ty])?;
                self.state.leading = -*ty;
                self.move_text_line(*tx, *ty)?;
            }
            Op::SetTextMatrix(m) => {
                require_finite("Tm", m)?;
                self.state.text_matrix = *m;
                self.state.text_line_matrix = *m;
            }
            Op::NextLine => {
                let leading = self.state.leading;
                self.move_text_line(0.0, -leading)?;
            }

            // Show-text operators update the line matrix like T* (§9.4.3).
            Op::NextLineAndShowText(_) => {
                let leading = self.state.leading;
                self.move_text_line(0.0, -leading)?;
            }
            Op::SetSpacingAndShowText(aw, ac, _) => {
                require_finite("\"", &[*aw, *ac])?;
                self.state.word_spacing = *aw;
                self.state.char_spacing = *ac;
                let leading = self.state.leading;
                self.move_text_line(0.0, -leading)?;
            }
            Op::ShowText(_) | Op::ShowTextWithPositioning(_) => {}

            // Colour (§8.6.8).
            Op::SetStrokeColorSpace(name) => {
                self.state.stroke_color = ColorValue {
                    space: Some(name.clone()),
                    components: Vec::new(),
                    pattern: None,
                };
            }
            Op::SetFillColorSpace(name) => {
                self.state.fill_color = ColorValue {
                    space: Some(name.clone()),
                    components: Vec::new(),
                    pattern: None,
                };
            }
            Op::SetStrokeColor(components) => {
                require_finite("SC", components)?;
                self.state.stroke_color.components = components.clone();
                self.state.stroke_color.pattern = None;
            }
            Op::SetStrokeColorN(components) => {
                require_finite("SCN", components)?;
                self.state.stroke_color.components = components.clone();
                self.state.stroke_color.pattern = None;
            }
            Op::SetFillColor(components) => {
                require_finite("sc", components)?;
                self.state.fill_color.components = components.clone();
                self.state.fill_color.pattern = None;
            }
            Op::SetFillColorN(components) => {
                require_finite("scn", components)?;
                self.state.fill_color.components = components.clone();
                self.state.fill_color.pattern = None;
            }
            Op::SetStrokeColorPattern(pattern) => {
                self.state.stroke_color = ColorValue {
                    space: self.state.stroke_color.space.clone(),
                    components: Vec::new(),
                    pattern: Some(pattern.clone()),
                };
            }
            Op::SetStrokeColorPatternWithComponents(components, pattern) => {
                require_finite("SCN", components)?;
                self.state.stroke_color = ColorValue {
                    space: self.state.stroke_color.space.clone(),
                    components: components.clone(),
                    pattern: Some(pattern.clone()),
                };
            }
            Op::SetFillColorPattern(pattern) => {
                self.state.fill_color = ColorValue {
                    space: self.state.fill_color.space.clone(),
                    components: Vec::new(),
                    pattern: Some(pattern.clone()),
                };
            }
            Op::SetFillColorPatternWithComponents(components, pattern) => {
                require_finite("scn", components)?;
                self.state.fill_color = ColorValue {
                    space: self.state.fill_color.space.clone(),
                    components: components.clone(),
                    pattern: Some(pattern.clone()),
                };
            }
            Op::SetStrokeGray(v) => {
                set_device_color(&mut self.state.stroke_color, "G", *v, "DeviceGray")?;
            }
            Op::SetFillGray(v) => {
                set_device_color(&mut self.state.fill_color, "g", *v, "DeviceGray")?;
            }
            Op::SetStrokeRgb(r, g, b) => {
                set_device_color(&mut self.state.stroke_color, "RG", *r, "DeviceRGB")?;
                push_device_components(&mut self.state.stroke_color, "RG", &[*g, *b])?;
            }
            Op::SetFillRgb(r, g, b) => {
                set_device_color(&mut self.state.fill_color, "rg", *r, "DeviceRGB")?;
                push_device_components(&mut self.state.fill_color, "rg", &[*g, *b])?;
            }
            Op::SetStrokeCmyk(c, m, y, k) => {
                set_device_color(&mut self.state.stroke_color, "K", *c, "DeviceCMYK")?;
                push_device_components(&mut self.state.stroke_color, "K", &[*m, *y, *k])?;
            }
            Op::SetFillCmyk(c, m, y, k) => {
                set_device_color(&mut self.state.fill_color, "k", *c, "DeviceCMYK")?;
                push_device_components(&mut self.state.fill_color, "k", &[*m, *y, *k])?;
            }

            // Operators that do not alter the device-independent state in
            // this model: path construction/painting, clipping, inline
            // images, XObjects, shading, Type 3 glyph definitions, marked
            // content, compatibility sections and unmodeled operators.
            Op::MoveTo(..)
            | Op::LineTo(..)
            | Op::CurveTo(..)
            | Op::CurveToReplicateInitial(..)
            | Op::CurveToReplicateFinal(..)
            | Op::CloseSubpath
            | Op::Rectangle(..)
            | Op::Stroke
            | Op::CloseAndStroke
            | Op::Fill
            | Op::FillObsolete
            | Op::FillEvenOdd
            | Op::FillAndStroke
            | Op::FillAndStrokeEvenOdd
            | Op::CloseFillAndStroke
            | Op::CloseFillAndStrokeEvenOdd
            | Op::EndPath
            | Op::Clip
            | Op::ClipEvenOdd
            | Op::PaintShading(_)
            | Op::InlineImage(_)
            | Op::PaintXObject(_)
            | Op::SetGlyphWidth(..)
            | Op::SetGlyphWidthAndBBox(..)
            | Op::MarkedContentPoint(_)
            | Op::MarkedContentPointWithProperties(..)
            | Op::BeginMarkedContent(_)
            | Op::BeginMarkedContentWithProperties(..)
            | Op::EndMarkedContent
            | Op::BeginCompatibility
            | Op::EndCompatibility
            | Op::Unmodeled { .. } => {}
        }
        Ok(())
    }

    fn move_text_line(&mut self, tx: f64, ty: f64) -> std::result::Result<(), ContentStateError> {
        require_finite("Td", &[tx, ty])?;
        self.state.text_line_matrix =
            concat_matrices(&[1.0, 0.0, 0.0, 1.0, tx, ty], &self.state.text_line_matrix);
        self.state.text_matrix = self.state.text_line_matrix;
        Ok(())
    }

    fn apply_ext_g_state(&mut self, ext: &ExtGState) -> std::result::Result<(), ContentStateError> {
        if let Some(v) = ext.line_width {
            self.state.line_width = v;
        }
        if let Some(v) = ext.line_cap {
            self.state.line_cap = v;
        }
        if let Some(v) = ext.line_join {
            self.state.line_join = v;
        }
        if let Some(v) = ext.miter_limit {
            self.state.miter_limit = v;
        }
        if let Some(d) = &ext.dash_pattern {
            self.state.dash_pattern = d.array.clone();
            self.state.dash_phase = d.phase;
        }
        if let Some(v) = &ext.rendering_intent {
            self.state.rendering_intent = v.clone();
        }
        if let Some(v) = ext.flatness {
            self.state.flatness = v;
        }
        if let Some(v) = ext.smoothness {
            self.state.smoothness = v;
        }
        if let Some(v) = ext.overprint {
            self.state.stroke_overprint = v;
        }
        if let Some(v) = ext.nonstroking_overprint {
            self.state.fill_overprint = v;
        }
        if let Some(v) = ext.overprint_mode {
            self.state.overprint_mode = v;
        }
        if let Some(v) = ext.stroke_adjustment {
            self.state.stroke_adjustment = v;
        }
        if let Some(v) = &ext.blend_mode {
            self.state.blend_mode = match v {
                BlendMode::Name(name) => vec![name.clone()],
                BlendMode::Array(list) => list.clone(),
            };
        }
        if let Some(v) = ext.stroke_alpha {
            self.state.stroke_alpha = v;
        }
        if let Some(v) = ext.fill_alpha {
            self.state.fill_alpha = v;
        }
        if let Some(v) = ext.alpha_is_shape {
            self.state.alpha_is_shape = v;
        }
        if let Some(v) = ext.text_knockout {
            self.state.text_knockout = v;
        }
        // Table 58 `/Font`: the first element is normally an indirect
        // reference to the font object; only a direct name can be mapped to a
        // resource name here, references keep the previously selected font.
        if let Some(font_entry) = &ext.font
            && let crate::core::types::Value::Name(name) = &font_entry.font
        {
            self.state.font = Some((name.clone(), font_entry.size));
        }
        Ok(())
    }
}
