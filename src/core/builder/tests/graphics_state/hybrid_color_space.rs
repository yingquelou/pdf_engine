//! `/CS` 混合色彩空间与 `/CSS` 风格写出、约束校验与 PDF 1.4 门禁。
//!
//! 覆盖：单名形态、四种数组形态（N-N、N-Number、Number-N、Number-Number）、
//! `/CSS` 仅在 `/CS` 同现时有效、`/CSS` 字符串取值合法集、装配阶段回读
//! 字典的形态检查，以及 PDF 1.3 拒绝与 PDF 1.4 通过的版本门禁。

use super::super::*;
use super::ext_g_state;
use crate::core::types::PdfVersion;

#[test]
fn hybrid_color_space_name_form_is_written_as_a_name() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_name("CalRGB"),
    );
    let doc = b.assemble_document().expect("assemble");
    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    assert_eq!(gs.get("CS").and_then(Value::as_name), Some("CalRGB"));
    assert!(gs.get("CSS").is_none(), "/CSS must be absent when not set");
}

#[test]
fn hybrid_color_space_pair_of_names_writes_two_element_array() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_pair_names("CalRGB", "DeviceCMYK"),
    );
    let doc = b.assemble_document().expect("assemble");
    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let array = gs.get("CS").and_then(Value::as_array).expect("/CS array");
    assert_eq!(array.0.len(), 2);
    assert_eq!(array.0[0].as_name(), Some("CalRGB"));
    assert_eq!(array.0[1].as_name(), Some("DeviceCMYK"));
}

#[test]
fn hybrid_color_space_name_number_pair_writes_mixed_array() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_name_number("CalGray", 0.5),
    );
    let doc = b.assemble_document().expect("assemble");
    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let array = gs.get("CS").and_then(Value::as_array).expect("/CS array");
    assert_eq!(array.0.len(), 2);
    assert_eq!(array.0[0].as_name(), Some("CalGray"));
    match &array.0[1] {
        Value::Float(number) => assert_eq!(*number, 0.5),
        other => panic!("expected /CS second element to be a float, got {other:?}"),
    }
}

#[test]
fn hybrid_color_space_number_name_pair_writes_mixed_array() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_number_name(0.25, "CalRGB"),
    );
    let doc = b.assemble_document().expect("assemble");
    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let array = gs.get("CS").and_then(Value::as_array).expect("/CS array");
    assert!(matches!(array.0[0], Value::Float(x) if x == 0.25));
    assert_eq!(array.0[1].as_name(), Some("CalRGB"));
}

#[test]
fn hybrid_color_space_pair_of_numbers_writes_two_finite_floats() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_pair_numbers(0.1, 0.9),
    );
    let doc = b.assemble_document().expect("assemble");
    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let array = gs.get("CS").and_then(Value::as_array).expect("/CS array");
    assert_eq!(array.0.len(), 2);
    for slot in &array.0 {
        assert!(
            matches!(slot, Value::Float(_) if slot.as_array().is_none()),
            "array elements must be floats, got {slot:?}",
        );
    }
}

#[test]
fn hybrid_color_space_style_is_written_alongside_cs() {
    for style in ["Knockout", "Native"] {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.set_graphics_state(
            p,
            &GraphicsState::new()
                .with_hybrid_color_space_name("CalRGB")
                .with_hybrid_color_space_style(style),
        );
        let doc = b.assemble_document().expect("assemble");
        let (_, gs) = ext_g_state(&doc, 0, "GS1");
        assert_eq!(gs.get("CS").and_then(Value::as_name), Some("CalRGB"));
        assert_eq!(gs.get("CSS").and_then(Value::as_name), Some(style));
    }
}

#[test]
fn hybrid_color_space_style_without_cs_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_style("Knockout"),
    );
    super::assert_build_error(&mut b, "/CSS requires /CS");
}

#[test]
fn invalid_hybrid_color_space_style_string_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_hybrid_color_space_name("CalRGB")
            .with_hybrid_color_space_style("Surrender"),
    );
    super::assert_build_error(&mut b, "/CSS must be");
}

#[test]
fn invalid_hybrid_color_space_name_fails_the_build() {
    // `/CS` names must pass the same PDF-name syntax used for `/RI` and
    // `/BM`; a space is the smallest rejection the shared validator accepts.
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_name("bad name"),
    );
    super::assert_build_error(&mut b, "/CS");
}

#[test]
fn non_finite_hybrid_color_space_number_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_name_number("CalRGB", f64::NAN),
    );
    super::assert_build_error(&mut b, "/CS");
}

#[test]
fn hybrid_color_space_requires_pdf_1_4() {
    // `/CS` maps to the transparency feature model (§11.6.3), min V1_4;
    // registering `PdfFeature::Transparency` in the ExtGState assembly
    // step triggers the writer's version gate exactly like `/BM`/`/CA`/`/ca`
    // do via the shared reader scanner.
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_hybrid_color_space_name("CalRGB"),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject /CS");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "expected UnsupportedVersion, got {error:?}",
    );
    assert!(
        error.to_string().contains("transparency"),
        "error should mention transparency: {error}",
    );
    assert!(out.is_empty(), "no bytes on version rejection");

    // `/CSS` alone is rejected before version because it needs `/CS`;
    // only after `/CS` is set does the version gate become reachable.
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_hybrid_color_space_name("CalRGB")
            .with_hybrid_color_space_style("Knockout"),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject /CS+/CSS");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "expected UnsupportedVersion, got {error:?}",
    );
    assert!(out.is_empty(), "no bytes on version rejection");

    // Same content on PDF 1.4 succeeds and writes both keys.
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_hybrid_color_space_name("CalRGB")
            .with_hybrid_color_space_style("Native"),
    );
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports /CS + /CSS");
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("/CS /CalRGB"), "output missing /CS: {s}");
    assert!(s.contains("/CSS /Native"), "output missing /CSS: {s}");
}

#[test]
fn line_width_state_without_hybrid_entries_builds_at_pdf_1_3() {
    // Regression: adding `/CS`/`/CSS` fields to `GraphicsState` must not
    // register `Transparency` when neither hybrid field is set.
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_line_width(1.5));
    let mut out = Vec::new();
    b.build(&mut out)
        .expect("line-width-only state must stay 1.3-legal");
    let s = String::from_utf8_lossy(&out);
    assert!(
        !s.contains("/CS "),
        "no /CS must leak for a line-width state"
    );
    assert!(
        !s.contains("/CSS "),
        "no /CSS must leak for a line-width state"
    );
}
