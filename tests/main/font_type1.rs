use super::*;

#[test]
fn type1_font_resolves_indirect_widths_and_descriptor_metrics() {
    let doc = Document::new();
    doc.set_object(Object::new(
        10,
        0,
        Value::Array(Array(vec![Value::Integer(500), Value::Float(611.5)])),
    ));
    doc.set_object(Object::new(12, 0, Value::Integer(400)));
    doc.set_object(Object::new(
        11,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("ExampleSans".into()))
                .insert("FontFamily", Value::String("Example Sans".into()))
                .insert("FontStretch", Value::Name("Normal".into()))
                .insert("FontWeight", Value::Ref(12, 0))
                .insert("Flags", Value::Integer(32))
                .insert(
                    "FontBBox",
                    Value::Array(Array(vec![
                        Value::Integer(-20),
                        Value::Integer(-200),
                        Value::Integer(1000),
                        Value::Integer(900),
                    ])),
                )
                .insert("ItalicAngle", Value::Integer(0))
                .insert("Ascent", Value::Integer(750))
                .insert("Descent", Value::Integer(-200))
                .insert("Leading", Value::Integer(200))
                .insert("CapHeight", Value::Integer(700))
                .insert("XHeight", Value::Integer(500))
                .insert("StemV", Value::Integer(80))
                .insert("StemH", Value::Integer(60))
                .insert("AvgWidth", Value::Integer(600))
                .insert("MaxWidth", Value::Integer(1000))
                .insert("MissingWidth", Value::Integer(400))
                .insert("CharSet", Value::String("/A/B".into())),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(66))
            .insert("Widths", Value::Ref(10, 0))
            .insert("FontDescriptor", Value::Ref(11, 0)),
    );

    let font = doc.type1_font(&font).unwrap().unwrap();
    assert_eq!(font.base_font, "ExampleSans");
    assert_eq!(font.width_for(65), Some(500.0));
    assert_eq!(font.width_for(66), Some(611.5));
    assert_eq!(font.width_for(64), Some(400.0));
    assert_eq!(font.width_for(67), Some(400.0));
    let descriptor = font.descriptor.unwrap();
    assert_eq!(descriptor.font_name.as_deref(), Some("ExampleSans"));
    assert_eq!(descriptor.font_family.as_deref(), Some("Example Sans"));
    assert_eq!(descriptor.font_stretch.as_deref(), Some("Normal"));
    assert_eq!(descriptor.font_weight, Some(400.0));
    assert_eq!(descriptor.flags, Some(32));
    assert!(descriptor.is_nonsymbolic());
    assert!(!descriptor.is_serif());
    assert_eq!(descriptor.font_bbox, Some([-20.0, -200.0, 1000.0, 900.0]));
    assert_eq!(descriptor.italic_angle, Some(0.0));
    assert_eq!(descriptor.ascent, Some(750.0));
    assert_eq!(descriptor.descent, Some(-200.0));
    assert_eq!(descriptor.leading, Some(200.0));
    assert_eq!(descriptor.cap_height, Some(700.0));
    assert_eq!(descriptor.x_height, Some(500.0));
    assert_eq!(descriptor.stem_v, Some(80.0));
    assert_eq!(descriptor.stem_h, Some(60.0));
    assert_eq!(descriptor.avg_width, Some(600.0));
    assert_eq!(descriptor.max_width, Some(1000.0));
    assert_eq!(descriptor.missing_width, Some(400.0));
    assert_eq!(descriptor.char_set.as_deref(), Some("/A/B"));
    assert_eq!(descriptor.cid_is_present(0), None);

    let incomplete = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("CustomFont".into())),
    );
    assert!(doc.type1_font(&incomplete).is_err());

    let standard_14 = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert_eq!(
        doc.type1_font(&standard_14).unwrap().unwrap().width_for(65),
        Some(667.0)
    );

    let unsupported = Value::Dict(Dict::new().insert("Subtype", Value::Name("TrueType".into())));
    assert!(doc.type1_font(&unsupported).unwrap().is_none());
}

#[test]
fn type1_font_program_resolves_and_decodes_descriptor_streams() {
    let doc = Document::new();
    doc.set_object(Object::new(
        40,
        0,
        Value::Stream(
            Dict::new().insert("Filter", Value::Name("ASCIIHexDecode".into())),
            StreamData::Owned(b"414243>".to_vec()),
        ),
    ));
    doc.set_object(Object::new(
        41,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontFile", Value::Ref(40, 0)),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("FontDescriptor", Value::Ref(41, 0)),
    );

    let program = doc.type1_font_program(&font).unwrap().unwrap();
    assert_eq!(program.source, FontProgramSource::FontFile);
    assert_eq!(program.subtype, None);
    assert_eq!(program.data, b"ABC");

    let type1c_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile3",
                            Value::Stream(
                                Dict::new().insert("Subtype", Value::Name("Type1C".into())),
                                StreamData::Owned(b"CFF".to_vec()),
                            ),
                        ),
                ),
            ),
    );
    let type1c_program = doc.type1_font_program(&type1c_font).unwrap().unwrap();
    assert_eq!(type1c_program.source, FontProgramSource::FontFile3);
    assert_eq!(type1c_program.subtype.as_deref(), Some("Type1C"));
    assert_eq!(type1c_program.data, b"CFF");

    let no_program = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(doc.type1_font_program(&no_program).unwrap().is_none());

    let unsupported_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into())),
    );
    assert!(doc.type1_font_program(&unsupported_font).unwrap().is_none());

    let invalid_file2 = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile2",
                            Value::Stream(Dict::new(), StreamData::Owned(b"font".to_vec())),
                        ),
                ),
            ),
    );
    assert!(doc.type1_font_program(&invalid_file2).is_err());
}

#[test]
fn type1_font_program_reports_malformed_descriptor_entries() {
    let doc = Document::new();
    let make_font = |descriptor| {
        Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name("ExampleSans".into()))
                .insert("FontDescriptor", descriptor),
        )
    };
    let invalid_stream = Value::Stream(
        Dict::new().insert("Filter", Value::Name("FlateDecode".into())),
        StreamData::Owned(b"not deflate data".to_vec()),
    );
    let invalid_descriptor = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("FontDescriptor".into()))
            .insert("FontFile", invalid_stream),
    );
    assert!(
        doc.type1_font_program(&make_font(invalid_descriptor))
            .is_err()
    );

    let multiple_entries = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("FontDescriptor".into()))
            .insert(
                "FontFile",
                Value::Stream(Dict::new(), StreamData::Owned(Vec::new())),
            )
            .insert(
                "FontFile2",
                Value::Stream(Dict::new(), StreamData::Owned(Vec::new())),
            ),
    );
    assert!(
        doc.type1_font_program(&make_font(multiple_entries))
            .is_err()
    );

    let missing_subtype = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("FontDescriptor".into()))
            .insert(
                "FontFile3",
                Value::Stream(Dict::new(), StreamData::Owned(b"CFF".to_vec())),
            ),
    );
    assert!(doc.type1_font_program(&make_font(missing_subtype)).is_err());
}

#[test]
fn type1_program_parses_embedded_font_file() {
    let doc = Document::new();
    doc.set_object(Object::new(
        60,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(sample_type1_program())),
    ));
    doc.set_object(Object::new(
        61,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontFile", Value::Ref(60, 0)),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("EmbeddedSans".into()))
            .insert("FontDescriptor", Value::Ref(61, 0)),
    );

    let program = doc.type1_program(&font).unwrap().unwrap();
    assert_eq!(program.header.font_name.as_deref(), Some("EmbeddedSans"));
    assert_eq!(program.header.family_name.as_deref(), Some("Embedded Sans"));
    assert_eq!(program.header.font_bbox, Some([0.0, -200.0, 1000.0, 900.0]));
    assert_eq!(
        program.header.encoding,
        Some(Type1BuiltinEncoding::Predefined(
            "StandardEncoding".to_string()
        ))
    );
    assert_eq!(program.len_iv, 4);
    assert_eq!(
        program.charstrings.get(".notdef").map(Vec::as_slice),
        Some(b"\x8b\xf8\x88\x0d\x0e".as_slice())
    );
    assert_eq!(
        program.charstrings.get("A").map(Vec::as_slice),
        Some(
            b"\x9f\xf8\xec\x0d\xef\x8b\x15\x8b\xf9\x50\x05\x27\x8b\x05\x8b\xfd\x50\x05\x09\x0e"
                .as_slice()
        )
    );
    assert_eq!(
        program.subrs.get(&0).map(Vec::as_slice),
        Some(b"\x8b\x0b".as_slice())
    );

    let glyph = program.glyph("A").unwrap().unwrap();
    assert_eq!(glyph.width, 600.0);
    assert_eq!(glyph.side_bearing, [20.0, 0.0]);
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 120.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 120.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 0.0 },
            Type1PathSegment::ClosePath,
        ]
    );
    assert_eq!(glyph.seac, None);
    assert!(program.glyph("missing").unwrap().is_none());

    let no_program = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(doc.type1_program(&no_program).unwrap().is_none());

    let type1c_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("EmbeddedSans".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile3",
                            Value::Stream(
                                Dict::new().insert("Subtype", Value::Name("Type1C".into())),
                                StreamData::Owned(b"CFF".to_vec()),
                            ),
                        ),
                ),
            ),
    );
    assert!(doc.type1_program(&type1c_font).is_err());
}
