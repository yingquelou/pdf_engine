use super::super::*;

/// 写入 → 重新解析 → 提取：builder 为 CIDFont 写出的 `/ToUnicode`
/// CMap 应让读端把 CID 还原为 Unicode 文本。
#[test]
fn builder_cid_font_to_unicode_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(1, "T"), (2, "é"), (3, "测"), (4, "😀")],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04])
        .expect("should decode");
    let text: Vec<&str> = decoded
        .iter()
        .map(|code| code.unicode.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(text, ["T", "é", "测", "😀"]);

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Té测😀"));
}

/// 写入 → 重新解析 → 提取：`/ToUnicode` 的 `Adobe-Identity-UCS` 基座
/// 应让未显式映射的 2 字节字符码按码值还原为 Unicode。
#[test]
fn builder_to_unicode_identity_base_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_to_unicode_usecmap("Adobe-Identity-UCS");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid(page, index, 72.0, 700.0, &[0x0041, 0x0042]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x00, 0x41, 0x00, 0x42])
        .expect("should decode");
    let text: Vec<&str> = decoded
        .iter()
        .map(|code| code.unicode.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(text, ["A", "B"]);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析 → 提取：连续 CID 的 `/ToUnicode` 应以 `bfrange`
/// 写出，读端仍能把 CID 序列还原为文本。
#[test]
fn builder_cid_font_bfrange_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(1, "A"), (2, "B"), (3, "C"), (4, "D"), (5, "E")],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let (cmap_num, _) = type0
        .as_dict()
        .and_then(|dict| dict.get("ToUnicode"))
        .and_then(Value::as_ref)
        .expect("Type 0 字体应引用 ToUnicode");
    let cmap = reparsed
        .get_object(cmap_num)
        .expect("ToUnicode 对象应可解析")
        .expect("ToUnicode 对象应存在");
    let cmap_bytes = reparsed
        .decode_stream_bytes(&cmap.value)
        .expect("ToUnicode 流应可解码");
    let cmap_text = String::from_utf8_lossy(&cmap_bytes);
    assert!(
        cmap_text.contains("<0001> <0005> <0041>"),
        "连续 CID 应以 bfrange 写出: {cmap_text}"
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("ABCDE"));
}
