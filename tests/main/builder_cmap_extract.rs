use super::*;

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

/// 写入 → 重新解析 → 提取：自定义嵌入 CMap（非 Identity）应把 2 字节
/// 字符码映射为 CID，ToUnicode 以字符码为键，提取器仍还原出原文。
#[test]
fn builder_custom_cid_encoding_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let encoding = CidEncoding::new(0)
        .mapping(0x0020, 1)
        .mapping(0x0021, 2)
        .mapping(0x0022, 3);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(0x0020, "A"), (0x0021, "B"), (0x0022, "C")],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    assert!(
        matches!(
            type0.as_dict().and_then(|dict| dict.get("Encoding")),
            Some(Value::Ref(_, _))
        ),
        "自定义编码应以流引用写出"
    );
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert!(!font.is_vertical(), "WMode 0 应为水平书写");
    let decoded = font
        .decode_codes(&[0x00, 0x20, 0x00, 0x21, 0x00, 0x22])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [1, 2, 3],
        "字符码应经嵌入 CMap 映射为 CID"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("A"), Some("B"), Some("C")]
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("ABC"));
}

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

/// 写入 → 重新解析 → 提取：1/2/3 字节混合字符码经嵌入 CMap 映射为 CID，
/// 读端按最长匹配解码，ToUnicode 键长度与字符码一致。
#[test]
fn builder_multibyte_cid_cmap_roundtrips_through_extractor() {
    use pdf_engine::core::builder::{
        CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let encoding = CidEncoding::new(0)
        .mapping_bytes(&[0x41], 1)
        .mapping(0xB1B2, 2)
        .mapping_bytes(&[0x01, 0x02, 0x03], 3);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_code_bytes_unicode(
        page,
        index,
        72.0,
        700.0,
        &[
            (&[0x41], "A"),
            (&[0xB1, 0xB2], "B"),
            (&[0x01, 0x02, 0x03], "C"),
        ],
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
        .decode_codes(&[0x41, 0xB1, 0xB2, 0x01, 0x02, 0x03])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [1, 2, 3],
        "混合码长应最长匹配并映射为 CID"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.code.len())
            .collect::<Vec<_>>(),
        [1, 2, 3],
        "回读应保留字符码字节长度"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("A"), Some("B"), Some("C")]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("ABC"));
}

/// 写入 → 重新解析 → 提取：自定义 /CMapName 与 usecmap 基座（Identity-H）
/// 一同写出；本地映射覆盖基座，未覆盖码仍继承基座映射。
#[test]
fn builder_usecmap_and_custom_cmap_name_roundtrip() {
    use pdf_engine::core::builder::{
        CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let encoding = CidEncoding::new(0)
        .with_cmap_name("Roundtrip-H")
        .with_usecmap("Identity-H")
        .mapping(0x0041, 9);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0x0041, "A"), (0x0043, "C")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("/CMapName /Roundtrip-H def"),
        "应写自定义 CMapName"
    );
    assert!(text.contains("/Identity-H usecmap"), "应写 usecmap 基座");

    let reparsed = parse_buf(&buf);
    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x00, 0x41, 0x00, 0x43])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [9, 0x43],
        "本地映射覆盖基座，未覆盖码继承 Identity-H"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("A"), Some("C")]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AC"));
}

/// 写入 → 重新解析 → 提取：预定义 CMap 的 vendored 真实数据（GB-EUC-H）
/// 以名称引用写出；读端按 Adobe-GB1 ordering 解出真实 CID，ToUnicode 以
/// 字符码为键，提取器还原原文。
#[test]
fn builder_predefined_gb_euc_cmap_roundtrips_real_ordering() {
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
    .with_predefined_encoding("GB-EUC-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0xA1A1, "中"), (0xB0A1, "啊")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    assert!(
        matches!(
            type0.as_dict().and_then(|dict| dict.get("Encoding")),
            Some(Value::Name(name)) if name == "GB-EUC-H"
        ),
        "vendored 预定义 CMap 应以名称引用写出"
    );
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert!(!font.is_vertical());
    let decoded = font
        .decode_codes(&[0xA1, 0xA1, 0xB0, 0xA1])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [96, 940],
        "CID 应来自 Adobe-GB1 真实 ordering（A1A1→96、B0A1→940）"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.unicode.as_deref())
            .collect::<Vec<_>>(),
        [Some("中"), Some("啊")]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中啊"));
}

/// 写入 → 重新解析 → 提取：vendored 竖排变体（GB-V）经文件内 `usecmap`
/// 继承 GB-H，读端识别 WMode 1，本地覆盖码与继承码都解出真实 CID。
#[test]
fn builder_predefined_gb_v_encoding_roundtrips_vertical_mode() {
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
    .with_predefined_encoding("GB-V");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0x2121, "上"), (0x2122, "下")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert!(font.is_vertical(), "GB-V 的 /WMode 1 应被识别为竖排");
    let decoded = font
        .decode_codes(&[0x21, 0x21, 0x21, 0x22])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [96, 575],
        "0x2121 继承 GB-H 的 CID 96，0x2122 由 GB-V 覆盖为 CID 575"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("上下"));
}

/// 写入 → 重新解析 → 提取：GBK-EUC-H 预定义 CMap 以名称引用写出，
/// 字符码经 vendored 的 Adobe ordering 映射为 CID，提取由 ToUnicode 驱动。
#[test]
fn builder_predefined_gbk_euc_cmap_roundtrips_real_ordering() {
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
    .with_predefined_encoding("GBK-EUC-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0x8140, "GBK"), (0x8179, "99")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x81, 0x40, 0x81, 0x79])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [10072, 8281],
        "0x8140/0x8179 应按 GBK-EUC-H 原文映射"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("GBK99"));
}

/// 写入 → 重新解析 → 提取：GBK2K-H 的 4 字节 codespace 与 2 字节区间在数值
/// 前缀上重叠，读端按声明最长码优先解出 4 字节码，2 字节码放在末尾避免被
/// 当作 4 字节前缀消费。
#[test]
fn builder_predefined_gbk2k_cmap_roundtrips_four_and_two_byte_codes() {
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
    .with_predefined_encoding("GBK2K-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_code_bytes_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(&[0x81, 0x30, 0x84, 0x36], "中"), (&[0x81, 0x40], "文")],
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
        .decode_codes(&[0x81, 0x30, 0x84, 0x36, 0x81, 0x40])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [22354, 10072],
        "4 字节 `<81308436>` 与 2 字节 `<8140>` 应按 GBK2K-H 原文映射"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.code.len())
            .collect::<Vec<_>>(),
        [4, 2]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中文"));
}

/// 写入 → 重新解析 → 提取：CNS-EUC-H 同时含 2 字节与 4 字节 EUC-TW 码空间，
/// 名称引用写出后读端按最长码优先解出真实 CID。
#[test]
fn builder_predefined_cns_euc_cmap_roundtrips_two_and_four_byte_codes() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "CNS1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("CNS-EUC-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_code_bytes_unicode(
        page,
        index,
        72.0,
        700.0,
        &[(&[0xA1, 0xA1], "中"), (&[0x8E, 0xA2, 0xA1, 0xA1], "文")],
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
        .decode_codes(&[0xA1, 0xA1, 0x8E, 0xA2, 0xA1, 0xA1])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [99, 5996],
        "2 字节与 4 字节码应分别取本表区间"
    );
    assert_eq!(
        decoded
            .iter()
            .map(|code| code.code.len())
            .collect::<Vec<_>>(),
        [2, 4]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("中文"));
}

/// 写入 → 重新解析 → 提取：Japan1 的 90ms-RKSJ-H 真实 ordering。
#[test]
fn builder_predefined_japan_rksj_cmap_roundtrips_real_ordering() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Japan1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("90ms-RKSJ-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0x8140, "日"), (0x82A0, "あ")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x81, 0x40, 0x82, 0xA0])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [633, 843],
        "0x8140/0x82A0 应按 Japan1 原文映射"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("日あ"));
}

/// 写入 → 重新解析 → 提取：Korea1 的 KSCms-UHC-H 真实 ordering。
#[test]
fn builder_predefined_korea_uhc_cmap_roundtrips_real_ordering() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, PageSize, PdfBuilder,
    };

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Korea1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("KSCms-UHC-H");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_codes_unicode(page, index, 72.0, 700.0, &[(0x8141, "가"), (0xA1A1, "！")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let type0 = find_font_dict(&reparsed, "Type0");
    let font = reparsed
        .type0_font(&type0)
        .expect("should parse Type0")
        .expect("should be Type0");
    let decoded = font
        .decode_codes(&[0x81, 0x41, 0xA1, 0xA1])
        .expect("should decode");
    assert_eq!(
        decoded.iter().map(|code| code.cid).collect::<Vec<_>>(),
        [9333, 101],
        "0x8141/0xA1A1 应按 Korea1 原文映射"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("가！"));
}
