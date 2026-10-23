use std::collections::BTreeMap;

use crate::pdf::types::{Document, Value};

use super::{FontError, FontResult, cid::parse_cid_metrics, embedded_program::EmbeddedFontProgram};

/// Parsed metrics and embedded program for a PDF CIDFontType0 descendant.
///
/// CIDFontType0 uses CID-keyed Type 1/CFF outlines. Glyphs are selected by
/// CID inside the font program, so no `/CIDToGIDMap` exists. This type parses
/// the dictionary, `/W`/`/DW` metrics, and the decoded `/FontFile3` stream;
/// CFF outline interpretation is not implemented.
#[derive(Debug, Clone, PartialEq)]
pub struct CidFontType0 {
    base_font: String,
    default_width: f64,
    widths: BTreeMap<u16, f64>,
    font_program: Option<EmbeddedFontProgram>,
}

impl CidFontType0 {
    /// Return the descendant font's `/BaseFont` name.
    pub fn base_font(&self) -> &str {
        &self.base_font
    }

    /// Return `/DW`, defaulting to 1000 when omitted.
    pub fn default_width(&self) -> f64 {
        self.default_width
    }

    /// Return the horizontal width for a CID using `/W` and `/DW`.
    pub fn width_for_cid(&self, cid: u16) -> f64 {
        self.widths.get(&cid).copied().unwrap_or(self.default_width)
    }

    /// Return the decoded `/FontFile3` program when present.
    ///
    /// The program `/Subtype` is `/CIDFontType0C` or `/OpenType`.
    pub fn font_program(&self) -> Option<&EmbeddedFontProgram> {
        self.font_program.as_ref()
    }
}

impl Document {
    /// Parse a CIDFontType0 descendant font's metrics and embedded program.
    ///
    /// `/W` and `/DW` provide widths in glyph-space units. An embedded
    /// `/FontFile3` stream is decoded; its `/Subtype` must be
    /// `/CIDFontType0C` or `/OpenType`.
    ///
    /// Returns `Ok(None)` for values whose `/Subtype` is not `/CIDFontType0`.
    pub fn cid_font_type0(&self, value: &Value) -> FontResult<Option<CidFontType0>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("CID font value is not a dictionary".into()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("CIDFontType0") {
            return Ok(None);
        }

        let metrics = parse_cid_metrics(self, dict)?;
        let font_program = match self.cid_font_type0_program(&value)? {
            Some(program) => match program.subtype.as_deref() {
                Some("CIDFontType0C") | Some("OpenType") => Some(program),
                Some(other) => {
                    return Err(FontError::Invalid(format!(
                        "unsupported CIDFontType0 /FontFile3 /Subtype /{other}"
                    )));
                }
                None => {
                    return Err(FontError::Invalid(
                        "CIDFontType0 /FontFile3 stream is missing /Subtype".into(),
                    ));
                }
            },
            None => None,
        };
        Ok(Some(CidFontType0 {
            base_font: metrics.base_font,
            default_width: metrics.default_width,
            widths: metrics.widths,
            font_program,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::pdf::types::{Array, Dict, Document, StreamData, Value};

    use super::super::FontProgramSource;

    fn cid_system_info() -> Value {
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String("Adobe".into()))
                .insert("Ordering", Value::String("Identity".into()))
                .insert("Supplement", Value::Integer(0)),
        )
    }

    fn cid_font(entries: Vec<(&str, Value)>) -> Value {
        cid_font_with_descriptor(entries, Vec::new())
    }

    fn cid_font_with_descriptor(
        entries: Vec<(&str, Value)>,
        descriptor_entries: Vec<(&str, Value)>,
    ) -> Value {
        let mut descriptor = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
        for (key, value) in descriptor_entries {
            descriptor = descriptor.insert(key, value);
        }
        let mut dict = Dict::new()
            .insert("Subtype", Value::Name("CIDFontType0".into()))
            .insert("BaseFont", Value::Name("ExampleCIDType0".into()))
            .insert("CIDSystemInfo", cid_system_info())
            .insert("FontDescriptor", Value::Dict(descriptor));
        for (key, value) in entries {
            dict = dict.insert(key, value);
        }
        Value::Dict(dict)
    }

    fn font_file3(subtype: &str, data: Vec<u8>) -> Value {
        Value::Stream(
            Dict::new().insert("Subtype", Value::Name(subtype.into())),
            StreamData::Owned(data),
        )
    }

    #[test]
    fn parses_metrics_and_cid_keyed_cff_program() {
        let document = Document::new();
        let font = cid_font_with_descriptor(
            vec![
                (
                    "W",
                    Value::Array(
                        Array::new()
                            .push(Value::Integer(1))
                            .push(Value::Array(
                                Array::new()
                                    .push(Value::Integer(500))
                                    .push(Value::Float(510.5)),
                            ))
                            .push(Value::Integer(5))
                            .push(Value::Integer(6))
                            .push(Value::Integer(700)),
                    ),
                ),
                ("DW", Value::Integer(950)),
            ],
            vec![("FontFile3", font_file3("CIDFontType0C", b"CFF ".to_vec()))],
        );

        let font = document.cid_font_type0(&font).unwrap().unwrap();
        assert_eq!(font.base_font(), "ExampleCIDType0");
        assert_eq!(font.default_width(), 950.0);
        assert_eq!(font.width_for_cid(1), 500.0);
        assert_eq!(font.width_for_cid(2), 510.5);
        assert_eq!(font.width_for_cid(5), 700.0);
        assert_eq!(font.width_for_cid(6), 700.0);
        assert_eq!(font.width_for_cid(7), 950.0);
        let program = font.font_program().unwrap();
        assert_eq!(program.source, FontProgramSource::FontFile3);
        assert_eq!(program.subtype.as_deref(), Some("CIDFontType0C"));
        assert_eq!(program.data, b"CFF ");
    }

    #[test]
    fn accepts_opentype_fontfile3_subtype() {
        let document = Document::new();
        let font = cid_font_with_descriptor(
            Vec::new(),
            vec![("FontFile3", font_file3("OpenType", b"OTTO".to_vec()))],
        );
        let font = document.cid_font_type0(&font).unwrap().unwrap();
        assert_eq!(
            font.font_program().unwrap().subtype.as_deref(),
            Some("OpenType")
        );
    }

    #[test]
    fn defaults_width_and_omits_program_when_absent() {
        let document = Document::new();
        let font = document
            .cid_font_type0(&cid_font(Vec::new()))
            .unwrap()
            .unwrap();
        assert_eq!(font.default_width(), 1000.0);
        assert_eq!(font.width_for_cid(42), 1000.0);
        assert!(font.font_program().is_none());
    }

    #[test]
    fn rejects_unsupported_fontfile3_subtypes() {
        let document = Document::new();
        let font = cid_font_with_descriptor(
            Vec::new(),
            vec![("FontFile3", font_file3("Type1C", Vec::new()))],
        );
        assert!(document.cid_font_type0(&font).is_err());
    }

    #[test]
    fn rejects_missing_descriptor_and_cid_system_info() {
        let document = Document::new();
        let missing_info = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType0".into()))
                .insert("BaseFont", Value::Name("MissingInfo".into()))
                .insert(
                    "FontDescriptor",
                    Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
                ),
        );
        assert!(document.cid_font_type0(&missing_info).is_err());

        let missing_descriptor = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType0".into()))
                .insert("BaseFont", Value::Name("MissingDescriptor".into()))
                .insert("CIDSystemInfo", cid_system_info()),
        );
        assert!(document.cid_font_type0(&missing_descriptor).is_err());
    }

    #[test]
    fn ignores_other_font_subtypes() {
        let document = Document::new();
        let value = Value::Dict(Dict::new().insert("Subtype", Value::Name("CIDFontType2".into())));
        assert!(document.cid_font_type0(&value).unwrap().is_none());
    }
}
