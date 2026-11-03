use super::model::{BlendMode, DashPattern, ExtGStateFont};
use super::{Dict, Document, GraphicsStateError, Result, Value};

/// Standard entries of the graphics state parameter dictionary.
pub(super) const STANDARD_ENTRIES: [&str; 26] = [
    "LW", "LC", "LJ", "ML", "D", "RI", "OP", "op", "OPM", "Font", "FL", "SM", "SA", "BM", "CA",
    "ca", "AIS", "TK", "SMask", "TR", "TR2", "HT", "BG", "BG2", "UCR", "UCR2",
];

pub(super) fn invalid(message: impl Into<String>) -> GraphicsStateError {
    GraphicsStateError::Invalid(message.into())
}

pub(super) fn entry(document: &Document, dict: &Dict, key: &str) -> Result<Option<Value>> {
    dict.get(key)
        .map(|value| document.resolve_graphics_state_value(value))
        .transpose()
}

pub(super) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Float(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

pub(super) fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(value) => Some(*value),
        Value::Float(value) if value.is_finite() && value.fract() == 0.0 => Some(*value as i64),
        _ => None,
    }
}

pub(super) fn optional_min_number(
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

pub(super) fn optional_ranged_number(
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

pub(super) fn optional_code(
    document: &Document,
    dict: &Dict,
    key: &str,
    max: u8,
) -> Result<Option<u8>> {
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

pub(super) fn optional_bool(document: &Document, dict: &Dict, key: &str) -> Result<Option<bool>> {
    match entry(document, dict, key)? {
        Some(Value::Bool(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(format!("/{key} must be a boolean"))),
        None => Ok(None),
    }
}

pub(super) fn optional_name(document: &Document, dict: &Dict, key: &str) -> Result<Option<String>> {
    match entry(document, dict, key)? {
        Some(Value::Name(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(format!("/{key} must be a name"))),
        None => Ok(None),
    }
}

pub(super) fn optional_raw(document: &Document, dict: &Dict, key: &str) -> Result<Option<Value>> {
    entry(document, dict, key)
}

pub(super) fn optional_dash(document: &Document, dict: &Dict) -> Result<Option<DashPattern>> {
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

pub(super) fn optional_font(document: &Document, dict: &Dict) -> Result<Option<ExtGStateFont>> {
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

pub(super) fn optional_blend_mode(document: &Document, dict: &Dict) -> Result<Option<BlendMode>> {
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
