//! CIDFont 的 FontDescriptor 写出、Table 122 兜底与 `/Style /Lang /FD` 覆盖。

use super::super::*;

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
fn cid_font_descriptor_defaults_fill_table_122_required_entries() {
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
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let descriptor = font_descriptor(&doc, &descendant);
    assert_eq!(
        descriptor.get("FontName").and_then(Value::as_name),
        Some("ExampleCIDFont")
    );
    assert_eq!(descriptor.get("Flags").and_then(Value::as_integer), Some(4));
    assert_eq!(
        descriptor.get("FontBBox").map(numbers),
        Some(vec![0.0, -200.0, 1000.0, 900.0])
    );
    assert_eq!(scalar(descriptor.get("ItalicAngle")), Some(0.0));
    assert_eq!(scalar(descriptor.get("Ascent")), Some(800.0));
    assert_eq!(scalar(descriptor.get("Descent")), Some(-200.0));
    assert_eq!(scalar(descriptor.get("CapHeight")), Some(700.0));
    assert_eq!(scalar(descriptor.get("StemV")), Some(80.0));
}

#[test]
fn cid_font_descriptor_writes_panose_lang_and_fd_overrides() {
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert(
        "Proportional".to_string(),
        FontDescriptor::new()
            .with_bbox([0.0, -250.0, 1000.0, 900.0])
            .with_ascent(750.0),
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

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let descriptor_num = descendant
        .get("FontDescriptor")
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("font descriptor");
    let descriptor = doc
        .get_object(descriptor_num)
        .expect("descriptor lookup")
        .expect("descriptor object")
        .value
        .as_dict()
        .expect("descriptor dict")
        .clone();

    let style = descriptor
        .get("Style")
        .and_then(Value::as_dict)
        .expect("/Style");
    assert_eq!(
        style.get("Panose"),
        Some(&Value::HexString("020B05020300000000000000".into())),
        "/Style /Panose 应写成 12 字节十六进制"
    );
    assert_eq!(
        descriptor.get("Lang").and_then(Value::as_name),
        Some("zh-CN"),
        "/Lang 应写成名称对象"
    );
    let fd = descriptor.get("FD").and_then(Value::as_dict).expect("/FD");
    let proportional = fd
        .get("Proportional")
        .and_then(Value::as_dict)
        .expect("Proportional 覆盖");
    assert_eq!(
        numbers(proportional.get("FontBBox").expect("FontBBox")),
        vec![0.0, -250.0, 1000.0, 900.0]
    );
    assert_eq!(scalar(proportional.get("Ascent")), Some(750.0));
    assert!(proportional.get("FD").is_none(), "字形类覆盖不应再嵌套 /FD");
    assert!(
        proportional.get("Type").and_then(Value::as_name) == Some("FontDescriptor"),
        "覆盖项应保留 FontDescriptor 类型"
    );
}
