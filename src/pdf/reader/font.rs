//! Semantic access to PDF font dictionaries and embedded font programs.

use std::{collections::BTreeMap, fmt};

use crate::pdf::types::{Array, Dict, Document, Value};

use super::ParseError;

mod cid;
mod cid_cmap;
mod cid_type0;
mod embedded_program;
mod encoding_data;
mod encoding_unicode;
mod standard14_metrics;
mod truetype;
mod type0;

pub use cid::CidFontType2;
pub use cid_type0::CidFontType0;
pub use embedded_program::{EmbeddedFontProgram, FontProgramSource};
pub use truetype::{CmapFormat, GlyphPoint, SfntFlavor, TrueTypeFont, TrueTypeGlyph};
pub use type0::{CidDescendant, Type0DecodedCode, Type0Font};

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

/// A predefined encoding used by a simple font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredefinedEncoding {
    Standard,
    WinAnsi,
    MacRoman,
    Symbol,
    ZapfDingbats,
}

/// How a character code is interpreted by a simple font encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingMapping<'a> {
    /// The code is interpreted by the selected predefined encoding.
    Predefined {
        encoding: PredefinedEncoding,
        code: u8,
    },
    /// The font's `/Differences` array assigns this glyph name to the code.
    GlyphName(&'a str),
}

/// Parsed `/Encoding` information for a Type 1 simple font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleFontEncoding {
    base_encoding: PredefinedEncoding,
    differences: BTreeMap<u8, String>,
}

impl SimpleFontEncoding {
    /// Return the encoding used for codes not overridden by `/Differences`.
    pub fn base_encoding(&self) -> PredefinedEncoding {
        self.base_encoding
    }

    /// Return the encoding interpretation for a one-byte character code.
    pub fn mapping_for(&self, code: u8) -> EncodingMapping<'_> {
        match self.differences.get(&code) {
            Some(name) => EncodingMapping::GlyphName(name),
            None => EncodingMapping::Predefined {
                encoding: self.base_encoding,
                code,
            },
        }
    }

    /// Resolve a character code to a glyph name when this encoding knows one.
    ///
    /// StandardEncoding covers its defined ASCII and high-byte glyph slots;
    /// other predefined tables are added independently as their data is
    /// validated.
    pub fn glyph_name_for(&self, code: u8) -> Option<String> {
        match self.mapping_for(code) {
            EncodingMapping::GlyphName(name) => Some(name.to_owned()),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Standard,
                code,
            } => encoding_data::standard_glyph_name(code),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Symbol,
                code,
            } => encoding_data::symbol_name(code).map(str::to_owned),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::ZapfDingbats,
                code,
            } => encoding_data::zapf_dingbats_name(code).map(str::to_owned),
            _ => None,
        }
    }

    /// Resolve a character code to Unicode when its encoding provides a known
    /// mapping. `/Differences` glyph names are resolved for the supported
    /// Adobe glyph names with a single Unicode scalar.
    pub fn unicode_for(&self, code: u8) -> Option<char> {
        match self.mapping_for(code) {
            EncodingMapping::GlyphName(name) => encoding_unicode::unicode_for_glyph_name(name),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::WinAnsi,
                code,
            } => encoding_data::win_ansi_unicode(code),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Standard,
                code,
            } => encoding_data::standard_glyph_name(code)
                .and_then(|name| encoding_unicode::unicode_for_glyph_name(&name)),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::MacRoman,
                code,
            } => encoding_data::mac_roman_unicode(code),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Symbol,
                code,
            } => {
                encoding_data::symbol_name(code).and_then(encoding_unicode::unicode_for_glyph_name)
            }
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::ZapfDingbats,
                code,
            } => encoding_unicode::zapf_dingbats_unicode(code),
        }
    }

    /// Resolve a character code to its Unicode sequence when its encoding
    /// provides a known mapping.
    ///
    /// Adobe Glyph List mappings with multiple Unicode scalars are available
    /// here, while [`Self::unicode_for`] returns only single-scalar mappings.
    pub fn unicode_sequence_for(&self, code: u8) -> Option<String> {
        match self.mapping_for(code) {
            EncodingMapping::GlyphName(name) => {
                encoding_unicode::unicode_sequence_for_glyph_name(name)
            }
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::WinAnsi,
                code,
            } => encoding_data::win_ansi_unicode(code).map(String::from),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Standard,
                code,
            } => encoding_data::standard_glyph_name(code)
                .and_then(|name| encoding_unicode::unicode_sequence_for_glyph_name(&name)),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::MacRoman,
                code,
            } => encoding_data::mac_roman_unicode(code).map(String::from),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Symbol,
                code,
            } => encoding_data::symbol_name(code)
                .and_then(encoding_unicode::unicode_sequence_for_glyph_name),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::ZapfDingbats,
                code,
            } => encoding_unicode::zapf_dingbats_unicode(code).map(String::from),
        }
    }
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
            Self::Invalid(message) => write!(f, "invalid font: {message}"),
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

    /// Resolve a Type 1 font's predefined `/Encoding` and `/Differences`.
    ///
    /// Returns `Ok(None)` for non-Type-1 fonts. The existing [`Type1Font`]
    /// structure remains unchanged; callers can opt into encoding semantics
    /// through this separate method.
    pub fn type1_font_encoding(&self, value: &Value) -> FontResult<Option<SimpleFontEncoding>> {
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
            .ok_or_else(|| FontError::Invalid("missing or invalid /BaseFont".to_string()))?;
        let default_encoding = default_encoding(base_font);
        let Some(encoding_value) = dict.get("Encoding") else {
            return Ok(Some(SimpleFontEncoding {
                base_encoding: default_encoding,
                differences: BTreeMap::new(),
            }));
        };
        let encoding = self.resolve_value(encoding_value)?;
        match encoding {
            Value::Name(name) => Ok(Some(SimpleFontEncoding {
                base_encoding: predefined_encoding(&name)?,
                differences: BTreeMap::new(),
            })),
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
                Ok(Some(SimpleFontEncoding {
                    base_encoding,
                    differences,
                }))
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

fn predefined_encoding(name: &str) -> FontResult<PredefinedEncoding> {
    match name {
        "StandardEncoding" => Ok(PredefinedEncoding::Standard),
        "WinAnsiEncoding" => Ok(PredefinedEncoding::WinAnsi),
        "MacRomanEncoding" => Ok(PredefinedEncoding::MacRoman),
        "SymbolEncoding" => Ok(PredefinedEncoding::Symbol),
        "ZapfDingbatsEncoding" => Ok(PredefinedEncoding::ZapfDingbats),
        _ => Err(FontError::Invalid(format!(
            "unsupported predefined encoding /{name}"
        ))),
    }
}

fn default_encoding(base_font: &str) -> PredefinedEncoding {
    match base_font {
        "Symbol" => PredefinedEncoding::Symbol,
        "ZapfDingbats" => PredefinedEncoding::ZapfDingbats,
        _ => PredefinedEncoding::Standard,
    }
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
