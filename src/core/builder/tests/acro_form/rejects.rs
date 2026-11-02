//! 构建期拒绝且零字节输出：非法字段树（Widget 带子、无 `/FT` 叶子、`/Q` 越界、
//! `/MaxLen`/`/Opt` 与字段类型错配、未知页）、`/AP` 外观句柄越界，以及默认字体
//! 资源的重复名与非法名。

use super::super::*;
use super::support::build_error;
use crate::core::writer::WriteError;

#[test]
fn invalid_form_trees_are_rejected_without_output() {
    let rect = [72.0, 700.0, 200.0, 720.0];
    let cases = [
        (
            "widget with children",
            FormField::text("x", rect).with_child(FormField::text("y", [0.0, 0.0, 10.0, 10.0])),
            0,
        ),
        ("leaf without /FT", FormField::widget(rect), 0),
        (
            "quadding out of range",
            FormField::text("x", rect).with_quadding(3),
            0,
        ),
        (
            "max length on choice",
            FormField::choice("x", rect).with_max_len(3),
            0,
        ),
        (
            "options on text",
            FormField::text("x", rect).with_option(FormOption::new("A")),
            0,
        ),
        ("unknown page", FormField::text("x", rect), 7),
    ];
    for (label, field, page) in cases {
        let error = build_error(field, page);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{label}: unexpected error {error}"
        );
    }
}

#[test]
fn widget_appearance_handle_out_of_range_is_rejected() {
    let error = build_error(
        FormField::checkbox("x", [0.0, 0.0, 10.0, 10.0]).with_appearance(9),
        0,
    );
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error: {error}"
    );
}

/// 表单级校验失败的辅助：仅声明 AcroForm 选项，字段树保持合法。
fn form_build_error(form: AcroForm) -> WriteError {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = form;
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("invalid form options must be rejected");
    assert!(out.is_empty(), "failed build must not write bytes");
    error
}

#[test]
fn acro_form_rejects_duplicate_default_font_names() {
    let form = AcroForm::new()
        .with_default_font("Helv", Font::helvetica(12.0))
        .with_default_font("Helv", Font::helvetica(14.0));
    let error = form_build_error(form);
    assert!(
        matches!(&error, WriteError::InvalidObject(message) if message.contains("duplicate")),
        "unexpected error: {error:?}"
    );
}

#[test]
fn acro_form_rejects_invalid_default_font_names() {
    for name in ["", "He/lv", "He lv", "He[lv]"] {
        let form = AcroForm::new().with_default_font(name, Font::helvetica(12.0));
        let error = form_build_error(form);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "name {name:?} must be rejected, got {error:?}"
        );
    }
}
