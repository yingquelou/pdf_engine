//! Type 1 字形名子集化：`with_subset_type1_glyphs` 重写 `/FontFile`（Length1/
//! 2/3 覆盖整流、保留字形的 CharString 逐字不变）、缺失程序/非 Type 1 程序/
//! 未知字形名的 `InvalidObject` 零字节拒绝、与 `subset_glyphs` 互斥，
//! 以及不同字形列表不做字体去重。

use super::super::*;
use crate::core::reader::font::Type1FontProgram;
use crate::core::types::Value;
#[test]
fn type1_subset_glyphs_rewrite_fontfile() {
    let program = sample_type1_program();
    let original = Type1FontProgram::parse(&program).expect("fixture should parse");
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::Type1(program.clone()))
        .with_subset_type1_glyphs(&["A"]);
    assert!(font.subset, "with_subset_type1_glyphs 应同时打开子集标记");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type1");
    let (declared_length, bytes) = embedded_program(&doc, &font_dict, "FontFile");
    let (stream_dict, _) = embedded_program_stream(&doc, &font_dict, "FontFile");
    let length_sum: i64 = ["Length1", "Length2", "Length3"]
        .iter()
        .map(|key| {
            stream_dict
                .get(key)
                .and_then(Value::as_integer)
                .unwrap_or_else(|| panic!("missing {key}"))
        })
        .sum();
    assert_eq!(
        length_sum,
        bytes.len() as i64,
        "子集程序的 Length1/2/3 应覆盖整个流"
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

    let parsed = doc
        .type1_program(&Value::Dict(font_dict.clone()))
        .expect("子集程序应可解析")
        .expect("应含 FontFile 程序");
    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "A"], "应只保留 .notdef 与请求字形");
    for name in [".notdef", "A"] {
        assert_eq!(
            parsed.charstrings.get(name),
            original.charstrings.get(name),
            "保留字形 {name} 的 CharString 不应改变"
        );
    }

    // BaseFont 前缀仍由原始程序决定（确定性），与单独计算一致。
    let base_font = font_dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .expect("BaseFont");
    let expected = format!(
        "{}+Helvetica",
        FontProgram::Type1(program).subset_tag_for_name("Helvetica", &[2, 0, 0, 0, 1, b'A'])
    );
    assert_eq!(base_font, expected);
}

#[test]
fn type1_subset_glyphs_without_font_program_is_invalid() {
    let font = Font::helvetica(12.0).with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("缺少字体程序时不能按 Type 1 字形子集化");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("requires an embedded Type 1 font program"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type1_subset_glyphs_reject_non_type1_programs() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sample_truetype_program()))
        .with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("TrueType 程序不能按 Type 1 字形名子集化");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("requires an embedded Type 1 font program"),
        "{error}"
    );
    assert!(error.to_string().contains("TrueType"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type1_subset_glyphs_unknown_glyph_is_invalid_object() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::Type1(sample_type1_program()))
        .with_subset_type1_glyphs(&["Z"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未知字形名应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("Type 1 subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(error.to_string().contains('Z'), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn subset_glyphs_and_type1_subset_glyphs_cannot_be_combined() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::Type1(sample_type1_program()))
        .with_subset_glyphs(&[1])
        .with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("两种子集字形列表不能同时使用");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("cannot be combined"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type1_subset_glyph_lists_do_not_deduplicate_fonts() {
    let program = FontProgram::Type1(sample_type1_program());
    let wide = Font::helvetica(12.0)
        .with_font_program(program.clone())
        .with_subset_type1_glyphs(&["A", "B"]);
    let narrow = Font::helvetica(12.0)
        .with_font_program(program)
        .with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &wide, 10.0, 10.0, "AB");
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
    assert_eq!(font_count, 2, "不同 Type 1 字形列表应生成两个字体对象");
}
