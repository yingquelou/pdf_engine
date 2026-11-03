use super::helpers::{
    STANDARD_ENTRIES, invalid, optional_blend_mode, optional_bool, optional_code, optional_dash,
    optional_font, optional_min_number, optional_name, optional_ranged_number, optional_raw,
};
use super::model::{ExtGState, SoftMask};
use super::soft_mask::parse_soft_mask;
use super::{BTreeMap, Dict, Document, GraphicsStateError, Result, Value};

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
