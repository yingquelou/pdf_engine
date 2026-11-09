use super::super::*;

/// 写入 → 重新解析 → 提取：预定义 CMap（GB1 的 UCS2 变体 UniGB-UCS2-H）
/// 以名称引用作为 `/Encoding` 写出，Unicode 文本按码位写 2 字节字符码，
/// ToUnicode 以字符码为键，提取器仍还原出原文。
#[test]
fn builder_predefined_cmap_encoding_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "GB1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("UniGB-UCS2-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_predefined_unicode(page, index, 72.0, 700.0, "中文GB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    assert!(
        matches!(
            type0.as_dict().and_then(|dict| dict.get("Encoding")),
            Some(Value::Name(name)) if name == "UniGB-UCS2-H"
        ),
        "预定义 CMap 应以名称引用写出"
    );
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert!(!font.is_vertical(), "UniGB-UCS2-H 应为水平书写");
    let decoded = font
        .decode_codes(&[0x4E, 0x2D, 0x65, 0x87, 0x00, 0x47, 0x00, 0x42])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [4559, 3795, 40, 35],
        "真实 UniGB-UCS2-H：4E2D→4559、6587→3795、0047→40、0042→35（提取由 ToUnicode 驱动）"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("中"), Some("文"), Some("G"), Some("B")]
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中文GB"));
}

/// 写入 → 重新解析 → 提取：B6.4 的竖排变体 UniGB-UCS2-V 经文件内
/// `usecmap` 继承 UniGB-UCS2-H 并置 WMode 1，本地覆盖码生效。
#[test]
fn builder_predefined_uni_gb_vertical_encoding_roundtrips_vertical_mode() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "GB1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("UniGB-UCS2-V");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0x4E2D, "中"), (0x2014, "—")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert!(font.is_vertical(), "UniGB-UCS2-V 的 WMode 1 应被识别为竖排");
    let decoded = font
        .decode_codes(&[0x4E, 0x2D, 0x20, 0x14])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [4559, 598],
        "4E2D 继承 H 的 CID 4559，2014 由 V 覆盖为 CID 598"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("中"), Some("—")]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中—"));
}
