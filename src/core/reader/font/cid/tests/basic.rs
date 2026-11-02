//! Tests for CID-to-GID mapping, `/W` widths, descriptor metrics, and
//! table rejection.

use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

use super::{cid_font, cid_font_with_descriptor, cid_system_info};

#[test]
fn parses_cid_to_gid_map_and_both_width_forms() {
    let document = Document::new();
    let font = cid_font(vec![
        (
            "CIDToGIDMap",
            Value::Stream(Dict::new(), StreamData::Owned(vec![0, 4, 0, 9, 0, 10])),
        ),
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
    ]);

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    assert_eq!(font.base_font(), "ExampleCIDFont");
    assert_eq!(font.default_width(), 950.0);
    assert_eq!(font.glyph_id_for_cid(0), Some(4));
    assert_eq!(font.glyph_id_for_cid(1), Some(9));
    assert_eq!(font.glyph_id_for_cid(2), Some(10));
    assert_eq!(font.glyph_id_for_cid(3), None);
    assert_eq!(font.width_for_cid(0), 950.0);
    assert_eq!(font.width_for_cid(1), 500.0);
    assert_eq!(font.width_for_cid(2), 510.5);
    assert_eq!(font.width_for_cid(5), 700.0);
    assert_eq!(font.width_for_cid(6), 700.0);
    assert!(font.true_type_font().is_none());
}

#[test]
fn defaults_to_identity_cid_mapping_and_width() {
    let document = Document::new();
    let font = document
        .cid_font_type2(&cid_font(Vec::new()))
        .unwrap()
        .unwrap();
    assert_eq!(font.glyph_id_for_cid(0), Some(0));
    assert_eq!(font.glyph_id_for_cid(u16::MAX), Some(u16::MAX));
    assert_eq!(font.width_for_cid(123), 1000.0);
}

#[test]
fn resolves_indirect_widths_and_cid_to_gid_streams() {
    let document = Document::new();
    document.set_object(Object::new(
        10,
        0,
        Value::Array(Array::new().push(Value::Integer(650))),
    ));
    document.set_object(Object::new(
        11,
        0,
        Value::Array(
            Array::new()
                .push(Value::Integer(12))
                .push(Value::Ref(10, 0)),
        ),
    ));
    document.set_object(Object::new(
        12,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(vec![0, 22, 0, 23])),
    ));
    let font = cid_font(vec![
        ("W", Value::Ref(11, 0)),
        ("CIDToGIDMap", Value::Ref(12, 0)),
    ]);

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    assert_eq!(font.width_for_cid(12), 650.0);
    assert_eq!(font.glyph_id_for_cid(0), Some(22));
    assert_eq!(font.glyph_id_for_cid(1), Some(23));
}

#[test]
fn overlapping_w_entries_use_the_last_value() {
    let document = Document::new();
    let font = cid_font(vec![(
        "W",
        Value::Array(
            Array::new()
                .push(Value::Integer(4))
                .push(Value::Array(
                    Array::new()
                        .push(Value::Integer(500))
                        .push(Value::Integer(501)),
                ))
                .push(Value::Integer(5))
                .push(Value::Integer(6))
                .push(Value::Integer(600)),
        ),
    )]);

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    assert_eq!(font.width_for_cid(4), 500.0);
    assert_eq!(
        font.width_for_cid(5),
        600.0,
        "后出现的 /W 条目应覆盖先前的 CID 5"
    );
    assert_eq!(font.width_for_cid(6), 600.0);
}

#[test]
fn rejects_invalid_cid_font_tables() {
    let document = Document::new();
    let odd_map = cid_font(vec![(
        "CIDToGIDMap",
        Value::Stream(Dict::new(), StreamData::Owned(vec![0])),
    )]);
    assert!(document.cid_font_type2(&odd_map).is_err());

    let oversized_map = cid_font(vec![(
        "CIDToGIDMap",
        Value::Stream(
            Dict::new(),
            StreamData::Owned(vec![0; (usize::from(u16::MAX) + 2) * 2]),
        ),
    )]);
    assert!(document.cid_font_type2(&oversized_map).is_err());

    let invalid_map = cid_font(vec![("CIDToGIDMap", Value::Name("Custom".into()))]);
    assert!(document.cid_font_type2(&invalid_map).is_err());

    let truncated_widths = cid_font(vec![(
        "W",
        Value::Array(Array::new().push(Value::Integer(4))),
    )]);
    assert!(document.cid_font_type2(&truncated_widths).is_err());

    let missing_info = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("CIDFontType2".into()))
            .insert("BaseFont", Value::Name("MissingInfo".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
            ),
    );
    assert!(document.cid_font_type2(&missing_info).is_err());

    let missing_descriptor = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("CIDFontType2".into()))
            .insert("BaseFont", Value::Name("MissingDescriptor".into()))
            .insert("CIDSystemInfo", cid_system_info()),
    );
    assert!(document.cid_font_type2(&missing_descriptor).is_err());
}

#[test]
fn exposes_descriptor_metrics_and_cid_set_bits() {
    let document = Document::new();
    let font = cid_font_with_descriptor(
        Vec::new(),
        vec![
            ("FontName", Value::Name("CIDFont".into())),
            ("FontFamily", Value::String("Example Family".into())),
            ("FontStretch", Value::Name("Condensed".into())),
            ("FontWeight", Value::Integer(700)),
            ("Flags", Value::Integer(32)),
            (
                "FontBBox",
                Value::Array(
                    Array::new()
                        .push(Value::Integer(-10))
                        .push(Value::Integer(-200))
                        .push(Value::Integer(1000))
                        .push(Value::Integer(900)),
                ),
            ),
            ("ItalicAngle", Value::Integer(0)),
            ("Ascent", Value::Integer(800)),
            ("Descent", Value::Integer(-200)),
            ("Leading", Value::Integer(100)),
            ("CapHeight", Value::Integer(700)),
            ("XHeight", Value::Integer(500)),
            ("StemV", Value::Integer(80)),
            ("StemH", Value::Integer(60)),
            ("AvgWidth", Value::Integer(600)),
            ("MaxWidth", Value::Integer(1000)),
            ("MissingWidth", Value::Integer(450)),
            ("CharSet", Value::String("/A/B".into())),
            (
                "CIDSet",
                Value::Stream(
                    Dict::new().insert("Filter", Value::Name("ASCIIHexDecode".into())),
                    StreamData::Owned(b"AF01>".to_vec()),
                ),
            ),
        ],
    );

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    let descriptor = font.descriptor();
    assert_eq!(descriptor.font_name.as_deref(), Some("CIDFont"));
    assert_eq!(descriptor.font_family.as_deref(), Some("Example Family"));
    assert_eq!(descriptor.font_stretch.as_deref(), Some("Condensed"));
    assert_eq!(descriptor.font_weight, Some(700.0));
    assert_eq!(descriptor.font_bbox, Some([-10.0, -200.0, 1000.0, 900.0]));
    assert_eq!(descriptor.italic_angle, Some(0.0));
    assert_eq!(descriptor.ascent, Some(800.0));
    assert_eq!(descriptor.descent, Some(-200.0));
    assert_eq!(descriptor.leading, Some(100.0));
    assert_eq!(descriptor.cap_height, Some(700.0));
    assert_eq!(descriptor.x_height, Some(500.0));
    assert_eq!(descriptor.stem_v, Some(80.0));
    assert_eq!(descriptor.stem_h, Some(60.0));
    assert_eq!(descriptor.avg_width, Some(600.0));
    assert_eq!(descriptor.max_width, Some(1000.0));
    assert_eq!(descriptor.missing_width, Some(450.0));
    assert_eq!(descriptor.char_set.as_deref(), Some("/A/B"));
    assert!(descriptor.is_nonsymbolic());
    assert!(!descriptor.is_serif());
    assert_eq!(descriptor.cid_is_present(0), Some(true));
    assert_eq!(descriptor.cid_is_present(1), Some(false));
    assert_eq!(descriptor.cid_is_present(2), Some(true));
    assert_eq!(descriptor.cid_is_present(15), Some(true));
    assert_eq!(descriptor.cid_is_present(16), None);
}

#[test]
fn rejects_non_stream_cid_set() {
    let document = Document::new();
    let font =
        cid_font_with_descriptor(Vec::new(), vec![("CIDSet", Value::Name("Invalid".into()))]);
    assert!(document.cid_font_type2(&font).is_err());
}

#[test]
fn ignores_other_font_subtypes() {
    let document = Document::new();
    let value = Value::Dict(Dict::new().insert("Subtype", Value::Name("TrueType".into())));
    assert!(document.cid_font_type2(&value).unwrap().is_none());
}
