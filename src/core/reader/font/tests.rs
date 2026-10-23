use std::collections::BTreeMap;

use crate::core::types::{Array, Dict, Document, StreamData, Value};

use super::{PredefinedEncoding, code_for_char, parse_descriptor};

fn descriptor(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
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
