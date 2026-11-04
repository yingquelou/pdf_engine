use super::model::{
    BlendMode, DashPattern, ExtGStateFont, GraphicsStateIssue, HybridColorSpace,
    HybridColorSpaceElement, HybridColorSpaceStyle,
};
use super::{BTreeMap, Dict, Document, GraphicsStateError, Result, Value};

/// Standard entries of the graphics state parameter dictionary.
///
/// The 26 parameter keys of ISO 32000-1 Table 58 (8.4.5) — `/LW /LC /LJ /ML
/// /D /RI /OP /op /OPM /Font /BG /BG2 /UCR /UCR2 /TR /TR2 /HT /FL /SM /SA
/// /BM /SMask /CA /ca /AIS /TK` — plus the two hybrid-colour-space keys
/// `/CS` and `/CSS` standardised later in the transparency clause (11.6.3)
/// and PDF/VT-2 as PDF 1.4 extensions; `/Type` is validated separately.
/// Table 58 itself lists no other key: in particular `/DS` is not a
/// graphics state entry (it is a free-text annotation and variable-text
/// field entry, Table 174 and Table 222, PDF 1.5). A key outside this list
/// is not rejected: it is preserved verbatim in `ExtGState::other`. A key
/// *inside* this list whose shape the reader cannot type — only `/CS` and
/// `/CSS` today, since the other entries have hard Table 58 contracts and
/// malformed values fail the whole dictionary — is likewise preserved in
/// `ExtGState::other` with a diagnostic in `ExtGState::issues`.
pub(super) const STANDARD_ENTRIES: [&str; 28] = [
    "LW", "LC", "LJ", "ML", "D", "RI", "OP", "op", "OPM", "Font", "FL", "SM", "SA", "BM", "CA",
    "ca", "AIS", "TK", "SMask", "TR", "TR2", "HT", "BG", "BG2", "UCR", "UCR2", "CS", "CSS",
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
            // A negative integer, an integer above 255 and a value above
            // `max` are all the same defect: the code is outside `0..=max`.
            // Reporting them as "must be an integer" would hide the real
            // problem, so the range and the offending value are both named.
            let Some(code) = integer(&value) else {
                return Err(invalid(format!("/{key} must be an integer")));
            };
            if code < 0 || code > i64::from(max) {
                return Err(invalid(format!(
                    "/{key} must be an integer within 0..={max}, got {code}"
                )));
            }
            Ok(Some(code as u8))
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

/// Parse a `/CS` hybrid colour space value.
///
/// Accepted shapes (see [`HybridColorSpace`]): a bare name, or a two-element
/// array whose members are names or finite numbers. Every other shape — a
/// dictionary, a stream, a wrong-length array, a non-name/non-number
/// element, or an unresolvable indirect reference *inside* the array — is
/// NOT a hard failure: the as-written raw value (which may still be an
/// indirect reference) is put back into `other`, and a diagnostic is
/// appended to `issues`, so a reader never panics and never drops bytes.
/// `entry(...)` itself propagates unresolved top-level references as a hard
/// parse error, because the *existence* of the referenced object is not a
/// shape judgement the hybrid-clause model can defer.
pub(super) fn optional_hybrid_color_space(
    document: &Document,
    dict: &Dict,
    other: &mut BTreeMap<String, Value>,
    issues: &mut Vec<GraphicsStateIssue>,
) -> Result<Option<HybridColorSpace>> {
    let Some(raw) = dict.get("CS") else {
        return Ok(None);
    };
    let value = match document.resolve_graphics_state_value(raw) {
        Ok(value) => value,
        Err(error) => {
            reject_hybrid_entry("CS", raw.clone(), other, issues, &error.to_string());
            return Ok(None);
        }
    };
    match &value {
        Value::Name(name) => Ok(Some(HybridColorSpace::Name(name.clone()))),
        Value::Array(items) if items.0.len() == 2 => {
            let first = hybrid_element(document, &items.0[0]);
            let second = hybrid_element(document, &items.0[1]);
            match (first, second) {
                (Some(first), Some(second)) => Ok(Some(HybridColorSpace::Pair(first, second))),
                _ => {
                    reject_hybrid_entry(
                        "CS",
                        raw.clone(),
                        other,
                        issues,
                        "/CS array elements must each be a name or a finite number",
                    );
                    Ok(None)
                }
            }
        }
        Value::Array(_) => {
            reject_hybrid_entry(
                "CS",
                raw.clone(),
                other,
                issues,
                "/CS array must have exactly two elements",
            );
            Ok(None)
        }
        _ => {
            reject_hybrid_entry(
                "CS",
                raw.clone(),
                other,
                issues,
                &format!(
                    "/CS must be a name or a two-element array, got {}",
                    value.type_name()
                ),
            );
            Ok(None)
        }
    }
}

/// Parse a `/CSS` hybrid colour space style.
///
/// Accepted shapes: the exact names `/Knockout` or `/Native`. Anything
/// else — a non-name value, an unrecognised name, an unresolvable
/// reference — is preserved in `other` with a diagnostic in `issues`,
/// mirroring `/CS`.
pub(super) fn optional_hybrid_color_space_style(
    document: &Document,
    dict: &Dict,
    other: &mut BTreeMap<String, Value>,
    issues: &mut Vec<GraphicsStateIssue>,
) -> Result<Option<HybridColorSpaceStyle>> {
    let Some(raw) = dict.get("CSS") else {
        return Ok(None);
    };
    let value = match document.resolve_graphics_state_value(raw) {
        Ok(value) => value,
        Err(error) => {
            reject_hybrid_entry("CSS", raw.clone(), &mut *other, issues, &error.to_string());
            return Ok(None);
        }
    };
    match value.as_name() {
        Some("Knockout") => Ok(Some(HybridColorSpaceStyle::Knockout)),
        Some("Native") => Ok(Some(HybridColorSpaceStyle::Native)),
        Some(name) => {
            reject_hybrid_entry(
                "CSS",
                raw.clone(),
                &mut *other,
                issues,
                &format!("/CSS name must be /Knockout or /Native, got /{name}"),
            );
            Ok(None)
        }
        None => {
            reject_hybrid_entry(
                "CSS",
                raw.clone(),
                &mut *other,
                issues,
                &format!("/CSS must be a name, got {}", value.type_name()),
            );
            Ok(None)
        }
    }
}

fn hybrid_element(document: &Document, value: &Value) -> Option<HybridColorSpaceElement> {
    let resolved = document.resolve_graphics_state_value(value).ok()?;
    match &resolved {
        Value::Name(name) => Some(HybridColorSpaceElement::Name(name.clone())),
        Value::Integer(number) => Some(HybridColorSpaceElement::Number(*number as f64)),
        Value::Float(number) if number.is_finite() => {
            Some(HybridColorSpaceElement::Number(*number))
        }
        _ => None,
    }
}

fn reject_hybrid_entry(
    key: &str,
    raw: Value,
    other: &mut BTreeMap<String, Value>,
    issues: &mut Vec<GraphicsStateIssue>,
    message: &str,
) {
    other.insert(key.to_string(), raw);
    issues.push(GraphicsStateIssue {
        key: key.to_string(),
        message: message.to_string(),
    });
}
