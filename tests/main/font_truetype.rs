use super::*;

#[test]
fn truetype_font_program_resolves_and_parses_fontfile2() {
    let doc = Document::new();
    let sfnt = sample_sfnt_font();
    let font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile2",
                            Value::Stream(Dict::new(), StreamData::Owned(sfnt.clone())),
                        ),
                ),
            ),
    );

    let font = doc.truetype_font(&font).unwrap().unwrap();
    assert_eq!(font.flavor(), SfntFlavor::TrueType);
    assert_eq!(font.units_per_em(), 1000);
    assert_eq!(font.glyph_count(), 3);
    assert_eq!(font.horizontal_advance_width(0), Some(500));
    assert_eq!(font.horizontal_advance_width(2), Some(600));
    assert_eq!(font.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    let glyph = font.glyph(0).unwrap().unwrap();
    assert_eq!(glyph.bounds, [1, 2, 5, 8]);
    assert_eq!(glyph.points.len(), 1);
    assert_eq!(font.glyph(2).unwrap().unwrap().points.len(), 1);
    assert_eq!(font.table(b"head").unwrap().len(), 54);

    let cid_font = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("CIDFontType2".into()))
            .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
            .insert(
                "CIDSystemInfo",
                Value::Dict(
                    Dict::new()
                        .insert("Registry", Value::String("Adobe".into()))
                        .insert("Ordering", Value::String("Identity".into()))
                        .insert("Supplement", Value::Integer(0)),
                ),
            )
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile2",
                            Value::Stream(Dict::new(), StreamData::Owned(sfnt)),
                        ),
                ),
            ),
    );
    let cid_font = doc.cid_font_type2(&cid_font).unwrap().unwrap();
    assert_eq!(cid_font.glyph_id_for_cid(2), Some(2));
    assert_eq!(cid_font.glyph_id_for_cid(3), None);
    assert_eq!(
        cid_font
            .true_type_font()
            .and_then(|font| font.glyph(2).ok().flatten())
            .unwrap()
            .points
            .len(),
        1
    );

    let no_program = Value::Dict(Dict::new().insert("Subtype", Value::Name("TrueType".into())));
    assert!(doc.truetype_font(&no_program).unwrap().is_none());

    let wrong_program = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile",
                            Value::Stream(Dict::new(), StreamData::Owned(vec![0; 32])),
                        ),
                ),
            ),
    );
    assert!(doc.truetype_font(&wrong_program).is_err());
}

/// 写入 → 重新解析 → 提取：Type 0 + CIDFontType2（合成 TrueType 程序、
/// ToUnicode CMap）下的十六进制 CID 文本应原样还原为 Unicode。
#[test]
fn type0_text_roundtrips_through_writer_and_extractor() {
    let mut content = core::content::ContentStream::new();
    content
        .begin_text()
        .set_font("F1", 18.0)
        .move_next_line(72.0, 700.0)
        .show_text_hex([0x00u8, 0x41, 0x00, 0x42, 0x00, 0x43])
        .end_text();

    let to_unicode = b"/CIDInit /ProcSet findresource begin\n\
12 dict begin\n\
begincmap\n\
/CMapName /Test-UCS2 def\n\
/CMapType 2 def\n\
1 begincodespacerange\n\
<0000> <FFFF>\n\
endcodespacerange\n\
3 beginbfchar\n\
<0041> <4E2D>\n\
<0042> <6587>\n\
<0043> <D83DDE00>\n\
endbfchar\n\
endcmap\n\
CMapName currentdict /CMap defineresource pop\n\
end\n\
end"
    .to_vec();

    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Contents", Value::Ref(5, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert(
                        "Font",
                        Value::Dict(Dict::new().insert("F1", Value::Ref(4, 0))),
                    )),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
                .insert("Encoding", Value::Name("Identity-H".into()))
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(Value::Ref(6, 0))),
                )
                .insert("ToUnicode", Value::Ref(7, 0)),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(content.to_bytes())),
    ));
    doc.set_object(Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
                .insert(
                    "CIDSystemInfo",
                    Value::Dict(
                        Dict::new()
                            .insert("Registry", Value::String("Adobe".into()))
                            .insert("Ordering", Value::String("Identity".into()))
                            .insert("Supplement", Value::Integer(0)),
                    ),
                )
                .insert("CIDToGIDMap", Value::Name("Identity".into()))
                .insert("FontDescriptor", Value::Ref(8, 0)),
        ),
    ));
    doc.set_object(Object::new(
        7,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(to_unicode)),
    ));
    doc.set_object(Object::new(
        8,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("ExampleCIDFont".into()))
                .insert("FontFile2", Value::Ref(9, 0)),
        ),
    ));
    doc.set_object(Object::new(
        9,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(sample_sfnt_font())),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(10)),
    };

    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let reparsed = parse_buf(&buf);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中文😀"));
}

/// 写入 → 重新解析 → 提取：缺少 `/Encoding` 的 TrueType 简单字体应通过
/// 嵌入程序的 cmap（内建编码）解码，而不是按 `/BaseFont`（Symbol）的
/// 预定义编码把 0x41 解成希腊字母 Α。
#[test]
fn truetype_without_encoding_extracts_through_builtin_cmap() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Contents", Value::Ref(5, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert(
                        "Font",
                        Value::Dict(Dict::new().insert("F1", Value::Ref(4, 0))),
                    )),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("TrueType".into()))
                .insert("BaseFont", Value::Name("Symbol".into()))
                .insert("FontDescriptor", Value::Ref(6, 0)),
        ),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Stream(
            Dict::new(),
            StreamData::Owned(b"BT /F1 12 Tf (A) Tj ET".to_vec()),
        ),
    ));
    doc.set_object(Object::new(
        6,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("FontDescriptor".into()))
                .insert("FontName", Value::Name("Symbol".into()))
                .insert("FontFile2", Value::Ref(7, 0)),
        ),
    ));
    doc.set_object(Object::new(
        7,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(sample_sfnt_font())),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(8)),
    };

    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let reparsed = parse_buf(&buf);
    // sample_sfnt_font 的 cmap 将 0x41 映射到 glyph 2，反向为 U+0041；
    // Symbol 预定义编码则会把 0x41 解成 U+0391 (Α)。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}
