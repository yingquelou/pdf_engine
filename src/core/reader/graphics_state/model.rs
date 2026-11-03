use super::{BTreeMap, Value};

/// Dash pattern (`/D`): alternating dash/gap lengths and phase.
#[derive(Debug, Clone, PartialEq)]
pub struct DashPattern {
    /// Dash array; non-negative lengths in user space units.
    pub array: Vec<f64>,
    /// Dash phase.
    pub phase: f64,
}

/// Font entry (`/Font`): font resource plus text state size.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtGStateFont {
    /// First `/Font` array element as written (usually an indirect reference).
    ///
    /// Kept unresolved so the caller can decide how to resolve the font
    /// resource; the element is validated to resolve to a dictionary.
    pub font: Value,
    /// Font size in text space units; always positive.
    pub size: f64,
}

/// Blend mode (`/BM`): a single name or an ordered fallback list.
#[derive(Debug, Clone, PartialEq)]
pub enum BlendMode {
    /// Single blend mode name.
    Name(String),
    /// Ordered fallback list of blend mode names (non-empty).
    Array(Vec<String>),
}

/// Soft-mask subtype (`/S`, ISO 32000-1 §11.6.5.2 Table 144).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoftMaskSubtype {
    /// `/Alpha`: use the group's alpha values.
    Alpha,
    /// `/Luminosity`: use the group's luminosity.
    Luminosity,
}

/// Parsed soft-mask dictionary (`/SMask`, Table 144).
#[derive(Debug, Clone, PartialEq)]
pub struct SoftMask {
    /// `/S` subtype.
    pub subtype: SoftMaskSubtype,
    /// `/G` transparency group XObject as written (usually a reference).
    pub group: Value,
    /// `/BC` backdrop colour components, each within `0..=1`.
    pub backdrop: Option<Vec<f64>>,
    /// `/TR` transfer function as written (name `Identity` or a function).
    pub transfer: Option<Value>,
    /// `/SMaskInData` (0–2).
    pub smask_in_data: Option<u8>,
    /// `/Matte` matte colour components (non-empty, finite); PDF 1.6.
    pub matte: Option<Vec<f64>>,
    /// Entries outside Table 146, preserved verbatim and unresolved.
    pub other: BTreeMap<String, Value>,
}

/// Parsed graphics state parameter dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtGState {
    /// `/LW` line width.
    pub line_width: Option<f64>,
    /// `/LC` line cap style (0–2).
    pub line_cap: Option<u8>,
    /// `/LJ` line join style (0–2).
    pub line_join: Option<u8>,
    /// `/ML` miter limit (≥ 1).
    pub miter_limit: Option<f64>,
    /// `/D` dash pattern.
    pub dash_pattern: Option<DashPattern>,
    /// `/RI` rendering intent name.
    pub rendering_intent: Option<String>,
    /// `/OP` overprint for stroking.
    pub overprint: Option<bool>,
    /// `/op` overprint for non-stroking operations.
    pub nonstroking_overprint: Option<bool>,
    /// `/OPM` overprint mode (0 or 1).
    pub overprint_mode: Option<u8>,
    /// `/Font` font resource and size.
    pub font: Option<ExtGStateFont>,
    /// `/FL` flatness tolerance (0–100).
    pub flatness: Option<f64>,
    /// `/SM` smoothness tolerance (0–1).
    pub smoothness: Option<f64>,
    /// `/SA` automatic stroke adjustment.
    pub stroke_adjustment: Option<bool>,
    /// `/BM` blend mode.
    pub blend_mode: Option<BlendMode>,
    /// `/CA` stroking alpha constant (0–1).
    pub stroke_alpha: Option<f64>,
    /// `/ca` non-stroking alpha constant (0–1).
    pub fill_alpha: Option<f64>,
    /// `/AIS` alpha is shape.
    pub alpha_is_shape: Option<bool>,
    /// `/TK` text knockout.
    pub text_knockout: Option<bool>,
    /// `/SMask` soft mask: `/None` or a soft-mask dictionary.
    pub soft_mask: Option<Value>,
    /// `/TR` transfer function.
    pub transfer: Option<Value>,
    /// `/TR2` transfer function for image masking.
    pub transfer2: Option<Value>,
    /// `/HT` halftone dictionary or stream.
    pub halftone: Option<Value>,
    /// `/BG` black-generation function.
    pub black_generation: Option<Value>,
    /// `/BG2` black-generation function for image masking.
    pub black_generation2: Option<Value>,
    /// `/UCR` undercolour-removal function.
    pub under_color_removal: Option<Value>,
    /// `/UCR2` undercolour-removal function for image masking.
    pub under_color_removal2: Option<Value>,
    /// Entries outside Table 58, preserved verbatim and unresolved.
    pub other: BTreeMap<String, Value>,
}
