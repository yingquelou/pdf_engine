//! 页级命名颜色空间（`/Resources /ColorSpace` 与 `CS`/`cs`/`SC`/`sc`）测试。

use super::*;

fn cal_rgb() -> ColorSpace {
    let mut cal = CalRGB::new([0.9505, 1.0, 1.089]);
    cal.gamma = Some([2.2, 2.2, 2.2]);
    ColorSpace::CalRGB(cal)
}

#[test]
fn named_color_spaces_write_resources_and_switch_operators() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let cal = b.color_space(&cal_rgb());
    let lab = b.color_space(&ColorSpace::Lab(Lab::new([0.9505, 1.0, 1.089])));
    // 先引用后注册的句柄：/CS1 对应 Lab，/CS2 对应 CalRGB。
    b.set_fill_color_space(p, lab);
    b.set_stroke_color_space(p, cal);
    b.set_fill_color_components(p, &[0.1, 0.2, 0.3]);
    b.set_stroke_color_components(p, &[0.4, 0.5, 0.6]);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace <<"), "{s}");
    assert!(s.contains("/CalRGB <<"), "{s}");
    assert!(s.contains("/WhitePoint [0.9505 1.0 1.089]"), "{s}");
    assert!(s.contains("/Gamma [2.2 2.2 2.2]"), "{s}");
    assert!(s.contains("/Lab <<"), "{s}");
    assert!(s.contains("/CS1 cs\n"), "{s}");
    assert!(s.contains("/CS2 CS\n"), "{s}");
    assert!(s.contains("0.1 0.2 0.3 sc\n"), "{s}");
    assert!(s.contains("0.4 0.5 0.6 SC\n"), "{s}");
}

#[test]
fn named_icc_profile_is_shared_across_pages() {
    let mut b = PdfBuilder::new();
    let p1 = b.page(PageSize::A4);
    let p2 = b.page(PageSize::A4);
    let icc = b.color_space(&ColorSpace::ICCBased(ICCBased::new(3, vec![1, 2, 3])));
    b.set_fill_color_space(p1, icc);
    b.set_fill_color_space(p2, icc);
    let s = render(&mut b);

    assert_eq!(s.matches("/ICCBased").count(), 2, "每页各引用一次: {s}");
    assert_eq!(s.matches("/N 3").count(), 1, "配置文件流只写一次: {s}");
    assert!(s.contains("/CS1 cs\n"), "{s}");
}

#[test]
fn unused_color_space_is_not_written() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Plain");
    b.color_space(&ColorSpace::CalGray(CalGray::new([0.9505, 1.0, 1.089])));
    let s = render(&mut b);

    assert!(!s.contains("/ColorSpace <<"), "{s}");
    assert!(!s.contains("/CalGray"), "{s}");
    assert!(!s.contains(" CS\n"), "{s}");
}

#[test]
fn unknown_color_space_handles_and_pages_are_ignored() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_fill_color_space(p, 99);
    b.set_stroke_color_space(7, 0);
    b.set_stroke_color_components(7, &[0.5]);
    let s = render(&mut b);

    assert!(!s.contains("/ColorSpace <<"), "{s}");
    assert!(!s.contains(" cs\n"), "{s}");
    assert!(!s.contains(" SC\n"), "{s}");
}

#[test]
fn invalid_named_color_space_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let icc = b.color_space(&ColorSpace::ICCBased(ICCBased::new(3, Vec::new())));
    b.set_fill_color_space(p, icc);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("空 ICC 配置文件应被拒绝");

    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("color space"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn named_separation_writes_tint_function_object() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let separation = b.color_space(&ColorSpace::Separation(Separation::new(
        "Spot",
        ColorSpace::DeviceGray,
        Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
    )));
    b.set_fill_color_space(p, separation);
    b.set_fill_color_components(p, &[0.5]);
    let s = render(&mut b);

    assert!(s.contains("/Separation /Spot /DeviceGray"), "{s}");
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(s.contains("/CS1 cs\n"), "{s}");
    assert!(s.contains("0.5 sc\n"), "{s}");
}
