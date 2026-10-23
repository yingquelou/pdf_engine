//! Font descriptor metrics shared by simple and CID-keyed fonts.

use crate::core::types::{Dict, Document, PdfTextString, Value};

use super::{FontError, FontResult, number};

/// Metrics parsed from a font's `/FontDescriptor`.
///
/// Covers the numeric and name entries shared by simple and CID-keyed fonts.
/// `/CIDSet`, which only applies to CID fonts, is decoded to its bit array;
/// see [`FontDescriptorMetrics::cid_is_present`]. Embedded font programs are
/// handled separately (see [`super::EmbeddedFontProgram`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FontDescriptorMetrics {
    /// Descriptor `/FontName`, when present.
    pub font_name: Option<String>,
    /// Descriptor `/FontFamily` text, when present.
    pub font_family: Option<String>,
    /// Descriptor `/FontStretch` name, when present.
    pub font_stretch: Option<String>,
    /// Descriptor `/FontWeight`, when present.
    pub font_weight: Option<f64>,
    /// Descriptor `/Flags`, when present.
    pub flags: Option<u32>,
    /// Descriptor `/FontBBox`, when present.
    pub font_bbox: Option<[f64; 4]>,
    /// Descriptor `/ItalicAngle`, when present.
    pub italic_angle: Option<f64>,
    /// Descriptor `/Ascent`, when present.
    pub ascent: Option<f64>,
    /// Descriptor `/Descent`, when present.
    pub descent: Option<f64>,
    /// Descriptor `/Leading`, when present.
    pub leading: Option<f64>,
    /// Descriptor `/CapHeight`, when present.
    pub cap_height: Option<f64>,
    /// Descriptor `/XHeight`, when present.
    pub x_height: Option<f64>,
    /// Descriptor `/StemV`, when present.
    pub stem_v: Option<f64>,
    /// Descriptor `/StemH`, when present.
    pub stem_h: Option<f64>,
    /// Descriptor `/AvgWidth`, when present.
    pub avg_width: Option<f64>,
    /// Descriptor `/MaxWidth`, when present.
    pub max_width: Option<f64>,
    /// Descriptor `/MissingWidth`, when present.
    pub missing_width: Option<f64>,
    /// Descriptor `/CharSet` text, when present.
    pub char_set: Option<String>,
    /// Decoded `/CIDSet` bit array (most-significant bit first), when present.
    pub cid_set: Option<Vec<u8>>,
}

impl FontDescriptorMetrics {
    fn flag_set(&self, bit: u32) -> bool {
        self.flags
            .is_some_and(|flags| flags & (1 << (bit - 1)) != 0)
    }

    /// Whether PDF flag bit 1 (`FixedPitch`) is set.
    pub fn is_fixed_pitch(&self) -> bool {
        self.flag_set(1)
    }

    /// Whether PDF flag bit 2 (`Serif`) is set.
    pub fn is_serif(&self) -> bool {
        self.flag_set(2)
    }

    /// Whether PDF flag bit 3 (`Symbolic`) is set.
    pub fn is_symbolic(&self) -> bool {
        self.flag_set(3)
    }

    /// Whether PDF flag bit 4 (`Script`) is set.
    pub fn is_script(&self) -> bool {
        self.flag_set(4)
    }

    /// Whether PDF flag bit 6 (`Nonsymbolic`) is set.
    pub fn is_nonsymbolic(&self) -> bool {
        self.flag_set(6)
    }

    /// Whether PDF flag bit 7 (`Italic`) is set.
    pub fn is_italic(&self) -> bool {
        self.flag_set(7)
    }

    /// Whether PDF flag bit 17 (`AllCap`) is set.
    pub fn is_all_cap(&self) -> bool {
        self.flag_set(17)
    }

    /// Whether PDF flag bit 18 (`SmallCap`) is set.
    pub fn is_small_cap(&self) -> bool {
        self.flag_set(18)
    }

    /// Whether PDF flag bit 19 (`ForceBold`) is set.
    pub fn is_force_bold(&self) -> bool {
        self.flag_set(19)
    }

    /// Whether a CID is marked present in `/CIDSet`.
    ///
    /// Returns `None` when the descriptor has no `/CIDSet` or the bit array
    /// is too short to cover `cid`.
    pub fn cid_is_present(&self, cid: u16) -> Option<bool> {
        let cid_set = self.cid_set.as_ref()?;
        let byte = *cid_set.get(usize::from(cid) / 8)?;
        Some(byte & (0x80 >> (cid % 8)) != 0)
    }
}

pub(super) fn parse_descriptor(
    document: &Document,
    dict: &Dict,
) -> FontResult<FontDescriptorMetrics> {
    if dict.get("Type").and_then(Value::as_name) != Some("FontDescriptor") {
        return Err(FontError::Invalid(
            "/FontDescriptor has a missing or invalid /Type".to_string(),
        ));
    }
    Ok(FontDescriptorMetrics {
        font_name: optional_name(document, dict, "FontName")?,
        font_family: optional_text(document, dict, "FontFamily")?,
        font_stretch: optional_name(document, dict, "FontStretch")?,
        font_weight: optional_number(document, dict, "FontWeight")?,
        flags: optional_flags(document, dict)?,
        font_bbox: optional_bbox(document, dict, "FontBBox")?,
        italic_angle: optional_number(document, dict, "ItalicAngle")?,
        ascent: optional_number(document, dict, "Ascent")?,
        descent: optional_number(document, dict, "Descent")?,
        leading: optional_number(document, dict, "Leading")?,
        cap_height: optional_number(document, dict, "CapHeight")?,
        x_height: optional_number(document, dict, "XHeight")?,
        stem_v: optional_number(document, dict, "StemV")?,
        stem_h: optional_number(document, dict, "StemH")?,
        avg_width: optional_number(document, dict, "AvgWidth")?,
        max_width: optional_number(document, dict, "MaxWidth")?,
        missing_width: optional_number(document, dict, "MissingWidth")?,
        char_set: optional_text(document, dict, "CharSet")?,
        cid_set: optional_cid_set(document, dict)?,
    })
}

fn optional_name(document: &Document, dict: &Dict, key: &str) -> FontResult<Option<String>> {
    match optional_value(document, dict, key)? {
        Some(Value::Name(value)) => Ok(Some(value.clone())),
        Some(_) => Err(FontError::Invalid(format!(
            "invalid /FontDescriptor /{key}"
        ))),
        None => Ok(None),
    }
}

fn optional_text(document: &Document, dict: &Dict, key: &str) -> FontResult<Option<String>> {
    match optional_value(document, dict, key)? {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::HexString(value)) => {
            let bytes = hex_bytes(&value)
                .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}")))?;
            PdfTextString::decode(&bytes)
                .map(|text| Some(text.as_str().to_owned()))
                .map_err(|_| FontError::Invalid(format!("invalid /FontDescriptor /{key}")))
        }
        Some(_) => Err(FontError::Invalid(format!(
            "invalid /FontDescriptor /{key}"
        ))),
        None => Ok(None),
    }
}

fn optional_value(document: &Document, dict: &Dict, key: &str) -> FontResult<Option<Value>> {
    dict.get(key)
        .map(|value| document.resolve_value(value))
        .transpose()
}

fn hex_bytes(value: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for index in (0..digits.len()).step_by(2) {
        bytes.push(nibble(digits[index])? << 4 | nibble(digits[index + 1])?);
    }
    Some(bytes)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn optional_flags(document: &Document, dict: &Dict) -> FontResult<Option<u32>> {
    match optional_value(document, dict, "Flags")? {
        Some(value) => {
            let flags = value
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| FontError::Invalid("invalid /FontDescriptor /Flags".to_string()))?;
            Ok(Some(flags))
        }
        None => Ok(None),
    }
}

fn optional_number(document: &Document, dict: &Dict, key: &str) -> FontResult<Option<f64>> {
    match optional_value(document, dict, key)? {
        Some(value) => number(&value)
            .map(Some)
            .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}"))),
        None => Ok(None),
    }
}

fn optional_bbox(document: &Document, dict: &Dict, key: &str) -> FontResult<Option<[f64; 4]>> {
    let Some(value) = optional_value(document, dict, key)? else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}")))?;
    if values.len() != 4 {
        return Err(FontError::Invalid(format!(
            "/FontDescriptor /{key} must contain four numbers"
        )));
    }
    let mut bbox = [0.0; 4];
    for (index, value) in values.iter().enumerate() {
        bbox[index] = number(value)
            .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}")))?;
    }
    Ok(Some(bbox))
}

fn optional_cid_set(document: &Document, dict: &Dict) -> FontResult<Option<Vec<u8>>> {
    let Some(value) = dict.get("CIDSet") else {
        return Ok(None);
    };
    let value = document.resolve_value(value)?;
    if !matches!(value, Value::Stream(_, _)) {
        return Err(FontError::Invalid(
            "/FontDescriptor /CIDSet is not a stream".to_string(),
        ));
    }
    let data = document.decode_stream_bytes(&value).map_err(|error| {
        FontError::Invalid(format!(
            "failed to decode /FontDescriptor /CIDSet stream: {error}"
        ))
    })?;
    Ok(Some(data))
}
