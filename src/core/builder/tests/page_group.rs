//! 页组（页字典 `/Group`，PDF 1.4）写端测试。

use super::*;
use crate::core::types::PdfVersion;

#[test]
fn page_group_writes_transparency_dictionary() {
    let group = GroupXObject::transparency()
        .with_color_space(ColorSpace::DeviceRGB)
        .with_isolated(true);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Page");
    b.set_page_group(p, &group);
    let s = render(&mut b);

    assert!(s.contains("/Type /Page"), "{s}");
    assert!(s.contains("/Group <<"), "{s}");
    assert!(s.contains("/Type /Group"), "{s}");
    assert!(s.contains("/S /Transparency"), "{s}");
    assert!(s.contains("/CS /DeviceRGB"), "{s}");
    assert!(s.contains("/I true"), "{s}");
    assert!(!s.contains("/K true"), "未设置挖空时不应写 /K: {s}");
}

#[test]
fn page_without_group_omits_group_entry() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Plain");
    let s = render(&mut b);

    assert!(s.contains("/Type /Page"), "{s}");
    assert!(!s.contains("/Group <<"), "未设置页组时不应写 /Group: {s}");
}

#[test]
fn page_group_writes_cie_based_color_space() {
    let group = GroupXObject::transparency()
        .with_color_space(ColorSpace::CalRGB(CalRGB::new([0.9505, 1.0, 1.089])));
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_page_group(p, &group);
    let s = render(&mut b);

    assert!(s.contains("/Group <<"), "{s}");
    assert!(s.contains("/CS [/CalRGB <<"), "{s}");
    assert!(s.contains("/WhitePoint [0.9505 1.0 1.089]"), "{s}");
}

#[test]
fn page_group_only_applies_to_target_page() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let p1 = b.page(PageSize::A4);
    b.text(p0, &Font::helvetica(12.0), 72.0, 700.0, "One");
    b.text(p1, &Font::helvetica(12.0), 72.0, 700.0, "Two");
    b.set_page_group(p1, &GroupXObject::transparency());
    let s = render(&mut b);

    assert_eq!(
        s.matches("/Group <<").count(),
        1,
        "只有目标页应带 /Group: {s}"
    );
}

fn assert_page_group_rejected(group: GroupXObject, expected: &str) {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_page_group(p, &group);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法页组参数应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn invalid_page_group_color_space_fails_the_build() {
    assert_page_group_rejected(
        GroupXObject::transparency().with_color_space(ColorSpace::Indexed(Indexed::new(
            ColorSpace::DeviceRGB,
            0,
            vec![0, 0, 0],
        ))),
        "page group: group color space must not be Indexed",
    );
    assert_page_group_rejected(
        GroupXObject::transparency().with_color_space(ColorSpace::CalRGB(CalRGB::new([
            f64::NAN,
            1.0,
            1.0,
        ]))),
        "page group: group color space:",
    );
    assert_page_group_rejected(
        GroupXObject::transparency().with_color_space(ColorSpace::Separation(Separation::new(
            "Orange",
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        ))),
        "page group: group color space must be a device or CIE-based color space",
    );
}

#[test]
fn page_group_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.set_page_group(p, &GroupXObject::transparency());
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.3 must reject page groups");
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
    b.set_page_group(p, &GroupXObject::transparency());
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports page groups");
    assert!(String::from_utf8_lossy(&out).contains("/S /Transparency"));
}
