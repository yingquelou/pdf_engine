//! CID-keyed font data types and the `/CIDSystemInfo` dictionary parser.

use crate::core::types::{Dict, Document, Value};

use super::{FontError, FontResult};

/// Parsed `/CIDSystemInfo` dictionary for a CID-keyed font.
///
/// Identifies the character collection: `/Registry` (e.g. "Adobe"),
/// `/Ordering` (e.g. "GB1"), and `/Supplement` (revision number).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidSystemInfo {
    /// `/Registry` string (e.g. "Adobe").
    pub registry: String,
    /// `/Ordering` string (e.g. "GB1", "Japan1", "Korea1").
    pub ordering: String,
    /// `/Supplement` non-negative integer.
    pub supplement: i64,
}

/// Default vertical metrics from a CID font's `/DW2` entry.
///
/// `/DW2` is `[v_y, w1_y]` (ISO 32000-1 Table 116): `v_y` is the vertical
/// displacement of the glyph origin and `w1_y` is the default vertical
/// component of the position vector. The default is `[880, -1000]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultVerticalMetrics {
    /// `v_y`: vertical displacement of the glyph origin.
    pub v_y: f64,
    /// `w1_y`: default vertical component of the position vector.
    pub w1_y: f64,
}

impl Default for DefaultVerticalMetrics {
    fn default() -> Self {
        Self {
            v_y: 880.0,
            w1_y: -1000.0,
        }
    }
}

/// Vertical metrics for one CID from `/W2` (ISO 32000-1 Table 116).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalMetrics {
    /// `w1_y`: vertical component of the position vector.
    pub w1_y: f64,
    /// `v_x`: horizontal component of the vertical displacement.
    pub v1x: f64,
    /// `v_y`: vertical component of the vertical displacement.
    pub v1y: f64,
}

pub(in crate::core::reader::font) fn parse_cid_system_info(
    document: &Document,
    dict: &Dict,
) -> FontResult<CidSystemInfo> {
    let value = dict
        .get("CIDSystemInfo")
        .ok_or_else(|| FontError::Invalid("CID font is missing /CIDSystemInfo".into()))?;
    let value = document.resolve_value(value)?;
    let info = value
        .as_dict()
        .ok_or_else(|| FontError::Invalid("CID font /CIDSystemInfo is not a dictionary".into()))?;

    let registry = info
        .get("Registry")
        .ok_or_else(|| FontError::Invalid("CIDSystemInfo is missing /Registry".into()))?;
    let registry = document.resolve_value(registry)?;
    let registry = match registry {
        Value::String(s) | Value::HexString(s) => s.clone(),
        _ => {
            return Err(FontError::Invalid(
                "CIDSystemInfo /Registry is not a string".into(),
            ));
        }
    };

    let ordering = info
        .get("Ordering")
        .ok_or_else(|| FontError::Invalid("CIDSystemInfo is missing /Ordering".into()))?;
    let ordering = document.resolve_value(ordering)?;
    let ordering = match ordering {
        Value::String(s) | Value::HexString(s) => s.clone(),
        _ => {
            return Err(FontError::Invalid(
                "CIDSystemInfo /Ordering is not a string".into(),
            ));
        }
    };

    let supplement = info
        .get("Supplement")
        .ok_or_else(|| FontError::Invalid("CIDSystemInfo is missing /Supplement".into()))?;
    let supplement = document.resolve_value(supplement)?;
    let supplement = match supplement {
        Value::Integer(value) if value >= 0 => value,
        _ => {
            return Err(FontError::Invalid(
                "CIDSystemInfo /Supplement is not a nonnegative integer".into(),
            ));
        }
    };

    Ok(CidSystemInfo {
        registry,
        ordering,
        supplement,
    })
}
