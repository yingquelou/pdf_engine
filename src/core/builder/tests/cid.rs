//! CIDFont 描述符、程序嵌入与内容流写出测试。

use super::*;

#[test]
fn cid_font_descriptor_is_written_when_provided() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);

    // CIDSet：CID 0, 2, 15 存在 → 位数组 [0b10100000, 0b00000001] = [0xA0, 0x01]
    let descriptor = FontDescriptor::new()
        .with_font_name("ExampleCIDFont")
        .with_family("Example Family")
        .with_ascent(800.0)
        .with_descent(-200.0)
        .with_cap_height(700.0)
        .with_stem_v(80.0)
        .with_flags(4)
        .with_bbox([-10.0, -200.0, 1000.0, 900.0])
        .with_cid_set(vec![0xA0, 0x01]);

    let system_info = CidSystemInfo::new("Adobe", "Identity", 0);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        system_info,
        descriptor,
        12.0,
    )
    .with_default_width(1000.0)
    .with_widths(vec![
        CidWidth::Single(65, 500.0),
        CidWidth::Range(100, 102, 600.0),
    ]);

    let cf_idx = b.cid_font(p, &cid_font);
    b.text_cid(p, cf_idx, 10.0, 10.0, &[65, 66]);
    let s = render(&mut b);

    // Type 0 父字体
    assert!(s.contains("/Subtype /Type0"), "应含 Type0 父字体: {s}");
    assert!(
        s.contains("/Encoding /Identity-H"),
        "应含 Identity-H 编码: {s}"
    );
    assert!(s.contains("/DescendantFonts"), "应含 DescendantFonts: {s}");

    // CIDFont 后代字典
    assert!(
        s.contains("/Subtype /CIDFontType2"),
        "应含 CIDFontType2: {s}"
    );
    assert!(
        s.contains("/BaseFont /ExampleCIDFont"),
        "应含 BaseFont: {s}"
    );
    assert!(s.contains("/CIDSystemInfo"), "应含 CIDSystemInfo: {s}");
    assert!(s.contains("/Registry (Adobe)"), "应含 Registry: {s}");
    assert!(s.contains("/Ordering (Identity)"), "应含 Ordering: {s}");

    // FontDescriptor
    assert!(
        s.contains("/Type /FontDescriptor"),
        "应含 FontDescriptor 类型: {s}"
    );
    assert!(
        s.contains("/FontName /ExampleCIDFont"),
        "应含 FontName: {s}"
    );
    assert!(
        s.contains("/FontFamily (Example Family)"),
        "应含 FontFamily: {s}"
    );
    assert!(s.contains("/Ascent 800"), "应含 Ascent: {s}");
    assert!(s.contains("/Descent -200"), "应含 Descent: {s}");
    assert!(s.contains("/CapHeight 700"), "应含 CapHeight: {s}");
    assert!(s.contains("/StemV 80"), "应含 StemV: {s}");
    assert!(s.contains("/Flags 4"), "应含 Flags: {s}");

    // CIDSet 流
    assert!(s.contains("/CIDSet"), "应含 CIDSet 引用: {s}");
}

#[test]
fn cid_font_type2_embeds_truetype_program() {
    let program = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let s = render(&mut b);

    assert!(s.contains("/FontFile2"), "应含 /FontFile2 引用: {s}");
    assert!(
        s.contains(&format!("/Length {}", program.len())),
        "FontFile2 流长度应与程序字节一致: {s}"
    );
    assert!(!s.contains("/FontFile3"), "不应写 /FontFile3: {s}");
}

#[test]
fn cid_font_type0_embeds_cff_program_with_cidfonttype0c_subtype() {
    let program = vec![0x01, 0x00, 0x04, 0x05];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let s = render(&mut b);

    assert!(s.contains("/FontFile3"), "应含 /FontFile3 引用: {s}");
    assert!(
        s.contains("/Subtype /CIDFontType0C"),
        "FontFile3 流应声明 /CIDFontType0C: {s}"
    );
    assert!(!s.contains("/FontFile2"), "不应写 /FontFile2: {s}");
}

#[test]
fn cid_font_rejects_mismatched_program_type() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(vec![0x00]));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CIDFontType0 不应接受 TrueType 程序");
    assert!(
        error.to_string().contains("CIDFontType0"),
        "错误信息应指出子类型: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn cid_font_without_unicode_mappings_omits_to_unicode() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[65]);
    let s = render(&mut b);

    assert!(
        !s.contains("/ToUnicode"),
        "未登记映射时不应写 /ToUnicode: {s}"
    );
}

#[test]
fn text_codes_writes_two_byte_big_endian_hex() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes(p, idx, 10.0, 10.0, &[0x1234, 0xABCD]);
    let s = render(&mut b);

    assert!(
        s.contains("<1234ABCD> Tj"),
        "字符码应按 2 字节大端序写出: {s}"
    );
    // 未配置自定义编码时仍走 Identity-H。
    assert!(
        s.contains("/Encoding /Identity-H"),
        "默认仍为 Identity-H: {s}"
    );
}
