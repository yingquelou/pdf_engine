use super::*;

/// 写入 → 重新解析 → 提取：builder 写出的 Type 3 字体字典应能被读端
/// 解析出矩阵、宽度与字符过程，文本提取仍还原出原文（9.6.5）。
#[test]
fn builder_type3_font_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, Type3Font, Type3Glyph};

    let type3 = Type3Font::new()
        .glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"))
        .glyph(Type3Glyph::new(66, "B", 600.0, b"600 0 d0"));
    let font = Font::type3(12.0, type3);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type3");
    let parsed = reparsed
        .type3_font(&font_dict)
        .expect("should parse Type 3 font")
        .expect("should be Type 3");
    assert_eq!(parsed.font_matrix, [0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
    assert_eq!(parsed.font_bbox, [0.0, 0.0, 1000.0, 1000.0]);
    assert_eq!(parsed.first_char, 65);
    assert_eq!(parsed.widths, vec![500.0, 600.0]);
    assert_eq!(parsed.char_procs, vec!["A".to_string(), "B".to_string()]);
    assert_eq!(parsed.width_for(65), Some(500.0));
    assert_eq!(parsed.width_for(66), Some(600.0));
    assert_eq!(parsed.width_for(64), None);

    // /CharProcs 的每个条目应解析为含原始字符过程字节的流。
    let char_procs = font_dict
        .as_dict()
        .and_then(|dict| dict.get("CharProcs"))
        .and_then(Value::as_dict)
        .expect("/CharProcs");
    for (name, expected) in [("A", b"500 0 d0".as_slice()), ("B", b"600 0 d0".as_slice())] {
        let (proc_num, _) = char_procs
            .get(name)
            .and_then(Value::as_ref)
            .unwrap_or_else(|| panic!("缺少字符过程 {name}"));
        let proc_object = reparsed
            .get_object(proc_num)
            .expect("字符过程查找")
            .expect("字符过程对象");
        let bytes = reparsed
            .decode_stream_bytes(&proc_object.value)
            .expect("字符过程流应可解码");
        assert_eq!(bytes, expected, "{name} 的字符过程字节");
    }

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析：builder 写出的 Type 3 字形程序以 d0/d1 开头时，读端
/// 应能按字符码查询其声明的宽度与包围盒，且文本提取不受影响（9.6.5）。
#[test]
fn builder_type3_glyph_metrics_roundtrip_with_text_extraction() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, Type3Font, Type3Glyph};

    // 手工构造含 d0/d1 前缀的 CharProcs 字节：A 用 d1 声明宽度与包围盒，
    // B 用 d0 仅声明宽度（与 /Widths 中的 600 故意不同，两者独立可查）。
    let type3 = Type3Font::new()
        .glyph(Type3Glyph::new(
            65,
            "A",
            500.0,
            b"500 0 10 20 490 680 d1 10 20 480 660 re f",
        ))
        .glyph(Type3Glyph::new(66, "B", 600.0, b"650 0 d0"));
    let font = Font::type3(12.0, type3);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type3");
    let parsed = reparsed
        .type3_font(&font_dict)
        .expect("should parse Type 3 font")
        .expect("should be Type 3");

    // d1：声明宽度 + 包围盒（glyph 空间）。
    let metrics_a = parsed
        .glyph_metrics(&reparsed, 65)
        .expect("glyph A metrics")
        .expect("glyph A should have metrics");
    assert_eq!(metrics_a.width, [500.0, 0.0]);
    assert_eq!(metrics_a.bbox, Some([10.0, 20.0, 490.0, 680.0]));

    // d0：仅声明宽度，无包围盒；与 /Widths 的 600 共存且互不干扰。
    let metrics_b = parsed
        .glyph_metrics(&reparsed, 66)
        .expect("glyph B metrics")
        .expect("glyph B should have metrics");
    assert_eq!(metrics_b.width, [650.0, 0.0]);
    assert_eq!(metrics_b.bbox, None);
    assert_eq!(parsed.width_for(66), Some(600.0));

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析 → 提取：Type 3 的 `/Name`、`/FontDescriptor`、
/// 字体级 `/Resources` 与 `/ToUnicode` 应完整往返；`/ToUnicode` 优先于
/// `/Encoding` 决定提取出的文本（9.6.5，Table 112）。
#[test]
fn builder_type3_metadata_and_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Font, FontDescriptor, PageSize, PdfBuilder, Type3Font, Type3Glyph,
    };

    let resources = Dict::new().insert(
        "XObject",
        Value::Dict(Dict::new().insert("Im1", Value::Integer(1))),
    );
    let type3 = Type3Font::new()
        .glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"))
        .with_name("SampleType3")
        .with_resources(Value::Dict(resources.clone()))
        .with_to_unicode(65, "Ω");
    let descriptor = FontDescriptor::new()
        .with_family("Sample")
        .with_ascent(800.0);
    let font = Font::type3(12.0, type3).with_descriptor(descriptor);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type3");
    let parsed = reparsed
        .type3_font(&font_dict)
        .expect("should parse Type 3 font")
        .expect("should be Type 3");

    assert_eq!(parsed.name.as_deref(), Some("SampleType3"));
    assert!(parsed.has_to_unicode, "应记录 /ToUnicode 条目");
    let descriptor = parsed
        .descriptor
        .as_ref()
        .expect("应解析出 /FontDescriptor");
    assert_eq!(descriptor.font_family.as_deref(), Some("Sample"));
    assert_eq!(descriptor.ascent, Some(800.0));
    assert_eq!(
        parsed.resources(&reparsed).expect("应解析 /Resources"),
        Some(resources)
    );
    assert_eq!(
        parsed
            .resource(&reparsed, "XObject", "Im1")
            .expect("资源查找"),
        Some(Value::Integer(1))
    );
    assert_eq!(
        parsed.resource(&reparsed, "XObject", "Missing").unwrap(),
        None
    );

    // /ToUnicode 映射优先于 /Encoding：字符码 65 提取为 Ω 而非 A。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Ω"));
}
