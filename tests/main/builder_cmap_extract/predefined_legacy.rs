use super::super::*;

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
