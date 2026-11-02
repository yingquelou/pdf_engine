//! Type 1–3 直接渐变与 PatternType 2 着色图案的写出：`/Shading` 字典条目、
//! 图案资源复用与未使用渐变/图案不落盘的契约。

use super::super::*;
use super::support::{axial_shading, exponential, render_with_shading, shading_pattern};

#[test]
fn function_shading_writes_type1_domain_matrix_and_sampled_function() {
    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0, 0.0, 1.0],
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![2, 2],
        8,
        (0..12).map(|v| (v * 20) as f64).collect::<Vec<_>>(),
    ));
    let shading = Shading::Function(
        FunctionShading::new(ColorSpace::DeviceRGB, [0.0, 1.0, 0.0, 1.0], function)
            .with_matrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]),
    );
    let s = render_with_shading(shading);

    assert!(s.contains("/Shading <<"), "{s}");
    assert!(s.contains("/Sh1 "), "{s}");
    assert!(s.contains("/ShadingType 1"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Matrix [1.0 0.0 0.0 1.0 10.0 20.0]"), "{s}");
    assert!(s.contains("/FunctionType 0"), "{s}");
    assert!(s.contains("/Sh1 sh"), "{s}");
}

#[test]
fn axial_shading_writes_coords_domain_function_and_extend() {
    let s = render_with_shading(axial_shading());

    assert!(s.contains("/ShadingType 2"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceGray"), "{s}");
    assert!(s.contains("/Coords [0.0 0.0 100.0 0.0]"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0]"), "{s}");
    assert!(s.contains("/Extend [true false]"), "{s}");
    assert!(s.contains("/Function <<"), "{s}");
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(s.contains("/C0 [0.0]"), "{s}");
    assert!(s.contains("/C1 [1.0]"), "{s}");
    assert!(s.contains("/Sh1 sh"), "{s}");
}

#[test]
fn radial_shading_writes_six_coords_and_omits_default_entries() {
    let function = exponential(vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]);
    let shading = Shading::Radial(RadialShading::new(
        ColorSpace::DeviceRGB,
        [50.0, 50.0, 0.0, 50.0, 50.0, 80.0],
        function,
    ));
    let s = render_with_shading(shading);

    assert!(s.contains("/ShadingType 3"), "{s}");
    assert!(s.contains("/Coords [50.0 50.0 0.0 50.0 50.0 80.0]"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(!s.contains("/Extend"), "省略 Extend 时不应写出: {s}");
    assert_eq!(
        s.matches("/Domain [0.0 1.0]").count(),
        1,
        "只应出现函数自身的 Domain，渐变 Domain 省略: {s}"
    );
}

#[test]
fn shading_pattern_writes_pattern_type_two_and_reuses_page_resource() {
    let pattern = shading_pattern();
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.shading_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [70.0, 500.0, 120.0, 60.0]);
    b.fill_rect_with_pattern(p, h, [70.0, 400.0, 120.0, 60.0]);
    let s = render(&mut b);

    assert!(s.contains("/Pattern <<"), "{s}");
    assert!(s.contains("/P1 "), "{s}");
    assert!(!s.contains("/P2"), "同一图案应复用 /P1: {s}");
    assert_eq!(s.matches("/P1 scn").count(), 2, "{s}");
    assert!(s.contains("/Pattern cs"), "{s}");
    assert!(s.contains("/PatternType 2"), "{s}");
    assert!(s.contains("/Matrix [2.0 0.0 0.0 2.0 5.0 5.0]"), "{s}");
    assert!(s.contains("/Shading <<"), "图案应内联渐变字典: {s}");
    assert!(s.contains("/ShadingType 2"), "{s}");
    assert!(s.contains("70.0 500.0 120.0 60.0 re"), "{s}");
    assert!(s.contains("70.0 400.0 120.0 60.0 re"), "{s}");
}

#[test]
fn unused_shadings_and_patterns_are_not_written() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Plain");
    b.shading(&axial_shading());
    b.shading_pattern(&shading_pattern());
    let s = render(&mut b);

    assert!(!s.contains("/ShadingType"), "未绘制的渐变不应写出: {s}");
    assert!(!s.contains("/PatternType"), "未使用的图案不应写出: {s}");
    assert!(!s.contains("/Shading <<"), "{s}");
    assert!(!s.contains(" sh\n"), "{s}");
}
