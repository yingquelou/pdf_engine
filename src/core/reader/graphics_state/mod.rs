//! Graphics state parameter dictionaries (PDF 8.4.4, Table 58).
//!
//! [`ExtGState`] exposes every standard entry with type and range checks.
//! Entries that carry function, halftone or soft-mask objects (`/SMask`,
//! `/TR`, `/TR2`, `/HT`, `/BG`, `/BG2`, `/UCR`, `/UCR2`) are preserved as
//! resolved raw values; [`Document::soft_mask`] additionally parses an
//! `/SMask` value into a typed [`SoftMask`] dictionary and
//! `Document::halftone` (see the `halftone` submodule) parses an `/HT`
//! value into a [`HalftoneModel`]. Interpreting transfer functions remains
//! out of scope.
//! Unknown entries are kept verbatim in [`ExtGState::other`] without
//! resolving them.

mod halftone;

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::{Dict, Document, Value};

pub use halftone::{CompositeHalftone, HalftoneModel, Type1Halftone, Type2Halftone};

use super::ParseError;

type Result<T> = std::result::Result<T, GraphicsStateError>;

/// Errors produced while parsing a graphics state parameter dictionary.
#[derive(Debug)]
pub enum GraphicsStateError {
    /// An indirect object referenced by the dictionary could not be parsed.
    Parse(ParseError),
    /// The dictionary or one of its entries does not satisfy Table 58.
    Invalid(String),
}

impl fmt::Display for GraphicsStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid graphics state: {message}"),
        }
    }
}

impl std::error::Error for GraphicsStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<ParseError> for GraphicsStateError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

/// Standard entries of the graphics state parameter dictionary.
const STANDARD_ENTRIES: [&str; 26] = [
    "LW", "LC", "LJ", "ML", "D", "RI", "OP", "op", "OPM", "Font", "FL", "SM", "SA", "BM", "CA",
    "ca", "AIS", "TK", "SMask", "TR", "TR2", "HT", "BG", "BG2", "UCR", "UCR2",
];

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

impl Document {
    pub(crate) fn resolve_graphics_state_value(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self.get_object(*num)?.ok_or_else(|| {
                    GraphicsStateError::Invalid(format!(
                        "unresolved object reference {num} {generation} R"
                    ))
                })?;
                Ok(object.value)
            }
            _ => Ok(value.clone()),
        }
    }

    /// Parse one graphics state parameter dictionary.
    ///
    /// `value` may be a dictionary or an indirect reference to one.
    pub fn ext_g_state(&self, value: &Value) -> Result<ExtGState> {
        let value = self.resolve_graphics_state_value(value)?;
        let dict = value.as_dict().ok_or_else(|| {
            GraphicsStateError::Invalid(format!(
                "graphics state parameter dictionary must be a dictionary, got {}",
                value.type_name()
            ))
        })?;
        parse_ext_g_state(self, dict)
    }

    /// Parse a resource dictionary's `/ExtGState` sub-dictionary.
    ///
    /// Returns an empty map when the resource dictionary has no
    /// `/ExtGState` entry. A non-dictionary `/ExtGState` value or any
    /// invalid entry is reported as an error.
    pub fn ext_g_states(&self, resources: &Dict) -> Result<BTreeMap<String, ExtGState>> {
        let Some(value) = resources.get("ExtGState") else {
            return Ok(BTreeMap::new());
        };
        let value = self.resolve_graphics_state_value(value)?;
        let dict = value.as_dict().ok_or_else(|| {
            GraphicsStateError::Invalid("/ExtGState resource is not a dictionary".to_string())
        })?;
        let mut states = BTreeMap::new();
        for (name, value) in &dict.0 {
            states.insert(name.clone(), self.ext_g_state(value)?);
        }
        Ok(states)
    }

    /// Parse an ExtGState `/SMask` value as a soft-mask dictionary.
    ///
    /// `value` may be a dictionary or an indirect reference to one.
    /// `/S` must be `/Alpha` or `/Luminosity`, `/G` must resolve to a
    /// Form or Image XObject stream, `/BC` components must lie within
    /// `0..=1`, `/SMaskInData` must be 0–2, `/Matte` must be a non-empty
    /// array of numbers, and a `/TR` name must be `/Identity`. `/G` and
    /// `/TR` are kept as written so callers can resolve them themselves.
    pub fn soft_mask(&self, value: &Value) -> Result<SoftMask> {
        let value = self.resolve_graphics_state_value(value)?;
        let dict = value.as_dict().ok_or_else(|| {
            GraphicsStateError::Invalid(format!(
                "soft mask must be a dictionary, got {}",
                value.type_name()
            ))
        })?;
        parse_soft_mask(self, dict)
    }
}

/// Parse a graphics state parameter dictionary (`Table 58`).
///
/// Indirect values are resolved one level; `/Font` keeps its first element
/// unresolved (see [`ExtGStateFont::font`]).
pub fn parse_ext_g_state(document: &Document, dict: &Dict) -> Result<ExtGState> {
    if let Some(type_value) = dict.get("Type") {
        let type_value = document.resolve_graphics_state_value(type_value)?;
        if type_value.as_name() != Some("ExtGState") {
            return Err(invalid("/Type must be /ExtGState"));
        }
    }

    let mut other = BTreeMap::new();
    for (key, value) in &dict.0 {
        if key != "Type" && !STANDARD_ENTRIES.contains(&key.as_str()) {
            other.insert(key.clone(), value.clone());
        }
    }

    Ok(ExtGState {
        line_width: optional_min_number(document, dict, "LW", 0.0)?,
        line_cap: optional_code(document, dict, "LC", 2)?,
        line_join: optional_code(document, dict, "LJ", 2)?,
        miter_limit: optional_min_number(document, dict, "ML", 1.0)?,
        dash_pattern: optional_dash(document, dict)?,
        rendering_intent: optional_name(document, dict, "RI")?,
        overprint: optional_bool(document, dict, "OP")?,
        nonstroking_overprint: optional_bool(document, dict, "op")?,
        overprint_mode: optional_code(document, dict, "OPM", 1)?,
        font: optional_font(document, dict)?,
        flatness: optional_ranged_number(document, dict, "FL", 0.0, 100.0)?,
        smoothness: optional_ranged_number(document, dict, "SM", 0.0, 1.0)?,
        stroke_adjustment: optional_bool(document, dict, "SA")?,
        blend_mode: optional_blend_mode(document, dict)?,
        stroke_alpha: optional_ranged_number(document, dict, "CA", 0.0, 1.0)?,
        fill_alpha: optional_ranged_number(document, dict, "ca", 0.0, 1.0)?,
        alpha_is_shape: optional_bool(document, dict, "AIS")?,
        text_knockout: optional_bool(document, dict, "TK")?,
        soft_mask: optional_raw(document, dict, "SMask")?,
        transfer: optional_raw(document, dict, "TR")?,
        transfer2: optional_raw(document, dict, "TR2")?,
        halftone: optional_raw(document, dict, "HT")?,
        black_generation: optional_raw(document, dict, "BG")?,
        black_generation2: optional_raw(document, dict, "BG2")?,
        under_color_removal: optional_raw(document, dict, "UCR")?,
        under_color_removal2: optional_raw(document, dict, "UCR2")?,
        other,
    })
}

fn invalid(message: impl Into<String>) -> GraphicsStateError {
    GraphicsStateError::Invalid(message.into())
}

/// Parse a soft-mask dictionary (`Table 144`).
fn parse_soft_mask(document: &Document, dict: &Dict) -> Result<SoftMask> {
    let subtype = match entry(document, dict, "S")? {
        None => return Err(invalid("soft mask /S is required")),
        Some(Value::Name(name)) if name == "Alpha" => SoftMaskSubtype::Alpha,
        Some(Value::Name(name)) if name == "Luminosity" => SoftMaskSubtype::Luminosity,
        Some(_) => return Err(invalid("soft mask /S must be /Alpha or /Luminosity")),
    };

    let group = dict
        .get("G")
        .ok_or_else(|| invalid("soft mask /G is required"))?
        .clone();
    let resolved_group = document.resolve_graphics_state_value(&group)?;
    let Value::Stream(stream_dict, _) = &resolved_group else {
        return Err(invalid(
            "soft mask /G must be a Form or Image XObject stream",
        ));
    };
    let subtype_value = stream_dict
        .get("Subtype")
        .map(|value| document.resolve_graphics_state_value(value))
        .transpose()?;
    match subtype_value.as_ref().and_then(Value::as_name) {
        Some("Form" | "Image") => {}
        _ => {
            return Err(invalid(
                "soft mask /G must be a Form or Image XObject stream",
            ));
        }
    }

    let backdrop = match entry(document, dict, "BC")? {
        None => None,
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| invalid("soft mask /BC must be an array"))?;
            if array.is_empty() {
                return Err(invalid("soft mask /BC must not be empty"));
            }
            let mut components = Vec::with_capacity(array.len());
            for item in array.iter() {
                let item = document.resolve_graphics_state_value(item)?;
                let Some(component) = number(&item) else {
                    return Err(invalid("soft mask /BC entries must be numbers"));
                };
                if !(0.0..=1.0).contains(&component) {
                    return Err(invalid("soft mask /BC entries must be within 0..=1"));
                }
                components.push(component);
            }
            Some(components)
        }
    };

    let transfer = entry(document, dict, "TR")?;
    if let Some(Value::Name(name)) = &transfer
        && name.as_str() != "Identity"
    {
        return Err(invalid("soft mask /TR name must be /Identity"));
    }

    let smask_in_data = match entry(document, dict, "SMaskInData")? {
        None => None,
        Some(value) => {
            let Some(code) = integer(&value) else {
                return Err(invalid("soft mask /SMaskInData must be an integer"));
            };
            if !(0..=2).contains(&code) {
                return Err(invalid("soft mask /SMaskInData must be 0, 1 or 2"));
            }
            Some(code as u8)
        }
    };

    let matte = match entry(document, dict, "Matte")? {
        None => None,
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| invalid("soft mask /Matte must be an array"))?;
            if array.is_empty() {
                return Err(invalid("soft mask /Matte must not be empty"));
            }
            let mut components = Vec::with_capacity(array.len());
            for item in array.iter() {
                let item = document.resolve_graphics_state_value(item)?;
                let Some(component) = number(&item) else {
                    return Err(invalid("soft mask /Matte entries must be numbers"));
                };
                components.push(component);
            }
            Some(components)
        }
    };

    const SOFT_MASK_ENTRIES: [&str; 7] = ["Type", "S", "G", "BC", "TR", "SMaskInData", "Matte"];
    let mut other = BTreeMap::new();
    for (key, value) in &dict.0 {
        if !SOFT_MASK_ENTRIES.contains(&key.as_str()) {
            other.insert(key.clone(), value.clone());
        }
    }

    Ok(SoftMask {
        subtype,
        group,
        backdrop,
        transfer,
        smask_in_data,
        matte,
        other,
    })
}

fn entry(document: &Document, dict: &Dict, key: &str) -> Result<Option<Value>> {
    dict.get(key)
        .map(|value| document.resolve_graphics_state_value(value))
        .transpose()
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Float(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(value) => Some(*value),
        Value::Float(value) if value.is_finite() && value.fract() == 0.0 => Some(*value as i64),
        _ => None,
    }
}

fn optional_min_number(
    document: &Document,
    dict: &Dict,
    key: &str,
    min: f64,
) -> Result<Option<f64>> {
    match entry(document, dict, key)? {
        Some(value) => {
            let Some(number) = number(&value) else {
                return Err(invalid(format!("/{key} must be a number")));
            };
            if number < min {
                return Err(invalid(format!("/{key} must be >= {min}")));
            }
            Ok(Some(number))
        }
        None => Ok(None),
    }
}

fn optional_ranged_number(
    document: &Document,
    dict: &Dict,
    key: &str,
    min: f64,
    max: f64,
) -> Result<Option<f64>> {
    match entry(document, dict, key)? {
        Some(value) => {
            let Some(number) = number(&value) else {
                return Err(invalid(format!("/{key} must be a number")));
            };
            if !(min..=max).contains(&number) {
                return Err(invalid(format!("/{key} must be within {min}..={max}")));
            }
            Ok(Some(number))
        }
        None => Ok(None),
    }
}

fn optional_code(document: &Document, dict: &Dict, key: &str, max: u8) -> Result<Option<u8>> {
    match entry(document, dict, key)? {
        Some(value) => {
            let Some(code) = integer(&value).and_then(|value| u8::try_from(value).ok()) else {
                return Err(invalid(format!("/{key} must be an integer")));
            };
            if code > max {
                return Err(invalid(format!("/{key} must be within 0..={max}")));
            }
            Ok(Some(code))
        }
        None => Ok(None),
    }
}

fn optional_bool(document: &Document, dict: &Dict, key: &str) -> Result<Option<bool>> {
    match entry(document, dict, key)? {
        Some(Value::Bool(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(format!("/{key} must be a boolean"))),
        None => Ok(None),
    }
}

fn optional_name(document: &Document, dict: &Dict, key: &str) -> Result<Option<String>> {
    match entry(document, dict, key)? {
        Some(Value::Name(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(format!("/{key} must be a name"))),
        None => Ok(None),
    }
}

fn optional_raw(document: &Document, dict: &Dict, key: &str) -> Result<Option<Value>> {
    entry(document, dict, key)
}

fn optional_dash(document: &Document, dict: &Dict) -> Result<Option<DashPattern>> {
    let Some(value) = entry(document, dict, "D")? else {
        return Ok(None);
    };
    let Some(parts) = value.as_array() else {
        return Err(invalid("/D must be an array [dashArray dashPhase]"));
    };
    if parts.len() != 2 {
        return Err(invalid(
            "/D must contain a dash array and a dash phase".to_string(),
        ));
    }
    let array_value = document.resolve_graphics_state_value(&parts.0[0])?;
    let Some(lengths) = array_value.as_array() else {
        return Err(invalid("/D dash array must be an array"));
    };
    let mut array = Vec::with_capacity(lengths.len());
    for item in lengths.iter() {
        let item = document.resolve_graphics_state_value(item)?;
        let Some(length) = number(&item) else {
            return Err(invalid("/D dash array entries must be numbers"));
        };
        if length < 0.0 {
            return Err(invalid("/D dash array entries must be non-negative"));
        }
        array.push(length);
    }
    let phase_value = document.resolve_graphics_state_value(&parts.0[1])?;
    let Some(phase) = number(&phase_value) else {
        return Err(invalid("/D dash phase must be a number"));
    };
    if phase < 0.0 {
        return Err(invalid("/D dash phase must be non-negative"));
    }
    Ok(Some(DashPattern { array, phase }))
}

fn optional_font(document: &Document, dict: &Dict) -> Result<Option<ExtGStateFont>> {
    let Some(value) = entry(document, dict, "Font")? else {
        return Ok(None);
    };
    let Some(parts) = value.as_array() else {
        return Err(invalid("/Font must be an array [font size]"));
    };
    if parts.len() != 2 {
        return Err(invalid("/Font must contain a font and a size".to_string()));
    }
    let font = parts.0[0].clone();
    // Table 58: the first element is either a font resource name or a
    // direct/indirect font dictionary; only the dictionary form is resolved.
    if !matches!(font, Value::Name(_)) {
        let resolved_font = document.resolve_graphics_state_value(&font)?;
        if resolved_font.as_dict().is_none() {
            return Err(invalid(
                "/Font font entry must be a name or resolve to a dictionary",
            ));
        }
    }
    let size_value = document.resolve_graphics_state_value(&parts.0[1])?;
    let Some(size) = number(&size_value) else {
        return Err(invalid("/Font size must be a number"));
    };
    if size <= 0.0 {
        return Err(invalid("/Font size must be positive"));
    }
    Ok(Some(ExtGStateFont { font, size }))
}

fn optional_blend_mode(document: &Document, dict: &Dict) -> Result<Option<BlendMode>> {
    let Some(value) = entry(document, dict, "BM")? else {
        return Ok(None);
    };
    match value {
        Value::Name(name) => Ok(Some(BlendMode::Name(name))),
        Value::Array(items) => {
            if items.is_empty() {
                return Err(invalid("/BM fallback array must not be empty"));
            }
            let mut names = Vec::with_capacity(items.len());
            for item in items.iter() {
                let item = document.resolve_graphics_state_value(item)?;
                let Some(name) = item.as_name() else {
                    return Err(invalid("/BM fallback array entries must be names"));
                };
                names.push(name.to_string());
            }
            Ok(Some(BlendMode::Array(names)))
        }
        _ => Err(invalid("/BM must be a name or an array of names")),
    }
}

#[cfg(test)]
mod tests;
