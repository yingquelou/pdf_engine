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

/// Hybrid colour space parameter (`/CS`).
///
/// ISO 32000-1:2008 Table 58 (8.4.5) lists no `/CS` or `/CSS` entry; the
/// typed shape below is a conservative model of the hybrid-colour-space
/// parameter later standardised as a PDF 1.4 / PDF/VT-2 extension of the
/// transparency model (see 11.6.3, "Specifying Blending Colour Space and
/// Blend Mode"). It accepts the two documented shapes — a bare name or a
/// two-element array whose members are names or numeric special values —
/// and every other shape is preserved verbatim in [`ExtGState::other`]
/// with a [`GraphicsStateIssue`] recorded in [`ExtGState::issues`], so a
/// reader never loses the original bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum HybridColorSpace {
    /// Single colour-space name (e.g. `/CalRGB`).
    Name(String),
    /// Two-element array `[element element]` of colour-space names or
    /// numeric special values, kept in written order.
    Pair(HybridColorSpaceElement, HybridColorSpaceElement),
}

/// One element of a hybrid-colour-space array (`/CS [element element]`).
#[derive(Debug, Clone, PartialEq)]
pub enum HybridColorSpaceElement {
    /// Colour-space name or a special name value.
    Name(String),
    /// Numeric special value, kept finite; range is caller policy.
    Number(f64),
}

/// Hybrid colour space style (`/CSS`, always `/Knockout` or `/Native`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridColorSpaceStyle {
    /// `/Knockout`.
    Knockout,
    /// `/Native`.
    Native,
}

/// Diagnostic for an ExtGState entry that could not be typed.
///
/// Records the offending key and a human-readable reason. The raw value is
/// preserved in [`ExtGState::other`] under the same key so round-tripping
/// never drops bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsStateIssue {
    /// Dictionary key that produced the diagnostic (e.g. `"CS"`).
    pub key: String,
    /// Human-readable reason, always prefixed with `/<key>`.
    pub message: String,
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
///
/// Table 144 (11.6.5.2) lists five keys only — `Type`, `S`, `G`, `BC`, `TR`.
/// The two extra typed fields below come from the neighbouring soft-mask
/// *image* clauses and are tolerated, not required.
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
    /// `/SMaskInData` (0–2), kept for masks carried together with it.
    ///
    /// Not a Table 144 entry: 8.9.5 Table 89 defines it for an image
    /// dictionary whose samples embed soft-mask information (`/JPXDecode`,
    /// PDF 1.5, default 0). This reader also accepts it inside a soft-mask
    /// dictionary because real files attach it there; see the `soft_mask`
    /// module documentation.
    pub smask_in_data: Option<u8>,
    /// `/Matte` matte colour components (non-empty, finite).
    ///
    /// Not a Table 144 entry: 11.6.5.3 Table 146 defines it as the optional
    /// PDF 1.4 entry of a *soft-mask image* dictionary, naming the colour the
    /// parent image's data was preblended with. Accepted here as well
    /// because preblended masks travel with this dictionary in real files.
    pub matte: Option<Vec<f64>>,
    /// Entries outside Table 144, preserved verbatim and unresolved.
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
    /// `/FL` flatness tolerance, accepted in `0..=100`.
    ///
    /// Table 58 defers the value to 10.6.2, where flatness "shall be a
    /// positive number" with an initial value of 1.0 (Table 53); the only
    /// explicit numeric range in the standard is the `i` operator row of
    /// Table 57, "a number in the range 0 to 100; a value of 0 shall specify
    /// the output device's default flatness tolerance". Both bounds and the
    /// meaning of 0 therefore stay as they are.
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
    /// `/SMask` soft mask: `/None` (11.6.4.3) or a soft-mask dictionary.
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
    /// `/CS` hybrid colour space parameter (see [`HybridColorSpace`]).
    ///
    /// Not an ISO 32000-1:2008 Table 58 entry; the shape is modelled
    /// conservatively from the transparency-clause extension that defines
    /// this key as a name or a two-element array. A malformed value is
    /// preserved in [`ExtGState::other`] and reported through
    /// [`ExtGState::issues`] rather than failing the whole dictionary.
    pub hybrid_color_space: Option<HybridColorSpace>,
    /// `/CSS` hybrid colour space style (`/Knockout` or `/Native`).
    ///
    /// Companion of [`ExtGState::hybrid_color_space`]; unrecognised names
    /// and non-name values follow the same preserve-and-report contract.
    pub hybrid_color_space_style: Option<HybridColorSpaceStyle>,
    /// Entries outside Table 58, preserved verbatim and unresolved.
    pub other: BTreeMap<String, Value>,
    /// Diagnostics for standard keys the reader could not type; the raw
    /// value is simultaneously preserved in [`ExtGState::other`].
    pub issues: Vec<GraphicsStateIssue>,
}
