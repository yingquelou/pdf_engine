//! Reading of CID-keyed descendant font dictionaries (CIDFontType2 metrics,
//! CID-to-glyph mapping, and the `/W`/`/DW`/`/W2`/`/DW2` tables).

mod tables;
mod types;

use std::collections::BTreeMap;

use crate::core::types::{Dict, Document, Value};

use super::{
    FontDescriptorMetrics, FontError, FontResult, number, parse_descriptor, truetype::TrueTypeFont,
};

pub use self::types::{CidSystemInfo, DefaultVerticalMetrics, VerticalMetrics};

use self::tables::{metric_number, parse_vertical_metrics, parse_widths};
pub(in crate::core::reader::font) use self::types::parse_cid_system_info;

#[derive(Debug, Clone, PartialEq)]
enum CidToGidMap {
    Identity,
    Explicit(Vec<u8>),
}

/// Parsed metrics and CID-to-glyph mapping for a PDF CIDFontType2.
#[derive(Debug, Clone, PartialEq)]
pub struct CidFontType2 {
    base_font: String,
    system_info: CidSystemInfo,
    default_width: f64,
    widths: BTreeMap<u16, f64>,
    default_vertical_metrics: DefaultVerticalMetrics,
    vertical_metrics: BTreeMap<u16, VerticalMetrics>,
    descriptor: FontDescriptorMetrics,
    cid_to_gid: CidToGidMap,
    true_type_font: Option<TrueTypeFont>,
}

impl CidFontType2 {
    /// Return the descendant font's `/BaseFont` name.
    pub fn base_font(&self) -> &str {
        &self.base_font
    }

    /// Return the parsed `/CIDSystemInfo` (Registry, Ordering, Supplement).
    pub fn system_info(&self) -> &CidSystemInfo {
        &self.system_info
    }

    /// Return `/DW`, defaulting to 1000 when omitted.
    pub fn default_width(&self) -> f64 {
        self.default_width
    }

    /// Return the horizontal width for a CID using `/W` and `/DW`.
    pub fn width_for_cid(&self, cid: u16) -> f64 {
        self.widths.get(&cid).copied().unwrap_or(self.default_width)
    }

    /// Return `/DW2`, defaulting to `[880, -1000]` when omitted.
    pub fn default_vertical_metrics(&self) -> DefaultVerticalMetrics {
        self.default_vertical_metrics
    }

    /// Return the vertical metrics for a CID.
    ///
    /// `/W2` entries take precedence. Otherwise `w1_y` and `v_y` fall back to
    /// `/DW2`, and `v_x` defaults to half the glyph's horizontal width.
    pub fn vertical_metrics_for_cid(&self, cid: u16) -> VerticalMetrics {
        self.vertical_metrics
            .get(&cid)
            .copied()
            .unwrap_or(VerticalMetrics {
                w1_y: self.default_vertical_metrics.w1_y,
                v1x: self.width_for_cid(cid) / 2.0,
                v1y: self.default_vertical_metrics.v_y,
            })
    }

    /// Map a CID through `/CIDToGIDMap`, defaulting to identity.
    ///
    /// An explicit map that does not contain this CID returns `None`.
    pub fn glyph_id_for_cid(&self, cid: u16) -> Option<u16> {
        let glyph_id = match &self.cid_to_gid {
            CidToGidMap::Identity => Some(cid),
            CidToGidMap::Explicit(bytes) => {
                let offset = usize::from(cid).checked_mul(2)?;
                let pair = bytes.get(offset..offset.checked_add(2)?)?;
                Some(u16::from_be_bytes([pair[0], pair[1]]))
            }
        }?;
        self.true_type_font
            .as_ref()
            .is_none_or(|font| glyph_id < font.glyph_count())
            .then_some(glyph_id)
    }

    /// Return the parsed `/FontDescriptor` metrics.
    pub fn descriptor(&self) -> &FontDescriptorMetrics {
        &self.descriptor
    }

    /// Return the embedded TrueType program when `/FontFile2` is present.
    pub fn true_type_font(&self) -> Option<&TrueTypeFont> {
        self.true_type_font.as_ref()
    }
}

impl Document {
    /// Parse a CIDFontType2 descendant font's CID metrics and glyph mapping.
    ///
    /// `/CIDToGIDMap` may be `/Identity` or a stream of big-endian 16-bit
    /// glyph IDs. `/W` and `/DW` provide widths in glyph-space units. An
    /// embedded `/FontFile2` is parsed as a TrueType font when present.
    ///
    /// Returns `Ok(None)` for values whose `/Subtype` is not `/CIDFontType2`.
    pub fn cid_font_type2(&self, value: &Value) -> FontResult<Option<CidFontType2>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("CID font value is not a dictionary".into()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("CIDFontType2") {
            return Ok(None);
        }

        let metrics = parse_cid_metrics(self, dict)?;

        let cid_to_gid = match dict.get("CIDToGIDMap") {
            None => CidToGidMap::Identity,
            Some(value) => {
                let value = self.resolve_value(value)?;
                match value {
                    Value::Name(name) if name == "Identity" => CidToGidMap::Identity,
                    Value::Stream(_, _) => {
                        let bytes = self.decode_stream_bytes(&value).map_err(|error| {
                            FontError::Invalid(format!(
                                "failed to decode CID font /CIDToGIDMap stream: {error}"
                            ))
                        })?;
                        if bytes.len() % 2 != 0 {
                            return Err(FontError::Invalid(
                                "CID font /CIDToGIDMap stream has an odd byte length".into(),
                            ));
                        }
                        if bytes.len() > (usize::from(u16::MAX) + 1) * 2 {
                            return Err(FontError::Invalid(
                                "CID font /CIDToGIDMap stream exceeds the CID range".into(),
                            ));
                        }
                        CidToGidMap::Explicit(bytes)
                    }
                    _ => {
                        return Err(FontError::Invalid(
                            "CID font /CIDToGIDMap is neither /Identity nor a stream".into(),
                        ));
                    }
                }
            }
        };

        let true_type_font = self
            .cid_font_type2_program(&value)?
            .map(|program| TrueTypeFont::parse(program.data))
            .transpose()?;
        Ok(Some(CidFontType2 {
            base_font: metrics.base_font,
            system_info: metrics.system_info,
            default_width: metrics.default_width,
            widths: metrics.widths,
            default_vertical_metrics: metrics.default_vertical_metrics,
            vertical_metrics: metrics.vertical_metrics,
            descriptor: metrics.descriptor,
            cid_to_gid,
            true_type_font,
        }))
    }
}

/// CID-keyed font metrics shared by CIDFontType0 and CIDFontType2.
pub(super) struct CidMetrics {
    pub(super) base_font: String,
    pub(super) system_info: CidSystemInfo,
    pub(super) default_width: f64,
    pub(super) widths: BTreeMap<u16, f64>,
    pub(super) default_vertical_metrics: DefaultVerticalMetrics,
    pub(super) vertical_metrics: BTreeMap<u16, VerticalMetrics>,
    pub(super) descriptor: FontDescriptorMetrics,
}

/// Parse the metrics common to both CID-keyed descendant font types.
pub(super) fn parse_cid_metrics(document: &Document, dict: &Dict) -> FontResult<CidMetrics> {
    let base_font = dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .ok_or_else(|| FontError::Invalid("missing or invalid CID font /BaseFont".into()))?
        .to_owned();
    let system_info = parse_cid_system_info(document, dict)?;
    let descriptor = dict
        .get("FontDescriptor")
        .ok_or_else(|| FontError::Invalid("CID font is missing /FontDescriptor".into()))?;
    let descriptor = document.resolve_value(descriptor)?;
    let descriptor = descriptor
        .as_dict()
        .ok_or_else(|| FontError::Invalid("CID font /FontDescriptor is not a dictionary".into()))?;
    if descriptor.get("Type").and_then(Value::as_name) != Some("FontDescriptor") {
        return Err(FontError::Invalid(
            "CID font /FontDescriptor has a missing or invalid /Type".into(),
        ));
    }
    let descriptor = parse_descriptor(document, descriptor)?;

    let default_width = match dict.get("DW") {
        Some(value) => {
            let value = document.resolve_value(value)?;
            number(&value)
                .ok_or_else(|| FontError::Invalid("CID font /DW is not a finite number".into()))?
        }
        None => 1000.0,
    };
    let widths = match dict.get("W") {
        Some(value) => {
            let value = document.resolve_value(value)?;
            let array = value
                .as_array()
                .ok_or_else(|| FontError::Invalid("CID font /W is not an array".into()))?;
            parse_widths(document, array)?
        }
        None => BTreeMap::new(),
    };
    let default_vertical_metrics = parse_default_vertical_metrics(document, dict)?;
    let vertical_metrics = match dict.get("W2") {
        Some(value) => {
            let value = document.resolve_value(value)?;
            let array = value
                .as_array()
                .ok_or_else(|| FontError::Invalid("CID font /W2 is not an array".into()))?;
            parse_vertical_metrics(document, array)?
        }
        None => BTreeMap::new(),
    };
    Ok(CidMetrics {
        base_font,
        system_info,
        default_width,
        widths,
        default_vertical_metrics,
        vertical_metrics,
        descriptor,
    })
}

fn parse_default_vertical_metrics(
    document: &Document,
    dict: &Dict,
) -> FontResult<DefaultVerticalMetrics> {
    let Some(value) = dict.get("DW2") else {
        return Ok(DefaultVerticalMetrics::default());
    };
    let value = document.resolve_value(value)?;
    let array = value
        .as_array()
        .ok_or_else(|| FontError::Invalid("CID font /DW2 is not an array".into()))?;
    if array.len() != 2 {
        return Err(FontError::Invalid(
            "CID font /DW2 must contain exactly two numbers".into(),
        ));
    }
    Ok(DefaultVerticalMetrics {
        v_y: metric_number(document, &array.0[0], "/DW2")?,
        w1_y: metric_number(document, &array.0[1], "/DW2")?,
    })
}

#[cfg(test)]
mod tests;
