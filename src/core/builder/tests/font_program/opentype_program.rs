//! OpenType 程序嵌入：CFF 风味（`OTTO`）走 `/FontFile3` + `/Subtype /OpenType`
//! 且不写程序长度条目、TrueType 风味走 `/FontFile2` + `Length1`、`true` 风味
//! 同样接受；非法 SFNT 版本标签与三类子集请求按 `InvalidObject` 零字节拒绝。

use super::super::*;
use crate::core::types::Value;
#[test]
fn opentype_cff_program_writes_fontfile3_with_opentype_subtype() {
    let program = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    // CFF-flavored OpenType 的字体子类型为 Type1（ISO 32000-1 §9.6.2）。
    let (_, font_dict) = find_font_object(&doc, "Type1");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile3");
    assert_eq!(
        stream_dict.get("Subtype").and_then(Value::as_name),
        Some("OpenType"),
        "FontFile3 流应声明 /Subtype /OpenType"
    );
    assert_eq!(bytes, program, "嵌入程序应与输入字节一致");
    for key in ["Length1", "Length2", "Length3"] {
        assert!(
            stream_dict.get(key).is_none(),
            "FontFile3 不应写 {key}: {stream_dict:?}"
        );
    }
}

#[test]
fn opentype_truetype_program_writes_fontfile2_with_length1() {
    let program = sample_truetype_program(); // 版本标签 0x00010000
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "TrueType");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile2");
    assert_eq!(bytes, program);
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "FontFile2 应写 Table 127 的 Length1"
    );
    assert!(stream_dict.get("Length2").is_none());
    assert!(stream_dict.get("Length3").is_none());
}

#[test]
fn opentype_program_accepts_apple_true_flavor() {
    let program = sample_opentype_sfnt(b"true", b"sample-cff-table");
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "TrueType");
    let (_, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile2");
    assert_eq!(bytes, program);
}

#[test]
fn opentype_program_rejects_unsupported_sfnt_flavor() {
    for program in [
        sample_opentype_sfnt(b"OTF ", b"payload"),
        sample_opentype_sfnt(b"typ1", b"payload"),
        vec![0x00, 0x01, 0x00],
    ] {
        let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program));

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法 SFNT 版本标签应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(
            error.to_string().contains("unsupported SFNT flavor"),
            "{error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn opentype_program_rejects_subset_requests() {
    let program = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let cases = [
        (
            Font::helvetica(12.0)
                .with_font_program(FontProgram::OpenType(program.clone()))
                .with_subset_glyphs(&[1]),
            "subset_glyphs",
        ),
        (
            Font::helvetica(12.0)
                .with_font_program(FontProgram::OpenType(program.clone()))
                .with_subset_type1_glyphs(&["A"]),
            "subset_type1_glyphs",
        ),
        (
            Font::helvetica(12.0)
                .with_font_program(FontProgram::OpenType(program.clone()))
                .with_subset_cff_glyphs(&["A"]),
            "subset_cff_glyphs",
        ),
    ];
    for (font, expected) in cases {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("OpenType 程序不支持子集化");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(error.to_string().contains("OpenType"), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}
