//! name-keyed CFF（`/FontFile3` + `/Type1C`）子集写入测试。

use super::*;
use crate::core::reader::font::RawCffFont;
use crate::core::reader::font::fixtures;
use crate::core::types::Value;

#[test]
fn cff_subset_glyphs_rewrite_fontfile3() {
    let program = fixtures::name_keyed_cff_custom_charset(0);
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::CFF(program.clone()))
        .with_subset_cff_glyphs(&["Beta"]);
    assert!(font.subset, "with_subset_cff_glyphs 应同时打开子集标记");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "B");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type1");
    let (declared_length, bytes) = embedded_program(&doc, &font_dict, "FontFile3");
    assert_ne!(bytes, program, "嵌入程序应被重写为子集");
    assert!(
        bytes.len() < program.len(),
        "子集程序应更短: {} >= {}",
        bytes.len(),
        program.len()
    );
    assert_eq!(
        declared_length,
        bytes.len() as i64,
        "/Length 应与子集字节一致"
    );

    let parsed = RawCffFont::parse(bytes).expect("子集程序应可解析");
    assert!(!parsed.is_cid_keyed(), "name-keyed CFF 仍是 name-keyed");
    assert_eq!(parsed.glyph_count(), 2, "应只保留 .notdef 与 Beta");
    assert_eq!(parsed.glyph_width(1), Some(610.0));

    // BaseFont 前缀仍由原始程序决定（确定性），与单独计算一致。
    let base_font = font_dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .expect("BaseFont");
    let expected = format!(
        "{}+Helvetica",
        FontProgram::CFF(program).subset_tag_with(&[3, 0, 0, 0, 4, b'B', b'e', b't', b'a'])
    );
    assert_eq!(base_font, expected);
}

#[test]
fn cff_subset_glyphs_require_cff_program() {
    for (program, kind) in [
        (FontProgram::Type1(sample_type1_program()), "Type 1"),
        (FontProgram::TrueType(sample_truetype_program()), "TrueType"),
    ] {
        let font = Font::helvetica(12.0)
            .with_font_program(program)
            .with_subset_cff_glyphs(&["Beta"]);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "B");

        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("只有 CFF 程序可以按字形名子集化");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(
            error.to_string().contains("subset_cff_glyphs") && error.to_string().contains(kind),
            "错误信息应说明所需程序类型: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    let font = Font::helvetica(12.0).with_subset_cff_glyphs(&["Beta"]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "B");
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("缺少字体程序时不能按 CFF 字形子集化");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("requires an embedded CFF font program"),
        "{error}"
    );
}

#[test]
fn cff_subset_glyphs_reject_builtin_encoding() {
    let font = Font::helvetica(12.0)
        .with_encoding(FontEncoding::BuiltIn)
        .with_font_program(FontProgram::CFF(fixtures::name_keyed_cff_custom_charset(0)))
        .with_subset_cff_glyphs(&["Beta"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "B");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CFF 子集化会移除内建 /Encoding，不能与 BuiltIn 同用");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("subset_cff_glyphs"),
        "错误信息应指出 subset_cff_glyphs: {error}"
    );
    assert!(
        error.to_string().contains("BuiltIn"),
        "错误信息应指出 BuiltIn 编码: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn cff_subset_glyphs_unknown_glyph_is_invalid_object() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::CFF(fixtures::name_keyed_cff_custom_charset(0)))
        .with_subset_cff_glyphs(&["NoSuchGlyph"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "N");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未知字形名应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("CFF subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(error.to_string().contains("NoSuchGlyph"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn cff_subset_glyphs_cannot_be_combined_with_other_subset_lists() {
    let program = FontProgram::CFF(fixtures::name_keyed_cff_custom_charset(0));
    let mut fonts = vec![
        Font::helvetica(12.0)
            .with_font_program(program.clone())
            .with_subset_glyphs(&[1])
            .with_subset_cff_glyphs(&["Beta"]),
        Font::helvetica(12.0)
            .with_font_program(program)
            .with_subset_type1_glyphs(&["Beta"])
            .with_subset_cff_glyphs(&["Beta"]),
    ];
    // 三种列表同时设置同样互斥。
    fonts.push(
        Font::helvetica(12.0)
            .with_font_program(FontProgram::CFF(fixtures::name_keyed_cff_custom_charset(0)))
            .with_subset_glyphs(&[1])
            .with_subset_type1_glyphs(&["Beta"])
            .with_subset_cff_glyphs(&["Beta"]),
    );

    for font in fonts {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "B");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("子集字形列表不能同时使用");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains("cannot be combined"), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn cff_subset_glyph_lists_do_not_deduplicate_fonts() {
    let program = FontProgram::CFF(fixtures::name_keyed_cff_custom_charset(0));
    let wide = Font::helvetica(12.0)
        .with_font_program(program.clone())
        .with_subset_cff_glyphs(&["Alpha", "Beta"]);
    let narrow = Font::helvetica(12.0)
        .with_font_program(program)
        .with_subset_cff_glyphs(&["Beta"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &wide, 10.0, 10.0, "AB");
    b.text(p, &narrow, 10.0, 20.0, "B");
    let doc = b.assemble_document().expect("assemble");

    let font_count = (1..=64)
        .filter(|num| {
            doc.get_object(*num).ok().flatten().is_some_and(|object| {
                object
                    .value
                    .as_dict()
                    .and_then(|dict| dict.get("Type"))
                    .and_then(Value::as_name)
                    == Some("Font")
            })
        })
        .count();
    assert_eq!(font_count, 2, "不同 CFF 字形列表应生成两个字体对象");
}
