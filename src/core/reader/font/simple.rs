//! Type 1 simple-font metrics and simple-font encoding resolution.

use std::collections::BTreeMap;

use crate::core::types::{Array, Dict, Document, Value};

use super::{
    FontDescriptorMetrics, FontError, FontResult, PredefinedEncoding, SimpleFontEncoding,
    default_encoding, number, parse_descriptor, predefined_encoding, standard14_metrics,
};

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
        if let Some(first_char) = self.first_char
            && let Some(index) = usize::from(code).checked_sub(usize::from(first_char))
            && let Some(width) = self.widths.get(index)
        {
            return Some(*width);
        }
        self.descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.missing_width)
            .or_else(|| standard14_width(&self.base_font, code))
    }
}

impl Document {
    /// Resolve a Type 1 font dictionary and its metrics from direct or indirect values.
    ///
    /// Returns `Ok(None)` for non-Type-1 font dictionaries. Malformed font data,
    /// dangling references, and object-generation mismatches are reported.
    /// Standard 14 fonts may omit `/Widths`; available built-in metrics provide
    /// fallback widths, but embedded font programs are not loaded by this API.
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
                Some(parse_descriptor(self, descriptor)?)
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

    /// Resolve a simple font's predefined `/Encoding` and `/Differences`.
    ///
    /// Accepts the three simple-font subtypes that carry a single-byte
    /// encoding: Type 1, MMType1, and TrueType. Returns `Ok(None)` for other
    /// subtypes.
    ///
    /// When `/Encoding` is absent, the encoding implied by `/BaseFont` is used
    /// (StandardEncoding, or Symbol/ZapfDingbats for those base fonts). The
    /// embedded font program's built-in encoding is not consulted, so a
    /// symbolic TrueType font that relies solely on its built-in cmap may
    /// decode through StandardEncoding instead.
    pub fn simple_font_encoding(&self, value: &Value) -> FontResult<Option<SimpleFontEncoding>> {
        self.simple_font_encoding_for(value, &["Type1", "MMType1", "TrueType"])
    }

    /// Resolve a Type 1 font's predefined `/Encoding` and `/Differences`.
    ///
    /// Returns `Ok(None)` for non-Type-1 fonts. The existing [`Type1Font`]
    /// structure remains unchanged; callers can opt into encoding semantics
    /// through this separate method.
    pub fn type1_font_encoding(&self, value: &Value) -> FontResult<Option<SimpleFontEncoding>> {
        self.simple_font_encoding_for(value, &["Type1"])
    }

    fn simple_font_encoding_for(
        &self,
        value: &Value,
        subtypes: &[&str],
    ) -> FontResult<Option<SimpleFontEncoding>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        let subtype = dict.get("Subtype").and_then(Value::as_name);
        if !subtype.is_some_and(|subtype| subtypes.contains(&subtype)) {
            return Ok(None);
        }
        let base_font = dict
            .get("BaseFont")
            .and_then(Value::as_name)
            .ok_or_else(|| FontError::Invalid("missing or invalid /BaseFont".to_string()))?;
        self.parse_encoding(dict, default_encoding(base_font))
            .map(Some)
    }

    fn parse_encoding(
        &self,
        dict: &Dict,
        default_encoding: PredefinedEncoding,
    ) -> FontResult<SimpleFontEncoding> {
        let Some(encoding_value) = dict.get("Encoding") else {
            return Ok(SimpleFontEncoding {
                base_encoding: default_encoding,
                differences: BTreeMap::new(),
            });
        };
        let encoding = self.resolve_value(encoding_value)?;
        match encoding {
            Value::Name(name) => Ok(SimpleFontEncoding {
                base_encoding: predefined_encoding(&name)?,
                differences: BTreeMap::new(),
            }),
            Value::Dict(encoding_dict) => {
                let base_encoding = match encoding_dict.get("BaseEncoding") {
                    Some(value) => {
                        let value = self.resolve_value(value)?;
                        let name = value.as_name().ok_or_else(|| {
                            FontError::Invalid("/Encoding /BaseEncoding is not a name".to_string())
                        })?;
                        predefined_encoding(name)?
                    }
                    None => default_encoding,
                };
                let differences = match encoding_dict.get("Differences") {
                    Some(value) => {
                        let value = self.resolve_value(value)?;
                        parse_differences(value.as_array().ok_or_else(|| {
                            FontError::Invalid("/Encoding /Differences is not an array".into())
                        })?)?
                    }
                    None => BTreeMap::new(),
                };
                Ok(SimpleFontEncoding {
                    base_encoding,
                    differences,
                })
            }
            _ => Err(FontError::Invalid(
                "/Encoding is neither a name nor a dictionary".to_string(),
            )),
        }
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
}

fn parse_differences(values: &Array) -> FontResult<BTreeMap<u8, String>> {
    let mut differences = BTreeMap::new();
    let mut next_code = None;
    let mut values = values.iter().peekable();
    while let Some(value) = values.next() {
        match value {
            Value::Integer(code) => {
                next_code = Some(u8::try_from(*code).map_err(|_| {
                    FontError::Invalid(
                        "/Encoding /Differences code is outside the byte range".to_string(),
                    )
                })?);
            }
            Value::Name(name) => {
                let code = next_code.ok_or_else(|| {
                    FontError::Invalid(
                        "/Encoding /Differences glyph name has no preceding code".to_string(),
                    )
                })?;
                differences.insert(code, name.clone());
                next_code = code.checked_add(1);
                if next_code.is_none() && matches!(values.peek(), Some(Value::Name(_))) {
                    return Err(FontError::Invalid(
                        "/Encoding /Differences assigns a glyph beyond code 255".to_string(),
                    ));
                }
            }
            _ => {
                return Err(FontError::Invalid(
                    "/Encoding /Differences contains a non-integer, non-name value".to_string(),
                ));
            }
        }
    }
    Ok(differences)
}

fn standard14_width(base_font: &str, code: u8) -> Option<f64> {
    match base_font {
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => Some(600.0),
        "Helvetica" => standard14_metrics::helvetica_width(code, false, false),
        "Helvetica-Bold" => standard14_metrics::helvetica_width(code, true, false),
        "Helvetica-Oblique" => standard14_metrics::helvetica_width(code, false, true),
        "Helvetica-BoldOblique" => standard14_metrics::helvetica_width(code, true, true),
        "Times-Roman" => standard14_metrics::times_width(code, false, false),
        "Times-Bold" => standard14_metrics::times_width(code, true, false),
        "Times-Italic" => standard14_metrics::times_width(code, false, true),
        "Times-BoldItalic" => standard14_metrics::times_width(code, true, true),
        "Symbol" => standard14_metrics::symbol_width(code),
        "ZapfDingbats" => standard14_metrics::zapf_dingbats_width(code),
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
