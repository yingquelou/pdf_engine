use super::*;

/// 写入 → 重新解析： CIDFont 的系统信息、宽度与描述符（含 `/CIDSet`）
/// 应可从生成的文档还原。
#[test]
fn builder_cid_font_descriptor_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, CidWidth, FontDescriptor, PageSize, PdfBuilder,
    };

    let descriptor = FontDescriptor::new()
        .with_font_name("ExampleCIDFont")
        .with_family("Example Family")
        .with_ascent(800.0)
        .with_descent(-200.0)
        .with_flags(4)
        .with_cid_set(vec![0xA0, 0x01]);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        descriptor,
        12.0,
    )
    .with_widths(vec![
        CidWidth::Single(2, 500.0),
        CidWidth::Range(3, 5, 600.0),
    ]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[2, 3, 4]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(parsed.system_info().registry, "Adobe");
    assert_eq!(parsed.system_info().ordering, "Identity");
    assert_eq!(parsed.width_for_cid(2), 500.0);
    assert_eq!(parsed.width_for_cid(3), 600.0);
    assert_eq!(parsed.width_for_cid(6), 1000.0);
    let metrics = parsed.descriptor();
    assert_eq!(metrics.ascent, Some(800.0));
    assert_eq!(metrics.descent, Some(-200.0));
    assert_eq!(metrics.cid_is_present(0), Some(true));
    assert_eq!(metrics.cid_is_present(1), Some(false));
    assert_eq!(metrics.cid_is_present(2), Some(true));
    assert_eq!(metrics.cid_is_present(15), Some(true));
}

/// 写入 → 重新解析：CIDFont 的 `/DW2` 与 `/W2` 垂直度量应能还原，
/// 未列出的 CID 按 `/DW2` 与水平宽度的一半回退。
#[test]
fn builder_cid_font_vertical_metrics_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, CidVerticalMetrics, CidWidth, FontDescriptor,
        PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::font::{DefaultVerticalMetrics, VerticalMetrics};

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_widths(vec![CidWidth::Single(3, 600.0)])
    .with_default_vertical_metrics(900.0, -800.0)
    .with_vertical_metrics(vec![
        CidVerticalMetrics::Single(1, -500.0, 250.0, 700.0),
        CidVerticalMetrics::Range(5, 6, -600.0, 300.0, 800.0),
    ]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[1]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(
        parsed.default_vertical_metrics(),
        DefaultVerticalMetrics {
            v_y: 900.0,
            w1_y: -800.0
        }
    );
    assert_eq!(
        parsed.vertical_metrics_for_cid(1),
        VerticalMetrics {
            w1_y: -500.0,
            v1x: 250.0,
            v1y: 700.0
        }
    );
    assert_eq!(
        parsed.vertical_metrics_for_cid(6),
        VerticalMetrics {
            w1_y: -600.0,
            v1x: 300.0,
            v1y: 800.0
        }
    );
    // CID 3 无 /W2：v_x 取水平宽度的一半，其余取 /DW2。
    assert_eq!(
        parsed.vertical_metrics_for_cid(3),
        VerticalMetrics {
            w1_y: -800.0,
            v1x: 300.0,
            v1y: 900.0
        }
    );
}

/// 写入 → 重新解析：CIDFont 描述符的 `/Style /Panose`、`/Lang` 与 `/FD`
/// 字形类覆盖应原样保留。
#[test]
fn builder_cid_font_descriptor_style_lang_fd_roundtrip() {
    use std::collections::BTreeMap;

    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "Proportional".to_string(),
        FontDescriptor::new()
            .with_ascent(750.0)
            .with_descent(-250.0),
    );
    let descriptor = FontDescriptor::new()
        .with_style_panose([
            0x02, 0x0B, 0x05, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ])
        .with_lang("zh-CN")
        .with_fd(overrides);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        descriptor,
        12.0,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[1]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    let descriptor = parsed.descriptor();
    assert_eq!(
        descriptor.style_panose,
        Some([
            0x02, 0x0B, 0x05, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ])
    );
    assert_eq!(descriptor.lang.as_deref(), Some("zh-CN"));
    let fd = descriptor.fd.as_ref().expect("/FD");
    let proportional = fd.get("Proportional").expect("Proportional 覆盖");
    assert_eq!(proportional.get("Ascent"), Some(&Value::Float(750.0)));
    assert_eq!(proportional.get("Descent"), Some(&Value::Float(-250.0)));
}

/// 写入 → 重新解析： CIDFontType2 嵌入的 TrueType 程序应作为 `/FontFile2`
/// 保存，读端可还原原始字节并继续解析 CIDFont。
#[test]
fn builder_cid_font_type2_embeds_truetype_program_roundtrip() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };

    let sfnt = sample_sfnt_font();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(sfnt.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[2]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let embedded = reparsed
        .cid_font_type2_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_eq!(embedded.data, sfnt);

    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(parsed.system_info().registry, "Adobe");
    assert_eq!(parsed.system_info().ordering, "Identity");
}

/// 写入 → 重新解析：CIDFontType2 的 TrueType 程序按 CID 子集化后，
/// `/CIDToGIDMap` 应按新 gid 重建，读端 CID→glyph 解析与文本提取保持一致。
#[test]
fn builder_cid_font_type2_subset_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::font::TrueTypeFont;

    let sfnt = sample_sfnt_font();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(sfnt.clone()))
    .with_cid_to_gid_map(&[0, 1, 2])
    .with_subset_cids(&[2]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(page, index, 72.0, 700.0, &[(2, "A")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType2");
    let base_font = descendant
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    assert!(
        base_font.contains('+'),
        "子集 BaseFont 应带标签前缀: {base_font}"
    );

    let embedded = reparsed
        .cid_font_type2_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_ne!(embedded.data, sfnt, "嵌入程序应为子集");
    assert!(embedded.data.len() < sfnt.len(), "子集程序应更短");
    let subset = TrueTypeFont::parse(embedded.data).expect("子集程序应可解析");
    assert_eq!(subset.glyph_count(), 2, "应只保留 glyph 0 与原 gid 2");

    let parsed = reparsed
        .cid_font_type2(&descendant)
        .expect("should parse CIDFontType2")
        .expect("should be CIDFontType2");
    assert_eq!(parsed.glyph_id_for_cid(2), Some(1), "CID 2 应指向新 gid 1");
    assert_eq!(parsed.glyph_id_for_cid(0), Some(0));
    assert_eq!(parsed.glyph_id_for_cid(1), Some(0), "未保留的 CID 应归零");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}

/// 写入 → 重新解析： CIDFontType0 嵌入的 CFF 程序应作为 `/FontFile3`
/// 保存，流子类型为 `/CIDFontType0C`。
#[test]
fn builder_cid_font_type0_embeds_cff_program_roundtrip() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };

    let cff = vec![0x01, 0x00, 0x04, 0x05];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(cff.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[2]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType0");
    let embedded = reparsed
        .cid_font_type0_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("CIDFontType0C"));
    assert_eq!(embedded.data, cff);

    let parsed = reparsed
        .cid_font_type0(&descendant)
        .expect("should parse CIDFontType0")
        .expect("should be CIDFontType0");
    let program = parsed.font_program().expect("should keep program");
    assert_eq!(program.subtype.as_deref(), Some("CIDFontType0C"));
    assert_eq!(program.data, cff);
}

/// 写入 → 重新解析 → 提取：CIDFontType0 嵌入 CFF-flavored OpenType
/// 程序时应写 `/FontFile3` `/Subtype /OpenType`，文本经 ToUnicode 还原。
#[test]
fn builder_cid_font_type0_opentype_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };

    let program = sample_opentype_cff_sfnt();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::OpenType(program.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(page, index, 72.0, 700.0, &[(7, "Hi")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let descendant = find_font_dict(&reparsed, "CIDFontType0");
    let embedded = reparsed
        .cid_font_type0_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("OpenType"));
    assert_eq!(embedded.data, program);

    let parsed = reparsed
        .cid_font_type0(&descendant)
        .expect("should parse CIDFontType0")
        .expect("should be CIDFontType0");
    let embedded = parsed.font_program().expect("should keep program");
    assert_eq!(embedded.subtype.as_deref(), Some("OpenType"));
    assert_eq!(embedded.data, program);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Hi"));
}
