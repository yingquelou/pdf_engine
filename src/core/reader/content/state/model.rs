//! 设备无关图形状态模型：仿射矩阵常量与拼接、`ContentGraphicsState`
//! 全量跟踪字段及其初值（Table 57），以及 `gs` 资源解析器类型别名。

use crate::core::reader::graphics_state::ExtGState;

use super::color::ColorValue;
use super::error::ContentStateError;

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
    /// Horizontal scaling factor (`Tz`).
    pub horizontal_scaling: f64,
    /// Text leading (`TL`).
    pub leading: f64,
    /// Current font resource name and size (`Tf`).
    pub font: Option<(String, f64)>,
    /// Text rendering mode (`Tr`): 0 fill, 1 stroke, 2 both, 3 invisible,
    /// 4 fill+clip, 5 stroke+clip, 6 both+clip, 7 clip only.
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
