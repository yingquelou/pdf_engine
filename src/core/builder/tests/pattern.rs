//! 平铺图案（PatternType 1）写端测试。

use super::*;
use crate::core::content::ContentStream;
use crate::core::types::{Array, Dict, PdfVersion, Value};

fn tile_content() -> ContentStream {
    let mut content = ContentStream::new();
    content.set_fill_gray(0.75).rect(0.0, 0.0, 4.0, 4.0).fill();
    content
}

fn tiling_pattern() -> TilingPattern {
    TilingPattern::new([0.0, 0.0, 8.0, 8.0], 8.0, 8.0, tile_content())
}

/// 无色图案：图块内容只有几何路径，颜色由填充时的 `scn` 分量给出。
fn uncolored_tiling_pattern() -> TilingPattern {
    let mut content = ContentStream::new();
    content.rect(0.0, 0.0, 4.0, 4.0).fill();
    TilingPattern::uncolored(
        [0.0, 0.0, 8.0, 8.0],
        8.0,
        8.0,
        ColorSpace::DeviceRGB,
        content,
    )
}

#[test]
fn tiling_pattern_writes_stream_dictionary_and_content() {
    let pattern = tiling_pattern()
        .with_tiling_type(TilingType::NoDistortion)
        .with_matrix([1.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.tiling_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [72.0, 500.0, 100.0, 100.0]);
    let s = render(&mut b);

    assert!(s.contains("/Pattern <<"), "{s}");
    assert!(s.contains("/P1 "), "{s}");
    assert!(s.contains("/Type /Pattern"), "{s}");
    assert!(s.contains("/PatternType 1"), "{s}");
    assert!(s.contains("/PaintType 1"), "{s}");
    assert!(s.contains("/TilingType 2"), "{s}");
    assert!(s.contains("/BBox [0.0 0.0 8.0 8.0]"), "{s}");
    assert!(s.contains("/XStep 8.0"), "{s}");
    assert!(s.contains("/YStep 8.0"), "{s}");
    assert!(s.contains("/Matrix [1.0 0.0 0.0 1.0 2.0 3.0]"), "{s}");
    assert!(s.contains("/Length "), "{s}");
    assert!(s.contains("0.75 g"), "图案流内容应原样写出: {s}");
    assert!(s.contains("0.0 0.0 4.0 4.0 re"), "{s}");
    assert!(s.contains("/Pattern cs"), "{s}");
    assert!(s.contains("/P1 scn"), "{s}");
    assert!(s.contains("72.0 500.0 100.0 100.0 re"), "{s}");
}

#[test]
fn tiling_pattern_omits_matrix_and_writes_empty_resources_by_default() {
    let pattern = tiling_pattern();
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.tiling_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [0.0, 0.0, 20.0, 20.0]);
    let s = render(&mut b);

    assert!(!s.contains("/Matrix"), "省略 Matrix 时不应写出: {s}");
    assert!(s.contains("/Resources << >>"), "省略资源时应写空字典: {s}");
    assert!(s.contains("/TilingType 1"), "默认平铺类型为 1: {s}");
    assert!(!s.contains("/ColorSpace"), "彩色图案不应写 ColorSpace: {s}");
}

#[test]
fn tiling_pattern_resources_are_written_verbatim() {
    let resources = Dict::new().insert(
        "ProcSet",
        Value::Array(Array::new().push(Value::Name("PDF".into()))),
    );
    let pattern = tiling_pattern().with_resources(resources);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.tiling_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [0.0, 0.0, 20.0, 20.0]);
    let s = render(&mut b);

    assert!(s.contains("/ProcSet [/PDF]"), "{s}");
    assert!(!s.contains("/Resources << >>"), "{s}");
}

#[test]
fn uncolored_tiling_pattern_writes_paint_type_and_color_space() {
    let pattern = uncolored_tiling_pattern();
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.tiling_pattern(&pattern);
    b.fill_rect_with_uncolored_pattern(p, h, &[0.2, 0.4, 0.6], [72.0, 500.0, 100.0, 80.0]);
    let s = render(&mut b);

    assert!(s.contains("/PaintType 2"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/PatternType 1"), "{s}");
    assert!(s.contains("/Pattern cs"), "{s}");
    assert!(s.contains("0.2 0.4 0.6 /P1 scn"), "{s}");
    assert!(s.contains("72.0 500.0 100.0 80.0 re"), "{s}");
    assert!(s.contains("0.0 0.0 4.0 4.0 re"), "图块内容应原样写出: {s}");
}

#[test]
fn uncolored_tiling_pattern_ignores_mismatched_components() {
    let pattern = uncolored_tiling_pattern();
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.tiling_pattern(&pattern);
    b.fill_rect_with_uncolored_pattern(p, h, &[0.2, 0.4], [0.0, 0.0, 10.0, 10.0]);
    b.fill_rect_with_uncolored_pattern(p, h, &[0.2, f64::NAN, 0.6], [0.0, 0.0, 10.0, 10.0]);
    let s = render(&mut b);

    assert!(
        !s.contains("/Pattern <<"),
        "分量非法时不应登记图案资源: {s}"
    );
    assert!(!s.contains("/PaintType 2"), "{s}");
    assert!(!s.contains(" scn"), "{s}");
}

#[test]
fn invalid_uncolored_tiling_pattern_fails_the_build() {
    fn assert_rejected(pattern: &TilingPattern, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.tiling_pattern(pattern);
        b.fill_rect_with_pattern(p, h, [0.0, 0.0, 10.0, 10.0]);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法无色图案应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    let mut pattern = uncolored_tiling_pattern();
    pattern.color_space = None;
    assert_rejected(&pattern, "requires a color space");

    let mut content = ContentStream::new();
    content.rect(0.0, 0.0, 4.0, 4.0).fill();
    let pattern = TilingPattern::uncolored(
        [0.0, 0.0, 8.0, 8.0],
        8.0,
        8.0,
        ColorSpace::CalGray(CalGray::new([f64::NAN, 1.0, 1.0])),
        content,
    );
    assert_rejected(&pattern, "uncolored tiling pattern color space");
}

#[test]
fn shading_and_tiling_patterns_share_page_pattern_resource_names() {
    let shading = Shading::Axial(AxialShading::new(
        ColorSpace::DeviceGray,
        [0.0, 0.0, 100.0, 0.0],
        Function::Exponential(
            ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                .with_c0(vec![0.0])
                .with_c1(vec![1.0]),
        ),
    ));
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let shading_pattern = b.shading_pattern(&ShadingPattern::new(shading));
    let tiling = b.tiling_pattern(&tiling_pattern());
    b.fill_rect_with_pattern(p, shading_pattern, [0.0, 0.0, 10.0, 10.0]);
    b.fill_rect_with_pattern(p, tiling, [20.0, 0.0, 10.0, 10.0]);
    let s = render(&mut b);

    assert!(s.contains("/P1 "), "{s}");
    assert!(s.contains("/P2 "), "{s}");
    assert_eq!(s.matches("/P1 scn").count(), 1, "{s}");
    assert_eq!(s.matches("/P2 scn").count(), 1, "{s}");
    assert!(s.contains("/PatternType 2"), "{s}");
    assert!(s.contains("/PatternType 1"), "{s}");
}

#[test]
fn unused_tiling_patterns_are_not_written() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Plain");
    b.tiling_pattern(&tiling_pattern());
    let s = render(&mut b);

    assert!(!s.contains("/PatternType 1"), "未使用的图案不应写出: {s}");
    assert!(!s.contains("/Pattern <<"), "{s}");
    assert!(!s.contains("/TilingType"), "{s}");
}

#[test]
fn tiling_pattern_requires_pdf_1_2() {
    let pattern = tiling_pattern();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p = b.page(PageSize::A4);
    let h = b.tiling_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [0.0, 0.0, 10.0, 10.0]);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.1 必须拒绝平铺图案");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "版本不足时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    let h = b.tiling_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [0.0, 0.0, 10.0, 10.0]);
    let s = render(&mut b);
    assert!(s.contains("%PDF-1.2"), "{s}");
    assert!(s.contains("/PatternType 1"), "{s}");
}

#[test]
fn invalid_tiling_pattern_parameters_fail_the_build() {
    fn assert_rejected(pattern: &TilingPattern, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.tiling_pattern(pattern);
        b.fill_rect_with_pattern(p, h, [0.0, 0.0, 10.0, 10.0]);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法平铺图案应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    let pattern = TilingPattern::new([0.0, f64::NAN, 8.0, 8.0], 8.0, 8.0, tile_content());
    assert_rejected(&pattern, "tiling pattern BBox");

    let pattern = TilingPattern::new([0.0, 0.0, -8.0, 8.0], 8.0, 8.0, tile_content());
    assert_rejected(&pattern, "minimum must not exceed maximum");

    let pattern = TilingPattern::new([0.0, 0.0, 8.0, 8.0], 0.0, 8.0, tile_content());
    assert_rejected(&pattern, "must be nonzero");

    let pattern = TilingPattern::new([0.0, 0.0, 8.0, 8.0], 8.0, f64::INFINITY, tile_content());
    assert_rejected(&pattern, "must be finite");

    let pattern = TilingPattern::new([0.0, 0.0, 8.0, 8.0], 8.0, 8.0, tile_content()).with_matrix([
        1.0,
        0.0,
        0.0,
        1.0,
        0.0,
        f64::NAN,
    ]);
    assert_rejected(&pattern, "tiling pattern Matrix");
}
