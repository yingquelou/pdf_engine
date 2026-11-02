//! CIDFont 程序嵌入：Type2 `/FontFile2`、Type0 `/FontFile3` 与 OpenType 轮廓门禁。

use super::super::*;

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
fn cid_font_type2_fontfile2_declares_length1() {
    let program = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(program));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &descendant, "FontFile2");
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "Length1 应为 TrueType 程序字节数"
    );
    assert!(stream_dict.get("Length2").is_none());
    assert!(stream_dict.get("Length3").is_none());
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
fn opentype_cff_program_writes_opentype_fontfile3_for_cid_type0() {
    let program = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[7]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType0");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &descendant, "FontFile3");
    assert_eq!(
        stream_dict.get("Subtype").and_then(Value::as_name),
        Some("OpenType"),
        "CIDFontType0 的 CFF-flavored OpenType 应写 /Subtype /OpenType"
    );
    assert_eq!(bytes, program);
    for key in ["Length1", "Length2", "Length3"] {
        assert!(
            stream_dict.get(key).is_none(),
            "FontFile3 不应写 {key}: {stream_dict:?}"
        );
    }

    let (_, type0) = find_font_object(&doc, "Type0");
    assert_eq!(
        type0.get("Encoding").and_then(Value::as_name),
        Some("Identity-H"),
        "Type 0 父字体保持默认 Identity-H 编码"
    );
}

#[test]
fn opentype_truetype_program_writes_fontfile2_for_cid_type2() {
    let program = sample_truetype_program();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::OpenType(program.clone()))
    .with_cid_to_gid_map(&[0, 2]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &descendant, "FontFile2");
    assert_eq!(bytes, program);
    assert_eq!(
        stream_dict.get("Length").and_then(Value::as_integer),
        Some(bytes.len() as i64)
    );
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "FontFile2 应写 Table 127 的 Length1"
    );
    assert!(descendant.get("CIDToGIDMap").is_some());
}

#[test]
fn opentype_cid_font_rejects_mismatched_flavor() {
    let cff_flavored = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let truetype_flavored = sample_truetype_program();
    let cases = [
        (
            CidFont::new(
                "ExampleCIDFont",
                CidFontSubtype::CIDFontType0,
                CidSystemInfo::new("Adobe", "Identity", 0),
                FontDescriptor::new(),
                12.0,
            )
            .with_font_program(FontProgram::OpenType(truetype_flavored)),
            "CFF-flavored",
        ),
        (
            CidFont::new(
                "ExampleCIDFont",
                CidFontSubtype::CIDFontType2,
                CidSystemInfo::new("Adobe", "Identity", 0),
                FontDescriptor::new(),
                12.0,
            )
            .with_font_program(FontProgram::OpenType(cff_flavored)),
            "TrueType-flavored",
        ),
    ];
    for (cid_font, expected) in cases {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, &cid_font);
        b.text_cid(p, idx, 10.0, 10.0, &[1]);

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("子类型与 OpenType 轮廓不匹配");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}
