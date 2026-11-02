//! 图形状态参数校验、版本门禁与应用形态：非法标量参数在构建期以
//! `WriteError::InvalidObject` 拒绝且零字节输出、透明度参数要求 PDF 1.4、
//! 纯线宽状态在 1.3 可构建，以及透明度条目只能写入 ExtGState 字典并经
//! `gs` 应用（内容流不得出现内联写法）。

use super::super::*;
use super::{as_number, content_stream, ext_g_state};
use crate::core::types::PdfVersion;

#[test]
fn invalid_graphics_state_parameters_fail_the_build() {
    let states = [
        GraphicsState::new().with_line_width(-1.0),
        GraphicsState::new().with_line_width(f64::NAN),
        GraphicsState::new().with_line_cap(3),
        GraphicsState::new().with_line_join(3),
        GraphicsState::new().with_miter_limit(0.5),
        GraphicsState::new().with_dash_pattern(DashPattern::new(vec![1.0, -1.0], 0.0)),
        GraphicsState::new().with_dash_pattern(DashPattern::new(vec![1.0], -0.5)),
        GraphicsState::new().with_flatness(101.0),
        GraphicsState::new().with_smoothness(1.5),
        GraphicsState::new().with_overprint_mode(2),
        GraphicsState::new().with_stroke_alpha(-0.1),
        GraphicsState::new().with_fill_alpha(1.1),
        GraphicsState::new().with_blend_mode("bad name"),
        GraphicsState::new().with_blend_modes(&[]),
        GraphicsState::new().with_font(Font::helvetica(12.0), 0.0),
    ];
    for state in states {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.set_graphics_state(p, &state);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid state must fail");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn transparency_parameters_require_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_fill_alpha(0.5));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.3 must reject transparency");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("transparency"),
        "错误信息应说明透明度特性: {error}"
    );
}

#[test]
fn line_width_only_state_builds_at_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_line_width(2.0));
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.3 supports line width");
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("/ExtGState"), "应写出 ExtGState 资源字典");
    assert!(s.contains("/GS1 gs"), "内容流应应用 GS1");
}

/// ISO 32000-1 Table A.1 没有 ca/CA/BM 操作符：透明度参数只能写入
/// ExtGState 字典并经 `gs` 应用，内容流中不得出现内联写法。
#[test]
fn transparency_parameters_are_applied_through_gs_only() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_blend_mode("Multiply")
            .with_stroke_alpha(0.8)
            .with_fill_alpha(0.6),
    );
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Hi");
    let doc = b.assemble_document().expect("assemble");

    let content = content_stream(&doc, 0);
    assert!(content.contains("/GS1 gs\n"), "{content}");
    for inline in [" ca", " CA", " BM"] {
        assert!(
            !content.contains(inline),
            "内容流不得内联 {inline:?}: {content}"
        );
    }

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    assert_eq!(gs.get("BM").and_then(Value::as_name), Some("Multiply"));
    assert_eq!(as_number(gs.get("CA").expect("/CA")), 0.8);
    assert_eq!(as_number(gs.get("ca").expect("/ca")), 0.6);
}
