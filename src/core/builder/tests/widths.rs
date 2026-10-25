//! 简单字体 `/Widths`（Table 111）与描述符必需项兜底（Table 122）测试。

use super::*;
use crate::core::types::Value;

/// 断言构建失败且未写出任何字节，返回错误。
fn build_error(mut b: PdfBuilder) -> WriteError {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("构建应失败");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
    error
}

#[test]
fn simple_font_writes_first_last_char_and_widths() {
    let font = Font::helvetica(12.0).with_widths(65, &[500.0, 600.5, 700.0]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "AB");

    let doc = b.assemble_document().expect("assemble");
    let (_, font_dict) = find_font_object(&doc, "Type1");
    assert_eq!(
        font_dict.get("FirstChar").and_then(Value::as_integer),
        Some(65)
    );
    assert_eq!(
        font_dict.get("LastChar").and_then(Value::as_integer),
        Some(67)
    );
    assert_eq!(
        font_dict.get("Widths").map(numbers),
        Some(vec![500.0, 600.5, 700.0])
    );

    // 浮点用 writer 的统一格式（至少一位小数）。
    let s = render(&mut b);
    assert!(s.contains("/FirstChar 65"), "{s}");
    assert!(s.contains("/LastChar 67"), "{s}");
    assert!(s.contains("/Widths [500.0 600.5 700.0]"), "{s}");
}

#[test]
fn widths_empty_list_is_invalid_object() {
    let font = Font::helvetica(12.0).with_widths(65, &[]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let error = build_error(b);
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("must not be empty"), "{error}");
    assert!(error.to_string().contains("with_widths"), "{error}");
}

#[test]
fn widths_beyond_single_byte_range_are_invalid_object() {
    // 255 + 2 个宽度覆盖字符码 255..=256，超出单字节码空间。
    let font = Font::helvetica(12.0).with_widths(255, &[500.0, 600.0]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let error = build_error(b);
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("255..=256"),
        "错误信息应给出越界区间: {error}"
    );
}

#[test]
fn widths_negative_nan_or_infinite_are_invalid_object() {
    for (widths, bad_code) in [
        (vec![500.0, -1.0], 66),
        (vec![f64::NAN], 65),
        (vec![f64::INFINITY], 65),
        (vec![f64::NEG_INFINITY], 65),
    ] {
        let font = Font::helvetica(12.0).with_widths(65, &widths);
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let error = build_error(b);
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(
            error
                .to_string()
                .contains(&format!("character code {bad_code}")),
            "错误信息应指出非法宽度的字符码: {error}"
        );
    }
}

#[test]
fn embedded_non_standard_font_without_widths_is_invalid_object() {
    let mut font =
        Font::helvetica(12.0).with_font_program(FontProgram::Type1(sample_type1_program()));
    font.name = "BuilderSans".to_string();

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let error = build_error(b);
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("requires explicit /Widths"),
        "错误信息应说明缺少 /Widths: {error}"
    );
    assert!(
        error.to_string().contains("with_widths"),
        "错误信息应提示 with_widths: {error}"
    );

    // 补上 widths 后即可构建，且描述符 FontName 默认取 BaseFont。
    let font = font.with_widths(65, &[500.0]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");
    let (_, font_dict) = find_font_object(&doc, "Type1");
    let descriptor = font_descriptor(&doc, &font_dict);
    assert_eq!(
        descriptor.get("FontName").and_then(Value::as_name),
        Some("BuilderSans")
    );
}

#[test]
fn embedded_standard_14_font_without_widths_still_builds() {
    let font = Font::helvetica(12.0).with_font_program(FontProgram::Type1(sample_type1_program()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let s = render(&mut b);

    assert!(
        !s.contains("/FirstChar"),
        "标准 14 字体未提供 widths 时不应写 /FirstChar: {s}"
    );
    assert!(!s.contains("/Widths"), "不应写 /Widths: {s}");
}

#[test]
fn font_descriptor_defaults_fill_table_122_required_entries() {
    let font = Font::helvetica(12.0).with_descriptor(FontDescriptor::new());

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");

    let doc = b.assemble_document().expect("assemble");
    let (_, font_dict) = find_font_object(&doc, "Type1");
    let descriptor = font_descriptor(&doc, &font_dict);

    assert_eq!(
        descriptor.get("FontName").and_then(Value::as_name),
        Some("Helvetica")
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
fn font_descriptor_explicit_values_are_not_overridden() {
    let descriptor = FontDescriptor::new()
        .with_font_name("CustomName")
        .with_flags(32)
        .with_bbox([-20.0, -250.0, 900.0, 850.0])
        .with_italic_angle(-12.0)
        .with_ascent(750.0)
        .with_descent(-250.0)
        .with_cap_height(720.0)
        .with_stem_v(90.0);
    let font = Font::helvetica(12.0).with_descriptor(descriptor);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");

    let doc = b.assemble_document().expect("assemble");
    let (_, font_dict) = find_font_object(&doc, "Type1");
    let descriptor = font_descriptor(&doc, &font_dict);

    assert_eq!(
        descriptor.get("FontName").and_then(Value::as_name),
        Some("CustomName"),
        "显式 FontName 不应被默认值覆盖"
    );
    assert_eq!(
        descriptor.get("Flags").and_then(Value::as_integer),
        Some(32)
    );
    assert_eq!(
        descriptor.get("FontBBox").map(numbers),
        Some(vec![-20.0, -250.0, 900.0, 850.0])
    );
    assert_eq!(scalar(descriptor.get("ItalicAngle")), Some(-12.0));
    assert_eq!(scalar(descriptor.get("Ascent")), Some(750.0));
    assert_eq!(scalar(descriptor.get("Descent")), Some(-250.0));
    assert_eq!(scalar(descriptor.get("CapHeight")), Some(720.0));
    assert_eq!(scalar(descriptor.get("StemV")), Some(90.0));
}

#[test]
fn subset_embedded_font_defaults_font_name_to_tagged_base_font() {
    let mut font =
        Font::helvetica(12.0).with_font_program(FontProgram::Type1(sample_type1_program()));
    font.name = "BuilderSans".to_string();
    let font = font.subset().with_widths(65, &[500.0]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let doc = b.assemble_document().expect("assemble");
    let (_, font_dict) = find_font_object(&doc, "Type1");
    let base_font = font_dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .expect("BaseFont");
    assert!(base_font.ends_with("+BuilderSans"), "{base_font}");
    let descriptor = font_descriptor(&doc, &font_dict);
    assert_eq!(
        descriptor.get("FontName").and_then(Value::as_name),
        Some(base_font),
        "描述符 FontName 默认值应等于带子集标签的 BaseFont"
    );
}
