use std::collections::BTreeMap;

use crate::core::types::{Array, Dict, Document, StreamData, Value};

use super::cff::{fixtures, name_keyed_fixtures};
use super::{
    CffPathSegment, FontError, FontProgramSource, PredefinedEncoding, SfntFlavor, code_for_char,
    parse_descriptor, strip_subset_tag,
};

fn descriptor(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

/// Build an SFNT table directory; table payloads start zeroed.
fn sfnt_with_tables(signature: [u8; 4], records: &[([u8; 4], u32, u32)]) -> Vec<u8> {
    let directory_end = 12 + records.len() * 16;
    let data_length = records
        .iter()
        .map(|(_, offset, length)| (*offset as usize) + (*length as usize))
        .max()
        .unwrap_or(directory_end)
        .max(directory_end);
    let mut bytes = vec![0; data_length];
    bytes[..4].copy_from_slice(&signature);
    bytes[4..6].copy_from_slice(&(records.len() as u16).to_be_bytes());
    for (index, (tag, offset, length)) in records.iter().enumerate() {
        let start = 12 + index * 16;
        bytes[start..start + 4].copy_from_slice(tag);
        bytes[start + 8..start + 12].copy_from_slice(&offset.to_be_bytes());
        bytes[start + 12..start + 16].copy_from_slice(&length.to_be_bytes());
    }
    bytes
}

/// A format 4 cmap that maps U+0041 to glyph 2 of a three-glyph font.
fn format4_cmap() -> Vec<u8> {
    let mut cmap = vec![0; 44];
    cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
    cmap[4..6].copy_from_slice(&3u16.to_be_bytes());
    cmap[6..8].copy_from_slice(&1u16.to_be_bytes());
    cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
    let subtable = &mut cmap[12..];
    subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
    subtable[2..4].copy_from_slice(&32u16.to_be_bytes());
    subtable[6..8].copy_from_slice(&4u16.to_be_bytes());
    subtable[8..10].copy_from_slice(&4u16.to_be_bytes());
    subtable[10..12].copy_from_slice(&1u16.to_be_bytes());
    subtable[14..16].copy_from_slice(&0x41u16.to_be_bytes());
    subtable[16..18].copy_from_slice(&u16::MAX.to_be_bytes());
    subtable[20..22].copy_from_slice(&0x41u16.to_be_bytes());
    subtable[22..24].copy_from_slice(&u16::MAX.to_be_bytes());
    subtable[24..26].copy_from_slice(&(-63i16).to_be_bytes());
    subtable[26..28].copy_from_slice(&1i16.to_be_bytes());
    cmap
}

/// A three-glyph TrueType-flavored SFNT whose cmap maps U+0041 to glyph 2.
fn truetype_sfnt() -> Vec<u8> {
    let mut bytes = sfnt_with_tables(
        [0, 1, 0, 0],
        &[
            (*b"head", 124, 54),
            (*b"maxp", 180, 32),
            (*b"hhea", 212, 36),
            (*b"hmtx", 248, 10),
            (*b"cmap", 260, 44),
            (*b"loca", 304, 8),
            (*b"glyf", 312, 34),
        ],
    );
    bytes[124 + 12..124 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
    bytes[124 + 18..124 + 20].copy_from_slice(&2048u16.to_be_bytes());
    bytes[180..184].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[184..186].copy_from_slice(&3u16.to_be_bytes());
    bytes[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[246..248].copy_from_slice(&2u16.to_be_bytes());
    bytes[248..250].copy_from_slice(&500u16.to_be_bytes());
    bytes[252..254].copy_from_slice(&600u16.to_be_bytes());
    bytes[260..304].copy_from_slice(&format4_cmap());
    // Short loca offsets 0 -> 9 -> 9 -> 17 into a 34-byte glyf table.
    for (index, offset) in [0u16, 9, 9, 17].iter().enumerate() {
        bytes[304 + index * 2..306 + index * 2].copy_from_slice(&offset.to_be_bytes());
    }
    bytes[312..314].copy_from_slice(&1i16.to_be_bytes());
    bytes[314..322].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
    bytes[330..332].copy_from_slice(&(-1i16).to_be_bytes());
    bytes[332..340].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
    bytes
}

fn font_with_program(
    font_subtype: &str,
    base_font: &str,
    file_key: &str,
    stream_subtype: Option<&str>,
    data: Vec<u8>,
) -> Value {
    let mut stream_dict = Dict::new();
    if let Some(stream_subtype) = stream_subtype {
        stream_dict = stream_dict.insert("Subtype", Value::Name(stream_subtype.into()));
    }
    let descriptor = descriptor(vec![(
        file_key,
        Value::Stream(stream_dict, StreamData::Owned(data)),
    )]);
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name(font_subtype.into()))
            .insert("BaseFont", Value::Name(base_font.into()))
            .insert("FontDescriptor", Value::Dict(descriptor)),
    )
}

fn truetype_font_with_program(
    file_key: &str,
    stream_subtype: Option<&str>,
    data: Vec<u8>,
) -> Value {
    font_with_program("TrueType", "Arial", file_key, stream_subtype, data)
}

fn type1_font_with_program(file_key: &str, stream_subtype: Option<&str>, data: Vec<u8>) -> Value {
    font_with_program("Type1", "EmbeddedSans", file_key, stream_subtype, data)
}

fn simple_font(subtype: &str, base_font: &str, encoding: Option<Value>) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name(subtype.into()))
        .insert("BaseFont", Value::Name(base_font.into()));
    if let Some(encoding) = encoding {
        dict = dict.insert("Encoding", encoding);
    }
    Value::Dict(dict)
}

#[test]
fn code_for_char_maps_predefined_encodings() {
    let none = BTreeMap::new();
    assert_eq!(
        code_for_char(PredefinedEncoding::WinAnsi, &none, 'é'),
        Some(0xE9)
    );
    assert_eq!(
        code_for_char(PredefinedEncoding::WinAnsi, &none, '€'),
        Some(0x80)
    );
    assert_eq!(
        code_for_char(PredefinedEncoding::MacRoman, &none, 'é'),
        Some(0x8E)
    );
    assert_eq!(
        code_for_char(PredefinedEncoding::Standard, &none, 'A'),
        Some(0x41)
    );
    assert_eq!(
        code_for_char(PredefinedEncoding::Symbol, &none, 'α'),
        Some(0x61)
    );
    assert_eq!(
        code_for_char(PredefinedEncoding::ZapfDingbats, &none, '✁'),
        Some(0x21)
    );
    assert_eq!(
        code_for_char(PredefinedEncoding::WinAnsi, &none, '中'),
        None
    );
}

#[test]
fn code_for_char_prefers_differences_and_hides_overridden_codes() {
    let mut differences = BTreeMap::new();
    differences.insert(65, "eacute".to_string());

    assert_eq!(
        code_for_char(PredefinedEncoding::WinAnsi, &differences, 'é'),
        Some(65)
    );
    // 65 已被 /Differences 改成 eacute，基编码的 'A' 不再可用。
    assert_eq!(
        code_for_char(PredefinedEncoding::WinAnsi, &differences, 'A'),
        None
    );
    assert_eq!(
        code_for_char(PredefinedEncoding::WinAnsi, &differences, 'B'),
        Some(66)
    );
}

#[test]
fn descriptor_flags_follow_pdf_bit_positions() {
    let document = Document::new();
    let bits = 1 | 2 | 4 | 32 | (1 << 16) | (1 << 18);
    let parsed = parse_descriptor(
        &document,
        &descriptor(vec![("Flags", Value::Integer(bits))]),
    )
    .unwrap();
    assert!(parsed.is_fixed_pitch());
    assert!(parsed.is_serif());
    assert!(parsed.is_symbolic());
    assert!(parsed.is_nonsymbolic());
    assert!(parsed.is_all_cap());
    assert!(parsed.is_force_bold());
    assert!(!parsed.is_script());
    assert!(!parsed.is_italic());
    assert!(!parsed.is_small_cap());
}

#[test]
fn descriptor_parses_hex_text_and_cid_set_bits() {
    let document = Document::new();
    let parsed = parse_descriptor(
        &document,
        &descriptor(vec![
            (
                "FontFamily",
                Value::HexString("4578616D706C652046616D696C79".into()),
            ),
            ("CharSet", Value::HexString("2F412F42".into())),
            (
                "CIDSet",
                Value::Stream(
                    Dict::new().insert("Filter", Value::Name("ASCIIHexDecode".into())),
                    StreamData::Owned(b"AF01>".to_vec()),
                ),
            ),
        ]),
    )
    .unwrap();
    assert_eq!(parsed.font_family.as_deref(), Some("Example Family"));
    assert_eq!(parsed.char_set.as_deref(), Some("/A/B"));
    assert_eq!(parsed.cid_is_present(0), Some(true));
    assert_eq!(parsed.cid_is_present(1), Some(false));
    assert_eq!(parsed.cid_is_present(2), Some(true));
    assert_eq!(parsed.cid_is_present(15), Some(true));
    assert_eq!(parsed.cid_is_present(16), None);
}

#[test]
fn descriptor_rejects_non_stream_cid_set() {
    let document = Document::new();
    let invalid = descriptor(vec![("CIDSet", Value::Name("Invalid".into()))]);
    assert!(parse_descriptor(&document, &invalid).is_err());
}

#[test]
fn descriptor_parses_style_lang_and_fd_overrides() {
    let document = Document::new();
    let parsed = parse_descriptor(
        &document,
        &descriptor(vec![
            (
                "Style",
                Value::Dict(Dict::new().insert(
                    "Panose",
                    Value::HexString("020B05020300000000000000".into()),
                )),
            ),
            ("Lang", Value::Name("zh-CN".into())),
            (
                "FD",
                Value::Dict(Dict::new().insert(
                    "Proportional",
                    Value::Dict(descriptor(vec![
                        ("Ascent", Value::Float(750.0)),
                        ("Flags", Value::Integer(32)),
                    ])),
                )),
            ),
        ]),
    )
    .unwrap();
    assert_eq!(
        parsed.style_panose,
        Some([
            0x02, 0x0B, 0x05, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ])
    );
    assert_eq!(parsed.lang.as_deref(), Some("zh-CN"));
    let overrides = parsed.fd.as_ref().expect("/FD");
    let proportional = overrides.get("Proportional").expect("Proportional");
    assert_eq!(proportional.get("Ascent"), Some(&Value::Float(750.0)));
    assert_eq!(proportional.get("Flags"), Some(&Value::Integer(32)));
}

#[test]
fn descriptor_accepts_literal_string_panose() {
    let document = Document::new();
    let parsed = parse_descriptor(
        &document,
        &descriptor(vec![(
            "Style",
            Value::Dict(Dict::new().insert("Panose", Value::String("0123456789ab".into()))),
        )]),
    )
    .unwrap();
    assert_eq!(parsed.style_panose, Some(*b"0123456789ab"));
}

#[test]
fn descriptor_rejects_invalid_style_and_fd_entries() {
    let document = Document::new();
    let invalid = [
        descriptor(vec![("Style", Value::Integer(1))]),
        descriptor(vec![(
            "Style",
            Value::Dict(Dict::new().insert("Panose", Value::HexString("020B05".into()))),
        )]),
        descriptor(vec![(
            "Style",
            Value::Dict(Dict::new().insert("Panose", Value::Integer(1))),
        )]),
        descriptor(vec![("FD", Value::Name("Invalid".into()))]),
        descriptor(vec![(
            "FD",
            Value::Dict(Dict::new().insert("Proportional", Value::Integer(1))),
        )]),
    ];
    for dict in invalid {
        assert!(
            parse_descriptor(&document, &dict).is_err(),
            "应拒绝无效的 Style/FD 条目: {dict:?}"
        );
    }
}

#[test]
fn simple_font_encoding_accepts_truetype_encodings() {
    let differences = Array::new()
        .push(Value::Integer(65))
        .push(Value::Name("copyright".into()));
    let encoding = Value::Dict(
        Dict::new()
            .insert("BaseEncoding", Value::Name("WinAnsiEncoding".into()))
            .insert("Differences", Value::Array(differences)),
    );
    let font = simple_font("TrueType", "Arial", Some(encoding));

    let document = Document::new();
    let encoding = document.simple_font_encoding(&font).unwrap().unwrap();
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::WinAnsi);
    assert_eq!(encoding.unicode_sequence_for(65).as_deref(), Some("©"));
    assert_eq!(encoding.unicode_sequence_for(0x93).as_deref(), Some("“"));
    assert_eq!(encoding.unicode_sequence_for(0x94).as_deref(), Some("”"));
}

#[test]
fn simple_font_encoding_accepts_mmtype1_fonts() {
    let font = simple_font(
        "MMType1",
        "Helvetica",
        Some(Value::Name("MacRomanEncoding".into())),
    );

    let document = Document::new();
    let encoding = document.simple_font_encoding(&font).unwrap().unwrap();
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::MacRoman);
}

#[test]
fn simple_font_encoding_uses_base_font_default_when_encoding_is_absent() {
    let document = Document::new();

    let font = simple_font("TrueType", "Symbol", None);
    let encoding = document.simple_font_encoding(&font).unwrap().unwrap();
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::Symbol);

    let font = simple_font("TrueType", "Arial", None);
    let encoding = document.simple_font_encoding(&font).unwrap().unwrap();
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::Standard);
}

#[test]
fn simple_font_encoding_rejects_composite_fonts() {
    let document = Document::new();
    let font = simple_font("Type0", "Composite", None);
    assert!(document.simple_font_encoding(&font).unwrap().is_none());
    assert!(document.simple_font_encoding(&Value::Integer(1)).is_err());
}

#[test]
fn type1_font_encoding_still_rejects_truetype_fonts() {
    let document = Document::new();
    let font = simple_font(
        "TrueType",
        "Arial",
        Some(Value::Name("WinAnsiEncoding".into())),
    );
    assert!(document.type1_font_encoding(&font).unwrap().is_none());
    assert!(document.simple_font_encoding(&font).unwrap().is_some());
}

#[test]
fn strip_subset_tag_removes_only_six_letter_prefixes() {
    assert_eq!(strip_subset_tag("ABCDEF+Helvetica"), "Helvetica");
    assert_eq!(strip_subset_tag("ABCDEF+Symbol"), "Symbol");
    // 标签必须恰好六个大写 ASCII 字母。
    assert_eq!(strip_subset_tag("ABCDE+Helvetica"), "ABCDE+Helvetica");
    assert_eq!(strip_subset_tag("abcdeF+Helvetica"), "abcdeF+Helvetica");
    assert_eq!(strip_subset_tag("ABCDEF1+Helvetica"), "ABCDEF1+Helvetica");
    assert_eq!(strip_subset_tag("ABCDEFHelvetica"), "ABCDEFHelvetica");
    assert_eq!(strip_subset_tag("ABCDEF+"), "ABCDEF+");
    assert_eq!(strip_subset_tag("Helvetica"), "Helvetica");
    assert_eq!(strip_subset_tag(""), "");
}

#[test]
fn subset_prefix_keeps_default_encoding_and_standard14_metrics() {
    let document = Document::new();

    let symbolic = simple_font("TrueType", "ABCDEF+Symbol", None);
    let encoding = document.simple_font_encoding(&symbolic).unwrap().unwrap();
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::Symbol);

    // 无 /Widths、无描述符时仍应按标准 14 回退，而不是拒绝字体。
    let subset = simple_font("Type1", "ABCDEF+Helvetica", None);
    let subset_font = document.type1_font(&subset).unwrap().unwrap();
    assert_eq!(subset_font.base_font, "ABCDEF+Helvetica");
    let plain = simple_font("Type1", "Helvetica", None);
    let plain_font = document.type1_font(&plain).unwrap().unwrap();
    assert_eq!(subset_font.width_for(0x41), plain_font.width_for(0x41));
    assert!(subset_font.width_for(0x41).is_some());
}

#[test]
fn truetype_font_accepts_opentype_fontfile3_program() {
    let font = truetype_font_with_program("FontFile3", Some("OpenType"), truetype_sfnt());
    let document = Document::new();

    let program = document.truetype_font_program(&font).unwrap().unwrap();
    assert_eq!(program.source, FontProgramSource::FontFile3);
    assert_eq!(program.subtype.as_deref(), Some("OpenType"));

    let parsed = document.truetype_font(&font).unwrap().unwrap();
    assert_eq!(parsed.flavor(), SfntFlavor::TrueType);
    assert_eq!(parsed.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    assert_eq!(parsed.builtin_encoding().unwrap()[0x41], Some('A'));

    // Without /Encoding, text extraction uses the program's built-in encoding.
    let resources = Dict::new().insert("Font", Value::Dict(Dict::new().insert("F1", font)));
    assert_eq!(
        document.extract_text(b"BT /F1 12 Tf (A) Tj ET", &resources),
        "A"
    );
}

#[test]
fn truetype_font_still_accepts_fontfile2_program() {
    let font = truetype_font_with_program("FontFile2", None, truetype_sfnt());
    let document = Document::new();

    let program = document.truetype_font_program(&font).unwrap().unwrap();
    assert_eq!(program.source, FontProgramSource::FontFile2);
    assert_eq!(program.subtype, None);
    assert!(document.truetype_font(&font).unwrap().is_some());
}

#[test]
fn truetype_font_rejects_fontfile3_type1c_program() {
    let font = truetype_font_with_program("FontFile3", Some("Type1C"), b"CFF ".to_vec());
    let document = Document::new();
    assert!(matches!(
        document.truetype_font(&font),
        Err(FontError::Invalid(_))
    ));
    assert!(matches!(
        document.truetype_font_program(&font),
        Err(FontError::Invalid(_))
    ));
}

#[test]
fn truetype_font_rejects_fontfile3_program_without_subtype() {
    let font = truetype_font_with_program("FontFile3", None, truetype_sfnt());
    let document = Document::new();
    assert!(matches!(
        document.truetype_font(&font),
        Err(FontError::Invalid(_))
    ));
}

#[test]
fn type1c_program_resolves_names_widths_and_outlines() {
    let font = type1_font_with_program(
        "FontFile3",
        Some("Type1C"),
        name_keyed_fixtures::name_keyed_cff_custom_charset(0),
    );
    let document = Document::new();
    let program = document.type1c_program(&font).unwrap().unwrap();

    assert_eq!(program.glyph_count(), 4);
    assert_eq!(program.glyph_name(0), Some(".notdef"));
    assert_eq!(program.glyph_name(1), Some("Alpha"));
    assert_eq!(program.glyph_for_name("Gamma"), Some(3));
    assert_eq!(program.glyph_for_name("Delta"), None);
    assert_eq!(program.units_per_em(), 2000);
    assert_eq!(program.glyph_width(1), Some(600.0));

    let glyph = program.outline(1).unwrap();
    assert_eq!(glyph.width, 600.0);
    assert_eq!(
        glyph.segments,
        vec![
            CffPathSegment::MoveTo { x: 100.0, y: 0.0 },
            CffPathSegment::LineTo { x: 300.0, y: 0.0 },
            CffPathSegment::ClosePath,
        ]
    );
}

#[test]
fn type1c_program_resolves_default_and_predefined_charsets() {
    let document = Document::new();

    let default = type1_font_with_program("FontFile3", Some("Type1C"), fixtures::name_keyed_cff());
    let program = document.type1c_program(&default).unwrap().unwrap();
    assert_eq!(program.glyph_name(0), Some(".notdef"));
    assert_eq!(program.glyph_name(1), Some("space"));
    assert_eq!(program.glyph_for_name("space"), Some(1));

    let expert = type1_font_with_program(
        "FontFile3",
        Some("Type1C"),
        name_keyed_fixtures::name_keyed_cff_predefined_charset(1, 3),
    );
    let program = document.type1c_program(&expert).unwrap().unwrap();
    assert_eq!(program.glyph_name(2), Some("exclamsmall"));
}

#[test]
fn type1c_program_rejects_cid_keyed_wrong_subtype_and_malformed_programs() {
    let document = Document::new();

    let cid_keyed = type1_font_with_program("FontFile3", Some("Type1C"), fixtures::cid_keyed_cff());
    let error = document.type1c_program(&cid_keyed).unwrap_err();
    assert!(matches!(
        &error,
        FontError::Invalid(message) if message.contains("CID-keyed")
    ));

    let wrong_subtype = type1_font_with_program(
        "FontFile3",
        Some("CIDFontType0C"),
        fixtures::cid_keyed_cff(),
    );
    let error = document.type1c_program(&wrong_subtype).unwrap_err();
    assert!(matches!(
        &error,
        FontError::Invalid(message) if message.contains("requires /Subtype /Type1C")
    ));

    let malformed = type1_font_with_program("FontFile3", Some("Type1C"), vec![1, 0, 4, 9]);
    assert!(matches!(
        document.type1c_program(&malformed),
        Err(FontError::Invalid(_))
    ));

    // A Type 1 font that embeds a /FontFile program is handled by
    // `type1_program`, not by the CFF parser.
    let font_file = type1_font_with_program("FontFile", None, b"not-cff".to_vec());
    assert!(document.type1c_program(&font_file).unwrap().is_none());

    let no_program = simple_font("Type1", "Helvetica", None);
    assert!(document.type1c_program(&no_program).unwrap().is_none());

    let truetype = simple_font("TrueType", "Arial", None);
    assert!(document.type1c_program(&truetype).unwrap().is_none());
}
