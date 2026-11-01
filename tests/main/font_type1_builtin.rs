use super::*;

#[test]
fn type1_font_uses_builtin_courier_widths_when_no_override_exists() {
    let doc = Document::new();
    for font_name in [
        "Courier",
        "Courier-Bold",
        "Courier-Oblique",
        "Courier-BoldOblique",
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        let font = doc.type1_font(&font).unwrap().unwrap();
        assert_eq!(font.width_for(0), Some(600.0), "{font_name}");
        assert_eq!(font.width_for(65), Some(600.0), "{font_name}");
        assert_eq!(font.width_for(255), Some(600.0), "{font_name}");
    }

    for (font_name, expected_a, expected_fi) in [
        ("Helvetica", 667.0, 500.0),
        ("Helvetica-Bold", 722.0, 611.0),
        ("Helvetica-Oblique", 667.0, 500.0),
        ("Helvetica-BoldOblique", 722.0, 611.0),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        let font = doc.type1_font(&font).unwrap().unwrap();
        assert_eq!(font.width_for(32), Some(278.0), "{font_name} space");
        assert_eq!(font.width_for(65), Some(expected_a), "{font_name} A");
        assert_eq!(font.width_for(174), Some(expected_fi), "{font_name} fi");
        assert_eq!(font.width_for(0), None, "{font_name} undefined code");
    }

    for (font_name, expected_a, expected_fi) in [
        ("Times-Roman", 722.0, 556.0),
        ("Times-Bold", 722.0, 556.0),
        ("Times-Italic", 611.0, 500.0),
        ("Times-BoldItalic", 667.0, 556.0),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        let font = doc.type1_font(&font).unwrap().unwrap();
        assert_eq!(font.width_for(32), Some(250.0), "{font_name} space");
        assert_eq!(font.width_for(65), Some(expected_a), "{font_name} A");
        assert_eq!(font.width_for(174), Some(expected_fi), "{font_name} fi");
        assert_eq!(font.width_for(0), None, "{font_name} undefined code");
    }

    for (font_name, code, expected_width) in [
        ("Symbol", 0x20, 250.0),
        ("Symbol", 0x41, 722.0),
        ("Symbol", 0xA0, 750.0),
        ("ZapfDingbats", 0x20, 278.0),
        ("ZapfDingbats", 0x21, 974.0),
        ("ZapfDingbats", 0xFE, 918.0),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        assert_eq!(
            doc.type1_font(&font).unwrap().unwrap().width_for(code),
            Some(expected_width),
            "{font_name} code {code:#04x}"
        );
    }

    for font_name in ["Symbol", "ZapfDingbats"] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(font_name.into())),
        );
        assert_eq!(
            doc.type1_font(&font).unwrap().unwrap().width_for(0xFF),
            None,
            "{font_name} undefined code"
        );
    }

    let explicit_width = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Courier".into()))
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(65))
            .insert("Widths", Value::Array(Array(vec![Value::Integer(575)]))),
    );
    assert_eq!(
        doc.type1_font(&explicit_width)
            .unwrap()
            .unwrap()
            .width_for(65),
        Some(575.0)
    );

    let other_standard_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into())),
    );
    assert!(
        doc.type1_font(&other_standard_font)
            .unwrap()
            .unwrap()
            .width_for(65)
            .is_some()
    );
}

#[test]
fn type1_font_encoding_resolves_predefined_encodings_and_differences() {
    let doc = Document::new();
    doc.set_object(Object::new(
        10,
        0,
        Value::Array(Array(vec![
            Value::Integer(65),
            Value::Name("A.alt".into()),
            Value::Name("Aacute.alt".into()),
            Value::Name("dalethatafpatah".into()),
            Value::Name("ffi".into()),
            Value::Name("Aacute".into()),
            Value::Integer(255),
            Value::Name("lastcode".into()),
        ])),
    ));
    doc.set_object(Object::new(
        11,
        0,
        Value::Dict(
            Dict::new()
                .insert("BaseEncoding", Value::Name("WinAnsiEncoding".into()))
                .insert("Differences", Value::Ref(10, 0)),
        ),
    ));
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Ref(11, 0)),
    );

    let encoding = doc.type1_font_encoding(&font).unwrap().unwrap();
    assert_eq!(encoding.base_encoding(), PredefinedEncoding::WinAnsi);
    assert_eq!(
        encoding.mapping_for(65),
        EncodingMapping::GlyphName("A.alt")
    );
    assert_eq!(encoding.unicode_for(65), None);
    assert_eq!(
        encoding.unicode_sequence_for(67),
        Some("\u{05d3}\u{05b2}".into())
    );
    assert_eq!(encoding.unicode_for(67), None);
    assert_eq!(encoding.unicode_for(68), Some('\u{fb03}'));
    assert_eq!(encoding.unicode_sequence_for(68), Some("\u{fb03}".into()));
    assert_eq!(encoding.unicode_for(69), Some('\u{00c1}'));
    assert_eq!(
        encoding.mapping_for(66),
        EncodingMapping::GlyphName("Aacute.alt")
    );
    assert_eq!(
        encoding.mapping_for(64),
        EncodingMapping::Predefined {
            encoding: PredefinedEncoding::WinAnsi,
            code: 64,
        }
    );
    assert_eq!(encoding.unicode_for(65), None);
    assert_eq!(
        encoding.mapping_for(255),
        EncodingMapping::GlyphName("lastcode")
    );

    for (name, expected) in [
        ("StandardEncoding", PredefinedEncoding::Standard),
        ("WinAnsiEncoding", PredefinedEncoding::WinAnsi),
        ("MacRomanEncoding", PredefinedEncoding::MacRoman),
        ("SymbolEncoding", PredefinedEncoding::Symbol),
        ("ZapfDingbatsEncoding", PredefinedEncoding::ZapfDingbats),
    ] {
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name("ExampleSans".into()))
                .insert("Encoding", Value::Name(name.into())),
        );
        assert_eq!(
            doc.type1_font_encoding(&font)
                .unwrap()
                .unwrap()
                .base_encoding(),
            expected
        );
    }

    let symbol_encoding = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Symbol".into())),
    );
    let symbol = doc.type1_font_encoding(&symbol_encoding).unwrap().unwrap();
    assert_eq!(symbol.glyph_name_for(0x21).as_deref(), Some("exclam"));
    assert_eq!(symbol.glyph_name_for(0x41).as_deref(), Some("Alpha"));
    assert_eq!(symbol.glyph_name_for(0x61).as_deref(), Some("alpha"));
    assert_eq!(symbol.glyph_name_for(0xA0).as_deref(), Some("Euro"));
    assert_eq!(symbol.glyph_name_for(0xFF), None);
    assert_eq!(symbol.unicode_for(0x41), Some('\u{0391}'));
    assert_eq!(symbol.unicode_for(0x61), Some('\u{03b1}'));
    assert_eq!(symbol.unicode_for(0xA0), Some('\u{20ac}'));
    assert_eq!(symbol.unicode_for(0x44), Some('\u{2206}'));
    assert_eq!(symbol.unicode_for(0x57), Some('\u{2126}'));
    assert_eq!(symbol.unicode_for(0xA1), Some('\u{03d2}'));
    assert_eq!(symbol.unicode_for(0xFF), None);

    let dingbats_encoding = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ZapfDingbats".into())),
    );
    let dingbats = doc
        .type1_font_encoding(&dingbats_encoding)
        .unwrap()
        .unwrap();
    assert_eq!(dingbats.glyph_name_for(0x20).as_deref(), Some("space"));
    assert_eq!(dingbats.glyph_name_for(0x21).as_deref(), Some("a1"));
    assert_eq!(dingbats.glyph_name_for(0x23).as_deref(), Some("a202"));
    assert_eq!(dingbats.glyph_name_for(0x7F), None);
    let glyph_code = |name| {
        (0..=u8::MAX)
            .find(|code| dingbats.glyph_name_for(*code).as_deref() == Some(name))
            .unwrap()
    };
    for (glyph, expected) in [
        ("a1", '\u{2701}'),
        ("a4", '\u{260e}'),
        ("a104", '\u{2764}'),
        ("a109", '\u{2660}'),
        ("a120", '\u{2460}'),
        ("a161", '\u{2192}'),
        ("a191", '\u{27be}'),
    ] {
        assert_eq!(dingbats.unicode_for(glyph_code(glyph)), Some(expected));
    }
    assert_eq!(dingbats.unicode_for(0xFF), None);

    let win_ansi_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
    );
    let win_ansi = doc.type1_font_encoding(&win_ansi_font).unwrap().unwrap();
    assert_eq!(win_ansi.unicode_for(65), Some('A'));
    assert_eq!(win_ansi.unicode_for(128), Some('\u{20ac}'));
    assert_eq!(win_ansi.unicode_for(147), Some('\u{201c}'));
    assert_eq!(win_ansi.unicode_for(0), None);
    assert_eq!(win_ansi.unicode_for(127), None);
    assert_eq!(win_ansi.unicode_for(129), None);
    assert_eq!(win_ansi.unicode_for(255), Some('\u{00ff}'));

    let mac_roman_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Name("MacRomanEncoding".into())),
    );
    let mac_roman = doc.type1_font_encoding(&mac_roman_font).unwrap().unwrap();
    assert_eq!(mac_roman.unicode_for(65), Some('A'));
    assert_eq!(mac_roman.unicode_for(128), Some('\u{00c4}'));
    assert_eq!(mac_roman.unicode_for(202), Some('\u{00a0}'));
    assert_eq!(mac_roman.unicode_for(240), Some('\u{f8ff}'));
    assert_eq!(mac_roman.unicode_for(0), None);
    assert_eq!(mac_roman.unicode_for(127), None);
    for code in 0x80..=u8::MAX {
        assert!(mac_roman.unicode_for(code).is_some(), "code {code:#04x}");
    }

    let symbol_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Symbol".into())),
    );
    assert_eq!(
        doc.type1_font_encoding(&symbol_font)
            .unwrap()
            .unwrap()
            .base_encoding(),
        PredefinedEncoding::Symbol
    );

    let default_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into())),
    );
    let standard = doc.type1_font_encoding(&default_font).unwrap().unwrap();
    assert_eq!(standard.base_encoding(), PredefinedEncoding::Standard);
    assert_eq!(standard.glyph_name_for(65).as_deref(), Some("A"));
    assert_eq!(standard.glyph_name_for(48).as_deref(), Some("zero"));
    assert_eq!(standard.glyph_name_for(39).as_deref(), Some("quoteright"));
    assert_eq!(standard.glyph_name_for(96).as_deref(), Some("quoteleft"));
    assert_eq!(standard.glyph_name_for(0xA1).as_deref(), Some("exclamdown"));
    assert_eq!(standard.glyph_name_for(0xAE).as_deref(), Some("fi"));
    assert_eq!(standard.glyph_name_for(0xDF).as_deref(), Some("AE"));
    assert_eq!(standard.glyph_name_for(0xFB).as_deref(), Some("germandbls"));
    assert_eq!(standard.glyph_name_for(0xA0), None);
    assert_eq!(standard.glyph_name_for(127), None);
    assert_eq!(standard.unicode_for(65), Some('A'));
    assert_eq!(standard.unicode_for(48), Some('0'));
    assert_eq!(standard.unicode_for(39), Some('\u{2019}'));
    assert_eq!(standard.unicode_for(96), Some('\u{2018}'));
    assert_eq!(standard.unicode_for(0xA1), Some('\u{00a1}'));
    assert_eq!(standard.unicode_for(0xAE), Some('\u{fb01}'));
    assert_eq!(standard.unicode_for(0xDF), Some('\u{00c6}'));
    assert_eq!(standard.unicode_for(0xFB), Some('\u{00df}'));
    assert_eq!(standard.unicode_for(0xA0), None);
    assert_eq!(standard.unicode_for(127), None);

    let standard_with_differences = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![
                        Value::Integer(65),
                        Value::Name("quoteright".into()),
                        Value::Integer(66),
                        Value::Name("unmappedGlyph".into()),
                        Value::Integer(67),
                        Value::Name("a1".into()),
                    ])),
                )),
            ),
    );
    let standard = doc
        .type1_font_encoding(&standard_with_differences)
        .unwrap()
        .unwrap();
    assert_eq!(standard.unicode_for(65), Some('\u{2019}'));
    assert_eq!(standard.unicode_for(66), None);
    assert_eq!(standard.unicode_for(67), Some('\u{2701}'));

    let unsupported = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert("Encoding", Value::Name("UnknownEncoding".into())),
    );
    assert!(doc.type1_font_encoding(&unsupported).is_err());

    let invalid_differences = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![Value::Name("noCode".into())])),
                )),
            ),
    );
    assert!(doc.type1_font_encoding(&invalid_differences).is_err());

    let overflow_differences = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![
                        Value::Integer(255),
                        Value::Name("lastcode".into()),
                        Value::Name("overflow".into()),
                    ])),
                )),
            ),
    );
    assert!(doc.type1_font_encoding(&overflow_differences).is_err());

    let invalid_code = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert(
                    "Differences",
                    Value::Array(Array(vec![
                        Value::Integer(256),
                        Value::Name("outOfRange".into()),
                    ])),
                )),
            ),
    );
    assert!(doc.type1_font_encoding(&invalid_code).is_err());

    let invalid_base_encoding = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("ExampleSans".into()))
            .insert(
                "Encoding",
                Value::Dict(
                    Dict::new().insert("BaseEncoding", Value::Name("UnknownEncoding".into())),
                ),
            ),
    );
    assert!(doc.type1_font_encoding(&invalid_base_encoding).is_err());
}
