//! CID 内容流编码写出：Identity-H 双字节字符码、`/ToUnicode` 省略与 CIDToGIDMap 门禁。

use super::super::*;

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

#[test]
fn cid_font_type2_writes_cid_to_gid_map_stream() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x0A,
    ]))
    .with_cid_to_gid_map(&[0, 5, 9]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let map_num = descendant
        .get("CIDToGIDMap")
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("CIDToGIDMap reference");
    let map = doc
        .get_object(map_num)
        .expect("CIDToGIDMap lookup")
        .expect("CIDToGIDMap object");
    match map.value {
        Value::Stream(ref stream_dict, StreamData::Owned(ref bytes)) => {
            assert_eq!(
                stream_dict.get("Length").and_then(Value::as_integer),
                Some(6),
                "/Length 应为 3 个 gid 的 2 字节数组"
            );
            assert_eq!(bytes, &[0, 0, 0, 5, 0, 9], "gid 应按大端序写出");
        }
        _ => panic!("CIDToGIDMap 应为 Owned 流"),
    }
}

#[test]
fn cid_to_gid_map_requires_cidfonttype2() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_cid_to_gid_map(&[0, 1]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CIDFontType0 不应写 CIDToGIDMap");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("cid_to_gid"), "{error}");
    assert!(out.is_empty());
}

#[test]
fn cid_to_gid_map_requires_embedded_truetype_program() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_cid_to_gid_map(&[0, 1]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("未嵌入 TrueType 程序时 CIDToGIDMap 不可解释");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("embedded TrueType"),
        "错误信息应说明需要嵌入 TrueType 程序: {error}"
    );
    assert!(out.is_empty());
}
