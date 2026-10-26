//! Form XObject 写端测试：字典字段、资源引用与非法参数。

use super::*;
use crate::core::content::ContentStream;
use crate::core::types::PdfVersion;

fn rect_form() -> FormXObject {
    let mut content = ContentStream::new();
    content
        .save_state()
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 100.0, 50.0)
        .fill()
        .restore_state();
    FormXObject::new([0.0, 0.0, 100.0, 50.0], content)
}

#[test]
fn form_xobject_writes_dictionary_and_stream() {
    let form = rect_form().with_matrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&form);
    b.draw_form(p, handle, [2.0, 0.0, 0.0, 2.0, 30.0, 40.0]);
    let s = render(&mut b);

    assert!(s.contains("/Subtype /Form"), "{s}");
    assert!(s.contains("/FormType 1"), "{s}");
    assert!(s.contains("/BBox [0.0 0.0 100.0 50.0]"), "{s}");
    assert!(s.contains("/Matrix [1.0 0.0 0.0 1.0 10.0 20.0]"), "{s}");
    assert!(s.contains("0.5 g"), "表单内容应写入流: {s}");
    assert!(s.contains("0.0 0.0 100.0 50.0 re"), "{s}");
    assert!(s.contains("2.0 0.0 0.0 2.0 30.0 40.0 cm"), "{s}");
    assert!(s.contains("/Fm1 Do"), "{s}");
}

#[test]
fn form_without_matrix_omits_matrix_entry() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form());
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let s = render(&mut b);

    assert!(s.contains("/Subtype /Form"), "{s}");
    assert!(!s.contains("/Matrix"), "未提供矩阵时不应写 /Matrix: {s}");
}

#[test]
fn repeated_draws_reuse_one_form_resource_and_object() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form());
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 100.0, 0.0]);
    let s = render(&mut b);

    assert_eq!(s.matches("/Subtype /Form").count(), 1, "{s}");
    assert_eq!(s.matches("/Fm1 Do").count(), 2, "{s}");
    // 一处资源引用 + 两处 Do 绘制。
    assert_eq!(s.matches("/Fm1 ").count(), 3, "{s}");
}

#[test]
fn form_drawn_on_two_pages_shares_one_object() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let p1 = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form());
    b.draw_form(p0, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    b.draw_form(p1, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let s = render(&mut b);

    assert_eq!(s.matches("/Subtype /Form").count(), 1, "{s}");
    assert_eq!(s.matches("/Fm1 Do").count(), 2, "{s}");
    assert_eq!(s.matches("/Fm1 ").count(), 4, "{s}");
}

#[test]
fn unused_form_object_is_not_written() {
    let mut b = PdfBuilder::new();
    let _ = b.page(PageSize::A4);
    let _ = b.form_xobject(&rect_form());
    let s = render(&mut b);
    assert!(!s.contains("/Subtype /Form"), "{s}");
}

fn assert_form_rejected(form: FormXObject, expected: &str) {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&form);
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法表单参数应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn group_xobject_writes_transparency_dictionary() {
    let group = GroupXObject::transparency()
        .with_color_space(ColorSpace::DeviceRGB)
        .with_isolated(true)
        .with_knockout(true);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form().with_group(group));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let s = render(&mut b);

    assert!(s.contains("/Group <<"), "{s}");
    assert!(s.contains("/Type /Group"), "{s}");
    assert!(s.contains("/S /Transparency"), "{s}");
    assert!(s.contains("/CS /DeviceRGB"), "{s}");
    assert!(s.contains("/I true"), "{s}");
    assert!(s.contains("/K true"), "{s}");
}

#[test]
fn group_without_options_omits_optional_entries() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form().with_group(GroupXObject::transparency()));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let s = render(&mut b);

    assert!(s.contains("/S /Transparency"), "{s}");
    assert!(!s.contains("/CS "), "未指定组颜色空间时不应写 /CS: {s}");
    assert!(!s.contains("/I true"), "非隔离组不应写 /I: {s}");
    assert!(!s.contains("/K true"), "非挖空组不应写 /K: {s}");
}

#[test]
fn group_color_space_accepts_cie_based_array() {
    let group = GroupXObject::transparency()
        .with_color_space(ColorSpace::CalRGB(CalRGB::new([0.9505, 1.0, 1.089])));
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form().with_group(group));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let s = render(&mut b);

    assert!(s.contains("/CS [/CalRGB <<"), "{s}");
    assert!(s.contains("/WhitePoint [0.9505 1.0 1.089]"), "{s}");
}

#[test]
fn invalid_group_color_space_fails_the_build() {
    assert_form_rejected(
        rect_form().with_group(
            GroupXObject::transparency().with_color_space(ColorSpace::Indexed(Indexed::new(
                ColorSpace::DeviceRGB,
                0,
                vec![0, 0, 0],
            ))),
        ),
        "form XObject: group: group color space must not be Indexed",
    );
    assert_form_rejected(
        rect_form().with_group(
            GroupXObject::transparency().with_color_space(ColorSpace::CalRGB(CalRGB::new([
                f64::NAN,
                1.0,
                1.0,
            ]))),
        ),
        "form XObject: group: group color space:",
    );
}

#[test]
fn group_xobject_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form().with_group(GroupXObject::transparency()));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject groups");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("transparency"),
        "错误信息应说明透明度特性: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&rect_form().with_group(GroupXObject::transparency()));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports groups");
    assert!(String::from_utf8_lossy(&out).contains("/S /Transparency"));
}

#[test]
fn invalid_form_bbox_and_matrix_fail_the_build() {
    assert_form_rejected(
        FormXObject::new([f64::NAN, 0.0, 1.0, 1.0], ContentStream::new()),
        "form XObject: BBox must contain 4 finite numbers",
    );
    assert_form_rejected(
        FormXObject::new([10.0, 0.0, 5.0, 1.0], ContentStream::new()),
        "form XObject: BBox minimum must not exceed maximum",
    );
    assert_form_rejected(
        FormXObject::new([0.0, 0.0, 1.0, 1.0], ContentStream::new()).with_matrix([
            1.0,
            0.0,
            0.0,
            1.0,
            f64::INFINITY,
            0.0,
        ]),
        "form XObject: Matrix must contain 6 finite numbers",
    );
}
