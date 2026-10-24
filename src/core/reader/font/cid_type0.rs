use std::collections::BTreeMap;

use crate::core::types::{Document, Value};

use super::{
    CffOutlineParser, CidSystemInfo, FontDescriptorMetrics, FontError, FontResult, RawCffBackend,
    TtfParserCffBackend, UnavailableCffBackend, cid::parse_cid_metrics,
    embedded_program::EmbeddedFontProgram,
};

/// Parsed metrics and embedded program for a PDF CIDFontType0 descendant.
///
/// CIDFontType0 uses CID-keyed Type 1/CFF outlines. Glyphs are selected by
/// CID inside the font program, so no `/CIDToGIDMap` exists. This type parses
/// the dictionary, `/W`/`/DW` metrics, and the decoded `/FontFile3` stream.
/// Outlines are available through [`Self::cff_outline_parser`] or the fallible
/// [`Self::try_cff_outline_parser`]: `/CIDFontType0C` (and `/Type1C`) programs
/// are parsed as bare CFF by [`RawCffBackend`] with **CID arguments**, while
/// `/OpenType` programs go through [`TtfParserCffBackend`] with GID arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct CidFontType0 {
    base_font: String,
    system_info: CidSystemInfo,
    default_width: f64,
    widths: BTreeMap<u16, f64>,
    descriptor: FontDescriptorMetrics,
    font_program: Option<EmbeddedFontProgram>,
}

impl CidFontType0 {
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

    /// Return the parsed `/FontDescriptor` metrics.
    pub fn descriptor(&self) -> &FontDescriptorMetrics {
        &self.descriptor
    }

    /// Return the decoded `/FontFile3` program when present.
    ///
    /// The program `/Subtype` is `/CIDFontType0C` or `/OpenType`.
    pub fn font_program(&self) -> Option<&EmbeddedFontProgram> {
        self.font_program.as_ref()
    }

    /// Create a CFF outline parser from the embedded font program.
    ///
    /// Returns `None` if no font program is embedded. The backend is selected
    /// from the program `/Subtype`:
    ///
    /// * `/CIDFontType0C` and `/Type1C` use [`RawCffBackend`], which parses
    ///   bare CFF1 data. Its `parse_glyph` argument is a **CID** that is mapped
    ///   to a glyph through the CFF charset;
    /// * `/OpenType` uses [`TtfParserCffBackend`], whose `parse_glyph` argument
    ///   is a glyph ID.
    ///
    /// Parse failures are deferred: the returned backend reports the error from
    /// every outline request. Use [`Self::try_cff_outline_parser`] to receive
    /// the error instead of a lazy failing backend.
    pub fn cff_outline_parser(&self) -> Option<Box<dyn CffOutlineParser>> {
        let program = self.font_program.as_ref()?;
        Some(match outline_backend(program) {
            Ok(backend) => backend,
            Err(error) => Box::new(UnavailableCffBackend::new(error.to_string())),
        })
    }

    /// Create a CFF outline parser, reporting backend construction failures.
    ///
    /// Returns `Ok(None)` if no font program is embedded. This is the fallible
    /// counterpart of [`Self::cff_outline_parser`]; it surfaces truncated or
    /// unsupported `/CIDFontType0C` programs before the first outline request.
    pub fn try_cff_outline_parser(&self) -> FontResult<Option<Box<dyn CffOutlineParser>>> {
        match self.font_program.as_ref() {
            Some(program) => outline_backend(program).map(Some),
            None => Ok(None),
        }
    }
}

/// Build the outline backend that matches an embedded program's `/Subtype`.
///
/// `/CIDFontType0C` and `/Type1C` are bare CFF1 programs and use
/// [`RawCffBackend`]; `/OpenType` is an SFNT container and keeps
/// [`TtfParserCffBackend`]. The document parser rejects `/Type1C` for
/// CIDFontType0 dictionaries (ISO 32000-1 only allows `/CIDFontType0C` and
/// `/OpenType`), but direct program values can still be handed to this
/// function.
fn outline_backend(program: &EmbeddedFontProgram) -> FontResult<Box<dyn CffOutlineParser>> {
    match program.subtype.as_deref() {
        Some("CIDFontType0C") | Some("Type1C") => {
            Ok(Box::new(RawCffBackend::parse(program.data.clone())?))
        }
        Some("OpenType") => Ok(Box::new(TtfParserCffBackend::new(program.data.clone()))),
        Some(other) => Err(FontError::Invalid(format!(
            "unsupported CIDFontType0 /FontFile3 /Subtype /{other}"
        ))),
        None => Err(FontError::Invalid(
            "CIDFontType0 /FontFile3 stream is missing /Subtype".into(),
        )),
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
            system_info: metrics.system_info,
            default_width: metrics.default_width,
            widths: metrics.widths,
            descriptor: metrics.descriptor,
            font_program,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::core::reader::font::cff::fixtures;
    use crate::core::types::{Array, Dict, Document, StreamData, Value};

    use super::super::{EmbeddedFontProgram, FontProgramSource};
    use super::outline_backend;

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
            vec![
                ("FontName", Value::Name("ExampleCIDType0".into())),
                ("FontFamily", Value::String("Example Family".into())),
                ("Flags", Value::Integer(4)),
                ("Ascent", Value::Integer(800)),
                ("FontFile3", font_file3("CIDFontType0C", b"CFF ".to_vec())),
            ],
        );

        let font = document.cid_font_type0(&font).unwrap().unwrap();
        assert_eq!(font.base_font(), "ExampleCIDType0");
        assert_eq!(font.default_width(), 950.0);
        let descriptor = font.descriptor();
        assert_eq!(descriptor.font_name.as_deref(), Some("ExampleCIDType0"));
        assert_eq!(descriptor.font_family.as_deref(), Some("Example Family"));
        assert_eq!(descriptor.ascent, Some(800.0));
        assert!(descriptor.is_symbolic());
        assert!(!descriptor.is_nonsymbolic());
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

    #[test]
    fn parses_cid_system_info() {
        let document = Document::new();
        let font = cid_font(vec![(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Registry", Value::String("Adobe".into()))
                    .insert("Ordering", Value::String("Japan1".into()))
                    .insert("Supplement", Value::Integer(7)),
            ),
        )]);

        let font = document.cid_font_type0(&font).unwrap().unwrap();
        let info = font.system_info();
        assert_eq!(info.registry, "Adobe");
        assert_eq!(info.ordering, "Japan1");
        assert_eq!(info.supplement, 7);
    }

    #[test]
    fn cid_keyed_cff_program_uses_raw_backend_with_cid_semantics() {
        let document = Document::new();
        let font = cid_font_with_descriptor(
            Vec::new(),
            vec![(
                "FontFile3",
                font_file3("CIDFontType0C", fixtures::cid_keyed_cff()),
            )],
        );
        let font = document.cid_font_type0(&font).unwrap().unwrap();

        let backend = font.try_cff_outline_parser().unwrap().unwrap();
        assert!(backend.is_cid_keyed());
        assert_eq!(backend.glyph_count(), 2);
        assert_eq!(backend.glyph_for_cid(7), Some(1));
        assert_eq!(backend.glyph_for_cid(1), None);

        // The argument is a CID: CID 7 maps to GID 1 and uses Font DICT 1.
        let glyph = backend.parse_glyph(7).unwrap();
        assert_eq!(glyph.width, 500.0);
        assert_eq!(glyph.segments.len(), 3);
        // CID 1 exists as a GID but not in the charset.
        assert!(backend.parse_glyph(1).is_err());

        // The infallible accessor builds the same backend.
        let backend = font.cff_outline_parser().unwrap();
        assert!(backend.is_cid_keyed());
        assert_eq!(backend.parse_glyph(7).unwrap().width, 500.0);
    }

    #[test]
    fn opentype_program_keeps_the_ttf_parser_backend() {
        let document = Document::new();
        let font = cid_font_with_descriptor(
            Vec::new(),
            vec![(
                "FontFile3",
                font_file3(
                    "OpenType",
                    fixtures::opentype_with_cff(&fixtures::name_keyed_cff()),
                ),
            )],
        );
        let font = document.cid_font_type0(&font).unwrap().unwrap();

        let backend = font.try_cff_outline_parser().unwrap().unwrap();
        assert!(!backend.is_cid_keyed());
        assert_eq!(backend.glyph_count(), 2);
        // The ttf-parser backend takes a GID and reads widths from hmtx.
        let glyph = backend.parse_glyph(1).unwrap();
        assert_eq!(glyph.width, 600.0);
        assert_eq!(glyph.segments.len(), 3);
    }

    #[test]
    fn reports_deferred_and_eager_errors_for_a_malformed_cff_program() {
        let document = Document::new();
        let font = cid_font_with_descriptor(
            Vec::new(),
            vec![("FontFile3", font_file3("CIDFontType0C", b"CFF ".to_vec()))],
        );
        let font = document.cid_font_type0(&font).unwrap().unwrap();

        assert!(font.try_cff_outline_parser().is_err());
        let backend = font.cff_outline_parser().unwrap();
        assert_eq!(backend.glyph_count(), 0);
        let error = backend.parse_glyph(0).unwrap_err();
        assert!(error.to_string().contains("CFF"), "{error}");
    }

    #[test]
    fn type1c_programs_select_the_raw_backend() {
        let program = EmbeddedFontProgram {
            source: FontProgramSource::FontFile3,
            subtype: Some("Type1C".into()),
            data: fixtures::name_keyed_cff(),
        };
        let backend = outline_backend(&program).unwrap();
        assert!(!backend.is_cid_keyed());
        assert_eq!(backend.glyph_count(), 2);
        assert_eq!(backend.parse_glyph(1).unwrap().width, 600.0);

        let unsupported = EmbeddedFontProgram {
            subtype: Some("Type1".into()),
            ..program.clone()
        };
        assert!(outline_backend(&unsupported).is_err());

        let missing = EmbeddedFontProgram {
            subtype: None,
            ..program
        };
        assert!(outline_backend(&missing).is_err());
    }
}
