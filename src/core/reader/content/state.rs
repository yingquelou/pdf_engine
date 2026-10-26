//! Content-stream graphics state machine (ROADMAP stage C, item C3).
//!
//! Interprets the operator stream produced by [`super::parse_content_stream`]
//! into the device-independent graphics state defined by ISO 32000-1 §8.4
//! (graphics state, CTM, stroke parameters, `q/Q` stack, `gs` ExtGState
//! application), §9.3 (text state), §9.4 (text object matrices) and §8.6
//! (current colour/pattern values).
//!
//! Model and data only: paths, clipping geometry, marked-content structure and
//! show-text results are not tracked here. The saved state pushed by `q`
//! covers every tracked component (including the text matrices), matching the
//! §8.4.5 statement that `q` saves the entire device-independent graphics
//! state. `BT` resets the text matrix and text line matrix to identity but
//! leaves the other text state intact (§9.4.1).

use crate::core::reader::graphics_state::{BlendMode, ExtGState};
use crate::core::reader::page_content::PageContentError;
use crate::core::types::Document;

use super::Op;

/// Maximum number of simultaneously saved graphics states (`q` without `Q`).
pub const MAX_SAVE_DEPTH: usize = 512;

/// Rendering intent names accepted by the `ri` operator (Table 58 `/RI`).
const RENDERING_INTENTS: [&str; 5] = [
    "RelativeColorimetric",
    "AbsoluteColorimetric",
    "Saturation",
    "Perceptual",
    "SelectByIntent",
];

/// Errors produced while interpreting a content stream into graphics states.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentStateError {
    /// An operator received an out-of-range or otherwise illegal value.
    InvalidValue { operator: String, message: String },
    /// The `q` save stack exceeded [`MAX_SAVE_DEPTH`].
    SaveStackTooDeep { limit: usize },
    /// A `gs` operand names a resource that is absent from `/Resources`.
    UnknownExtGState { name: String },
    /// A referenced ExtGState dictionary failed Table 58 validation.
    InvalidExtGState { name: String, message: String },
    /// The page content stream could not be obtained or parsed.
    PageContent(PageContentError),
}

impl std::fmt::Display for ContentStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidValue { operator, message } => {
                write!(f, "operator '{operator}' has an invalid value: {message}")
            }
            Self::SaveStackTooDeep { limit } => {
                write!(f, "graphics state save stack exceeded {limit}")
            }
            Self::UnknownExtGState { name } => {
                write!(f, "gs references unknown ExtGState resource '{name}'")
            }
            Self::InvalidExtGState { name, message } => {
                write!(f, "ExtGState resource '{name}' is invalid: {message}")
            }
            Self::PageContent(error) => write!(f, "page content error: {error}"),
        }
    }
}

impl std::error::Error for ContentStateError {}

impl From<PageContentError> for ContentStateError {
    fn from(error: PageContentError) -> Self {
        Self::PageContent(error)
    }
}

/// Current colour in one painting direction (§8.6.8).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorValue {
    /// Colour space resource name or device family for shorthand operators.
    pub space: Option<String>,
    /// Component values last set by `SC`/`sc`/`SCN`/`scn` or the shorthand
    /// gray/RGB/CMYK operators.
    pub components: Vec<f64>,
    /// Pattern resource name for `SCN`/`scn` with a trailing name (§8.7.3).
    pub pattern: Option<String>,
}

impl Default for ColorValue {
    fn default() -> Self {
        // Initial text/printing colour is device gray 0.0 (Table 57).
        Self {
            space: Some("DeviceGray".into()),
            components: vec![0.0],
            pattern: None,
        }
    }
}

/// Identity affine matrix in `[a b c d e f]` row-vector form.
pub const IDENTITY_MATRIX: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Concatenate two affine matrices, applying `first` before `second`
/// (row-vector convention: `result = first × second`).
pub fn concat_matrices(first: &[f64; 6], second: &[f64; 6]) -> [f64; 6] {
    let [a1, b1, c1, d1, e1, f1] = *first;
    let [a2, b2, c2, d2, e2, f2] = *second;
    [
        a1 * a2 + b1 * c2,
        a1 * b2 + b1 * d2,
        c1 * a2 + d1 * c2,
        c1 * b2 + d1 * d2,
        e1 * a2 + f1 * c2 + e2,
        e1 * b2 + f1 * d2 + f2,
    ]
}

/// The device-independent graphics state tracked across a content stream.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentGraphicsState {
    /// Current transformation matrix.
    pub ctm: [f64; 6],
    /// Line width (`w`, `/LW`).
    pub line_width: f64,
    /// Line cap style (`J`, `/LC`): 0 butt, 1 round, 2 square.
    pub line_cap: u8,
    /// Line join style (`j`, `/LJ`): 0 miter, 1 round, 2 bevel.
    pub line_join: u8,
    /// Miter limit (`M`, `/ML`).
    pub miter_limit: f64,
    /// Dash array (`d`, `/D` first element).
    pub dash_pattern: Vec<f64>,
    /// Dash phase (`d`, `/D` second element).
    pub dash_phase: f64,
    /// Rendering intent (`ri`, `/RI`).
    pub rendering_intent: String,
    /// Flatness tolerance (`i`, `/FL`).
    pub flatness: f64,
    /// Smoothness tolerance (`/SM`).
    pub smoothness: f64,
    /// Stroke overprint (`/OP`).
    pub stroke_overprint: bool,
    /// Fill overprint (`/op`).
    pub fill_overprint: bool,
    /// Overprint mode (`/OPM`).
    pub overprint_mode: u8,
    /// Stroke adjustment (`/SA`).
    pub stroke_adjustment: bool,
    /// Blend mode list (`/BM`), primary mode first.
    pub blend_mode: Vec<String>,
    /// Stroke alpha (`CA`).
    pub stroke_alpha: f64,
    /// Fill alpha (`ca`).
    pub fill_alpha: f64,
    /// Alpha is shape (`/AIS`).
    pub alpha_is_shape: bool,
    /// Text knockout (`/TK`).
    pub text_knockout: bool,

    /// Character spacing (`Tc`).
    pub char_spacing: f64,
    /// Word spacing (`Tw`).
    pub word_spacing: f64,
    /// Horizontal scaling factor (`Th`).
    pub horizontal_scaling: f64,
    /// Text leading (`TL`).
    pub leading: f64,
    /// Current font resource name and size (`Tf`).
    pub font: Option<(String, f64)>,
    /// Text rendering mode (`Tr`): 0 fill, 1 stroke, 2 both, 3 invisible.
    pub text_rendering_mode: u8,
    /// Text rise (`Ts`).
    pub text_rise: f64,
    /// Text matrix (Table 108), reset to identity by `BT`.
    pub text_matrix: [f64; 6],
    /// Text line matrix (Table 108), reset to identity by `BT`.
    pub text_line_matrix: [f64; 6],

    /// Current stroke colour value.
    pub stroke_color: ColorValue,
    /// Current fill colour value.
    pub fill_color: ColorValue,
}

impl Default for ContentGraphicsState {
    fn default() -> Self {
        Self {
            ctm: IDENTITY_MATRIX,
            line_width: 1.0,
            line_cap: 0,
            line_join: 0,
            miter_limit: 10.0,
            dash_pattern: Vec::new(),
            dash_phase: 0.0,
            rendering_intent: "RelativeColorimetric".into(),
            flatness: 1.0,
            smoothness: 0.0,
            stroke_overprint: false,
            fill_overprint: false,
            overprint_mode: 0,
            stroke_adjustment: true,
            blend_mode: vec!["Normal".into()],
            stroke_alpha: 1.0,
            fill_alpha: 1.0,
            alpha_is_shape: false,
            text_knockout: true,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 1.0,
            leading: 0.0,
            font: None,
            text_rendering_mode: 0,
            text_rise: 0.0,
            text_matrix: IDENTITY_MATRIX,
            text_line_matrix: IDENTITY_MATRIX,
            stroke_color: ColorValue::default(),
            fill_color: ColorValue::default(),
        }
    }
}

/// Resolves a `gs` operand name to its parsed ExtGState dictionary.
///
/// Returns `Ok(None)` when the resource is absent; the runner then reports
/// [`ContentStateError::UnknownExtGState`].
pub type ExtGStateResolver<'a> =
    &'a mut dyn FnMut(&str) -> std::result::Result<Option<ExtGState>, ContentStateError>;

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
                require_finite("Th", &[*v])?;
                if *v < 0.0 {
                    return Err(invalid("Th", "horizontal scaling must not be negative"));
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
                self.state.text_rendering_mode = require_enum("Tr", *mode, 3)?;
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

/// Run the state machine over operators, snapshotting the state after each.
pub fn run_content_state(
    ops: &[Op],
    resolve_ext_g_state: ExtGStateResolver<'_>,
) -> std::result::Result<Vec<ContentGraphicsState>, ContentStateError> {
    let mut runner = ContentStateRunner::new();
    let mut trace = Vec::with_capacity(ops.len());
    for op in ops {
        runner.apply(op, resolve_ext_g_state)?;
        trace.push(runner.state().clone());
    }
    Ok(trace)
}

impl Document {
    /// Apply the page content stream to the graphics state machine,
    /// returning the state snapshot after each operator.
    ///
    /// `gs` operands are resolved through the page `/Resources/ExtGState`
    /// dictionary (with `/Parent` inheritance) and parsed by
    /// [`Document::ext_g_state`].
    pub fn page_content_state_trace(
        &self,
        page_index: usize,
    ) -> std::result::Result<Vec<ContentGraphicsState>, ContentStateError> {
        let ops = self.page_content_ops(page_index)?;
        let mut resolver =
            |name: &str| -> std::result::Result<Option<ExtGState>, ContentStateError> {
                let value = self
                    .page_resource(page_index, "ExtGState", name)
                    .map_err(|_| ContentStateError::UnknownExtGState { name: name.into() })?;
                match value {
                    Some(value) => self.ext_g_state(&value).map(Some).map_err(|error| {
                        ContentStateError::InvalidExtGState {
                            name: name.into(),
                            message: error.to_string(),
                        }
                    }),
                    None => Ok(None),
                }
            };
        run_content_state(&ops, &mut resolver)
    }

    /// Final graphics state after applying the whole page content stream.
    pub fn page_content_state(
        &self,
        page_index: usize,
    ) -> std::result::Result<ContentGraphicsState, ContentStateError> {
        let trace = self.page_content_state_trace(page_index)?;
        Ok(trace.last().cloned().unwrap_or_default())
    }
}

fn invalid(operator: &str, message: &str) -> ContentStateError {
    ContentStateError::InvalidValue {
        operator: operator.into(),
        message: message.into(),
    }
}

fn require_finite(operator: &str, values: &[f64]) -> std::result::Result<(), ContentStateError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(invalid(operator, "values must be finite numbers"))
    }
}

fn require_enum(
    operator: &str,
    value: i64,
    maximum: i64,
) -> std::result::Result<u8, ContentStateError> {
    if value >= 0 && value <= maximum {
        Ok(value as u8)
    } else {
        Err(invalid(
            operator,
            &format!("value {value} outside 0..={maximum}"),
        ))
    }
}

fn set_device_color(
    color: &mut ColorValue,
    operator: &str,
    first: f64,
    space: &str,
) -> std::result::Result<(), ContentStateError> {
    require_finite(operator, &[first])?;
    if !(0.0..=1.0).contains(&first) {
        return Err(invalid(operator, "component must be between 0.0 and 1.0"));
    }
    color.space = Some(space.into());
    color.components = vec![first];
    color.pattern = None;
    Ok(())
}

fn push_device_components(
    color: &mut ColorValue,
    operator: &str,
    values: &[f64],
) -> std::result::Result<(), ContentStateError> {
    for v in values {
        require_finite(operator, &[*v])?;
        if !(0.0..=1.0).contains(v) {
            return Err(invalid(operator, "component must be between 0.0 and 1.0"));
        }
        color.components.push(*v);
    }
    Ok(())
}
