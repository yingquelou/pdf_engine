//! TrueType 字形子集化：`with_subset_glyphs` 重写 `/FontFile2`（更短的子集
//! 程序、`/Length` 与 `Length1` 一致、重编号后的 cmap）与非 TrueType 程序、
//! 缺失程序、越界 gid 的 `InvalidObject` 零字节拒绝，以及不同字形列表
//! 不做字体去重。

use super::super::*;
use crate::core::reader::font::TrueTypeFont;
use crate::core::types::Value;
#[test]
fn subset_glyphs_rewrite_truetype_fontfile2() {
    let program = sample_truetype_program();
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(program.clone()))
        .with_subset_glyphs(&[2]);
    assert!(font.subset, "with_subset_glyphs 应同时打开子集标记");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "TrueType");
    let (declared_length, bytes) = embedded_program(&doc, &font_dict, "FontFile2");
    let (stream_dict, _) = embedded_program_stream(&doc, &font_dict, "FontFile2");
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "子集程序的 Length1 应与重写后字节一致"
    );
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

    let parsed = TrueTypeFont::parse(bytes).expect("子集程序应可解析");
    assert_eq!(parsed.glyph_count(), 2, "应只保留 gid 0 与 2");
    assert_eq!(
        parsed.glyph_id_for_unicode(0x41).expect("cmap lookup"),
        Some(1),
        "U+0041 应重编号为 gid 1"
    );

    // BaseFont 前缀仍由原始程序决定（确定性），与单独计算一致。
    let base_font = font_dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .expect("BaseFont");
    let expected = format!(
        "{}+Helvetica",
        FontProgram::TrueType(program).subset_tag_for_name("Helvetica", &[1, 0, 2])
    );
    assert_eq!(base_font, expected);
}

#[test]
fn subset_glyphs_reject_non_truetype_programs() {
    for (program, kind) in [
        (
            FontProgram::Type1(b"%!PS-AdobeFont-1.0: TestFont".to_vec()),
            "Type1",
        ),
        (FontProgram::CFF(vec![0x01, 0x00, 0x04, 0x05]), "CFF"),
    ] {
        let font = Font::helvetica(12.0)
            .with_font_program(program)
            .with_subset_glyphs(&[1]);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "Test");

        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("只有 TrueType 程序可以按字形子集化");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains(kind) && error.to_string().contains("subset_glyphs"),
            "错误信息应说明所需程序类型: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn subset_glyphs_without_font_program_is_invalid() {
    let font = Font::helvetica(12.0).with_subset_glyphs(&[1]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("缺少字体程序时不能子集化");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("subset_glyphs"), "{error}");
    assert!(out.is_empty());
}

#[test]
fn subset_glyphs_invalid_glyph_id_is_invalid_object() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sample_truetype_program()))
        .with_subset_glyphs(&[99]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("越界 gid 应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("TrueType subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(out.is_empty());
}

#[test]
fn subset_glyph_lists_do_not_deduplicate_fonts() {
    let program = FontProgram::TrueType(sample_truetype_program());
    let wide = Font::helvetica(12.0)
        .with_font_program(program.clone())
        .with_subset_glyphs(&[0, 2]);
    let narrow = Font::helvetica(12.0)
        .with_font_program(program)
        .with_subset_glyphs(&[2]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &wide, 10.0, 10.0, "A");
    b.text(p, &narrow, 10.0, 20.0, "A");
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
    assert_eq!(font_count, 2, "不同字形列表应生成两个字体对象");

    let base_fonts: Vec<String> = (1..=64)
        .filter_map(|num| doc.get_object(num).ok().flatten())
        .filter_map(|object| {
            let dict = object.value.as_dict()?;
            if dict.get("Type").and_then(Value::as_name) != Some("Font") {
                return None;
            }
            dict.get("BaseFont")
                .and_then(Value::as_name)
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(base_fonts.len(), 2);
    assert_ne!(
        base_fonts[0], base_fonts[1],
        "不同子集不应共享同一 /BaseFont 标签"
    );
}
