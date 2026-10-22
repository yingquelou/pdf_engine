//! Semantic access to simple Type 1 font dictionaries.

use std::fmt;

use crate::pdf::types::{Dict, Document, Value};

use super::ParseError;

type FontResult<T> = std::result::Result<T, FontError>;

/// Metrics parsed from a Type 1 font's `/FontDescriptor`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontDescriptorMetrics {
    /// Descriptor `/FontName`, when present.
    pub font_name: Option<String>,
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
    /// Descriptor `/CapHeight`, when present.
    pub cap_height: Option<f64>,
    /// Descriptor `/StemV`, when present.
    pub stem_v: Option<f64>,
    /// Descriptor `/MissingWidth`, when present.
    pub missing_width: Option<f64>,
}

/// Resolved metrics from a simple Type 1 font dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1Font {
    /// Font dictionary `/BaseFont`.
    pub base_font: String,
    /// First code covered by `/Widths`; absent for a standard 14 font without
    /// an explicit width array.
    pub first_char: Option<u8>,
    /// Widths in text-space units, indexed from `first_char`.
    pub widths: Vec<f64>,
    /// Parsed `/FontDescriptor` metrics, when the font dictionary provides one.
    pub descriptor: Option<FontDescriptorMetrics>,
}

impl Type1Font {
    /// Return the glyph width for a one-byte character code when available.
    pub fn width_for(&self, code: u8) -> Option<f64> {
        let index = usize::from(code).checked_sub(usize::from(self.first_char?))?;
        self.widths.get(index).copied()
    }
}

#[derive(Debug)]
pub enum FontError {
    /// An indirect object referenced by the font could not be parsed.
    Parse(ParseError),
    /// A font dictionary or metric does not satisfy the supported structure.
    Invalid(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid Type 1 font: {message}"),
        }
    }
}

impl std::error::Error for FontError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<ParseError> for FontError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl Document {
    /// Resolve a Type 1 font dictionary and its metrics from direct or indirect values.
    ///
    /// Returns `Ok(None)` for non-Type-1 font dictionaries. Malformed font data,
    /// dangling references, and object-generation mismatches are reported.
    /// Standard 14 fonts may omit `/Widths`; built-in width tables and embedded
    /// font programs are not loaded by this API.
    pub fn type1_font(&self, value: &Value) -> FontResult<Option<Type1Font>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type1") {
            return Ok(None);
        }

        let base_font = dict
            .get("BaseFont")
            .and_then(Value::as_name)
            .ok_or_else(|| FontError::Invalid("missing or invalid /BaseFont".to_string()))?
            .to_string();
        let (first_char, widths) = self.read_widths(dict)?;
        let standard_font = is_standard_14(&base_font);
        if first_char.is_none() && !standard_font {
            return Err(FontError::Invalid(
                "non-standard Type 1 fonts require /FirstChar, /LastChar, and /Widths".to_string(),
            ));
        }
        let descriptor = match dict.get("FontDescriptor") {
            Some(value) => {
                let descriptor = self.resolve_value(value)?;
                let descriptor = descriptor.as_dict().ok_or_else(|| {
                    FontError::Invalid("/FontDescriptor is not a dictionary".to_string())
                })?;
                Some(parse_descriptor(descriptor)?)
            }
            None => None,
        };
        if descriptor.is_none() && !standard_font {
            return Err(FontError::Invalid(
                "non-standard Type 1 fonts require /FontDescriptor".to_string(),
            ));
        }

        Ok(Some(Type1Font {
            base_font,
            first_char,
            widths,
            descriptor,
        }))
    }

    fn read_widths(&self, dict: &Dict) -> FontResult<(Option<u8>, Vec<f64>)> {
        let first = dict.get("FirstChar");
        let last = dict.get("LastChar");
        let widths = dict.get("Widths");
        if first.is_none() && last.is_none() && widths.is_none() {
            return Ok((None, Vec::new()));
        }

        let first = first
            .and_then(Value::as_integer)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| FontError::Invalid("invalid or missing /FirstChar".to_string()))?;
        let last = last
            .and_then(Value::as_integer)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| FontError::Invalid("invalid or missing /LastChar".to_string()))?;
        if last < first {
            return Err(FontError::Invalid(
                "/LastChar precedes /FirstChar".to_string(),
            ));
        }
        let widths = self.resolve_value(
            widths.ok_or_else(|| FontError::Invalid("missing /Widths".to_string()))?,
        )?;
        let widths = widths
            .as_array()
            .ok_or_else(|| FontError::Invalid("/Widths is not an array".to_string()))?;
        let expected_len = usize::from(last - first) + 1;
        if widths.len() != expected_len {
            return Err(FontError::Invalid(format!(
                "/Widths contains {} entries; expected {expected_len}",
                widths.len()
            )));
        }
        let widths = widths
            .iter()
            .map(|value| {
                number(value).ok_or_else(|| {
                    FontError::Invalid(
                        "/Widths contains a non-finite or non-numeric value".to_string(),
                    )
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok((Some(first), widths))
    }

    fn resolve_value(&self, value: &Value) -> FontResult<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self
                    .get_object(*num)?
                    .filter(|object| object.generation == *generation)
                    .ok_or_else(|| {
                        FontError::Invalid(format!(
                            "unresolved object reference {num} {generation} R"
                        ))
                    })?;
                Ok(object.value)
            }
            _ => Ok(value.clone()),
        }
    }
}

fn parse_descriptor(dict: &Dict) -> FontResult<FontDescriptorMetrics> {
    if dict.get("Type").and_then(Value::as_name) != Some("FontDescriptor") {
        return Err(FontError::Invalid(
            "/FontDescriptor has a missing or invalid /Type".to_string(),
        ));
    }
    let font_name = optional_name(dict, "FontName")?;
    let flags = match dict.get("Flags") {
        Some(value) => {
            let flags = value
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| FontError::Invalid("invalid /FontDescriptor /Flags".to_string()))?;
            Some(flags)
        }
        None => None,
    };
    Ok(FontDescriptorMetrics {
        font_name,
        flags,
        font_bbox: optional_bbox(dict, "FontBBox")?,
        italic_angle: optional_number(dict, "ItalicAngle")?,
        ascent: optional_number(dict, "Ascent")?,
        descent: optional_number(dict, "Descent")?,
        cap_height: optional_number(dict, "CapHeight")?,
        stem_v: optional_number(dict, "StemV")?,
        missing_width: optional_number(dict, "MissingWidth")?,
    })
}

fn optional_name(dict: &Dict, key: &str) -> FontResult<Option<String>> {
    match dict.get(key) {
        Some(Value::Name(value)) => Ok(Some(value.clone())),
        Some(_) => Err(FontError::Invalid(format!(
            "invalid /FontDescriptor /{key}"
        ))),
        None => Ok(None),
    }
}

fn optional_number(dict: &Dict, key: &str) -> FontResult<Option<f64>> {
    match dict.get(key) {
        Some(value) => number(value)
            .map(Some)
            .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}"))),
        None => Ok(None),
    }
}

fn optional_bbox(dict: &Dict, key: &str) -> FontResult<Option<[f64; 4]>> {
    let Some(value) = dict.get(key) else {
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

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Float(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn is_standard_14(name: &str) -> bool {
    matches!(
        name,
        "Courier"
            | "Courier-Bold"
            | "Courier-Oblique"
            | "Courier-BoldOblique"
            | "Helvetica"
            | "Helvetica-Bold"
            | "Helvetica-Oblique"
            | "Helvetica-BoldOblique"
            | "Times-Roman"
            | "Times-Bold"
            | "Times-Italic"
            | "Times-BoldItalic"
            | "Symbol"
            | "ZapfDingbats"
    )
}
