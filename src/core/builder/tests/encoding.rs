//! 自定义 CID 编码 CMap 测试。

use super::*;

#[test]
fn custom_cid_encoding_writes_embedded_cmap_with_ranges_and_chars() {
    let encoding = CidEncoding::new(0)
        .mapping(0x0020, 1)
        .mapping(0x0021, 2)
        .mapping(0x0022, 3)
        .mapping(0x0030, 10)
        .mapping(0x0032, 12);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes_unicode(
        p,
        idx,
        10.0,
        10.0,
        &[(0x0020, "A"), (0x0021, "B"), (0x0022, "C")],
    );
    let s = render(&mut b);

    // 嵌入 CMap 的头部与书写模式。
    assert!(
        s.contains("/CIDInit /ProcSet findresource begin"),
        "应写 CMap 头: {s}"
    );
    assert!(s.contains("/CMapType 1 def"), "编码 CMap 应为 Type 1: {s}");
    assert!(
        s.contains("/CMapName /Custom-H def"),
        "水平编码应命名为 Custom-H: {s}"
    );
    assert!(s.contains("/WMode 0 def"), "水平编码 WMode 应为 0: {s}");
    assert!(
        s.contains("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange"),
        "应写 2 字节 codespace: {s}"
    );

    // 字符码与 CID 同时连续的三项合并为 cidrange，孤立项写 cidchar。
    assert!(
        s.contains("1 begincidrange\n<0020> <0022> 1\nendcidrange"),
        "连续映射应写 cidrange: {s}"
    );
    assert!(
        s.contains("2 begincidchar\n<0030> 10\n<0032> 12\nendcidchar"),
        "非连续映射应写 cidchar: {s}"
    );
    assert!(s.contains("endcmap"), "应以 endcmap 收尾: {s}");

    // Type 0 的 /Encoding 必须引用 CMap 流，且不再写 Identity-H。
    let after_encoding = s
        .find("/Encoding ")
        .map(|pos| &s[pos + "/Encoding ".len()..])
        .expect("应写出 /Encoding");
    assert!(
        after_encoding.starts_with(|c: char| c.is_ascii_digit()),
        "自定义编码 /Encoding 应为流引用: {s}"
    );
    assert!(!s.contains("/Identity-H"), "不应再写 Identity-H: {s}");

    // 内容流按 2 字节大端序写字符码。
    assert!(
        s.contains("<002000210022> Tj"),
        "内容流应写字符码而非 CID: {s}"
    );
    // ToUnicode 的键是字符码 0x0020..0x0022，而不是 CID 1..3。
    assert!(
        s.contains("<0020> <0022> <0041>"),
        "ToUnicode 应以字符码为键: {s}"
    );
    assert!(
        !s.contains("<0001> <0003> <0041>"),
        "ToUnicode 不应以 CID 为键: {s}"
    );
}

#[test]
fn custom_cid_encoding_wmode_one_is_vertical() {
    let encoding = CidEncoding::new(1).mapping(0x41, 7);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes(p, idx, 10.0, 10.0, &[0x41]);
    let s = render(&mut b);

    assert!(s.contains("/CMapName /Custom-V def"), "垂直编码命名: {s}");
    assert!(s.contains("/WMode 1 def"), "垂直编码 WMode 应为 1: {s}");
    assert!(
        s.contains("1 begincidchar\n<0041> 7\nendcidchar"),
        "单项映射应写 cidchar: {s}"
    );
    assert!(s.contains("<0041> Tj"), "内容流应写字符码: {s}");
}

#[test]
fn cid_encoding_rejects_invalid_wmode_and_empty_mapping() {
    for encoding in [
        CidEncoding::new(2).mapping(1, 1),
        CidEncoding::new(0),
        CidEncoding::new(1),
    ] {
        let cid_font = CidFont::new(
            "ExampleCIDFont",
            CidFontSubtype::CIDFontType2,
            CidSystemInfo::new("Adobe", "Identity", 0),
            FontDescriptor::new(),
            12.0,
        )
        .with_encoding(encoding);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, &cid_font);
        b.text_codes(p, idx, 10.0, 10.0, &[1]);

        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("非法 wmode 或空映射应使构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "应返回 InvalidObject: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn predefined_cid_encoding_writes_name_reference_without_cmap_stream() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "GB1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("Adobe-GB1");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes(p, idx, 10.0, 10.0, &[0xA1A1]);
    let s = render(&mut b);

    // /Encoding 直接写名册名称引用，且不再写 Identity-H。
    assert!(
        s.contains("/Encoding /Adobe-GB1"),
        "预定义 CMap 应按名称引用写出: {s}"
    );
    assert!(!s.contains("/Identity-H"), "不应再写 Identity-H: {s}");
    // 不生成嵌入 CMap 流（本用例未登记 ToUnicode，全文不应出现 CMap 流）。
    assert!(!s.contains("/CIDInit"), "不应生成嵌入 CMap 流: {s}");
    assert!(s.contains("<A1A1> Tj"), "内容流应写字符码: {s}");
}

#[test]
fn predefined_cid_encoding_rejects_unknown_names() {
    // 竖排名在读端名册内同样可写（见 `builder_predefined_gb_v_encoding_*`）；
    // 此处只覆盖名册外名称。
    for name in ["NoSuch-CMap", "UniCNS-UCS2-HW"] {
        let cid_font = CidFont::new(
            "ExampleCIDFont",
            CidFontSubtype::CIDFontType2,
            CidSystemInfo::new("Adobe", "GB1", 0),
            FontDescriptor::new(),
            12.0,
        )
        .with_predefined_encoding(name);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, &cid_font);
        b.text_codes(p, idx, 10.0, 10.0, &[1]);

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("名册外的 CMap 名应使构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "应返回 InvalidObject: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn predefined_cid_encoding_conflicts_with_custom_encoding() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "GB1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(CidEncoding::new(0).mapping(1, 1))
    .with_predefined_encoding("Adobe-GB1");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes(p, idx, 10.0, 10.0, &[1]);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("预定义 CMap 与嵌入 CMap 混用应使构建失败");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "应返回 InvalidObject: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn predefined_cid_encoding_unicode_text_encodes_ucs2_and_registers_to_unicode() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "GB1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("UniGB-UCS2-H");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    // '😀' 在补充平面，无法表示为 2 字节码，应被跳过。
    b.text_predefined_unicode(p, idx, 10.0, 10.0, "中A😀");
    let s = render(&mut b);

    assert!(
        s.contains("/Encoding /UniGB-UCS2-H"),
        "应按名称引用 UniGB-UCS2-H: {s}"
    );
    assert!(
        s.contains("<4E2D0041> Tj"),
        "Unicode 码位应按 2 字节大端序写出，补充平面字符跳过: {s}"
    );
    // ToUnicode 以字符码（Unicode 码位）为键写 bfchar。
    assert!(
        s.contains("<4E2D> <4E2D>"),
        "ToUnicode 应登记 中 的映射: {s}"
    );
    assert!(
        s.contains("<0041> <0041>"),
        "ToUnicode 应登记 A 的映射: {s}"
    );
}

#[test]
fn predefined_cid_encoding_unicode_text_skips_codes_outside_codespace() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "GB1", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_predefined_encoding("Adobe-GB1");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    // Adobe-GB1 的 2 字节码空间为 <A1A1>..<FEFE>：
    // 中(U+4E2D) 在码空间外被跳过，가(U+AC00) 在码空间内保留。
    b.text_predefined_unicode(p, idx, 10.0, 10.0, "中\u{AC00}");
    let s = render(&mut b);

    assert!(s.contains("<AC00> Tj"), "码空间内字符应写出: {s}");
    assert!(!s.contains("4E2D"), "码空间外字符不应写出: {s}");

    // 未配置预定义编码的字体调用本 API 应被忽略。
    let plain = CidFont::new(
        "PlainCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &plain);
    b.text_predefined_unicode(p, idx, 10.0, 10.0, "中");
    let s = render(&mut b);
    assert!(!s.contains("Tj"), "未配置预定义编码时不应写文本: {s}");
}
