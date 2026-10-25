//! 图形状态参数（ExtGState）资源与内容操作符测试。

use super::*;
use crate::core::types::PdfVersion;

fn page_dict(doc: &Document, index: i64) -> Dict {
    let page_num = 3 + index * 2;
    let object = doc
        .get_object(page_num)
        .expect("page lookup")
        .expect("page object");
    object
        .value
        .as_dict()
        .expect("page object should be a dictionary")
        .clone()
}

fn content_stream(doc: &Document, index: i64) -> String {
    let content_num = 4 + index * 2;
    let object = doc
        .get_object(content_num)
        .expect("content lookup")
        .expect("content object");
    let bytes = doc.stream_bytes(&object.value).expect("content bytes");
    String::from_utf8_lossy(&bytes).into_owned()
}

fn resource_entries(doc: &Document, index: i64, kind: &str) -> Dict {
    page_dict(doc, index)
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get(kind))
        .and_then(Value::as_dict)
        .cloned()
        .unwrap_or_else(|| panic!("page should have a /{kind} resource dictionary"))
}

fn ext_g_state(doc: &Document, index: i64, name: &str) -> (i64, Dict) {
    let resources = resource_entries(doc, index, "ExtGState");
    let num = resources
        .get(name)
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .unwrap_or_else(|| panic!("missing /{name} graphics state"));
    let object = doc
        .get_object(num)
        .expect("graphics state lookup")
        .expect("graphics state object");
    let dict = object
        .value
        .as_dict()
        .expect("graphics state should be a dictionary")
        .clone();
    (num, dict)
}

fn as_number(value: &Value) -> f64 {
    match value {
        Value::Integer(number) => *number as f64,
        Value::Float(number) => *number,
        other => panic!("expected a number, got {other:?}"),
    }
}

fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        _ => None,
    }
}

#[test]
fn set_graphics_state_writes_all_scalar_parameters() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_line_width(2.5)
            .with_line_cap(1)
            .with_line_join(2)
            .with_miter_limit(4.0)
            .with_dash_pattern(DashPattern::new(vec![2.0, 1.0], 0.5))
            .with_rendering_intent("RelativeColorimetric")
            .with_overprint(true)
            .with_nonstroking_overprint(false)
            .with_overprint_mode(1)
            .with_flatness(50.0)
            .with_smoothness(0.25)
            .with_stroke_adjustment(true)
            .with_blend_mode("Multiply")
            .with_stroke_alpha(0.8)
            .with_fill_alpha(0.6)
            .with_alpha_is_shape(true)
            .with_text_knockout(false),
    );
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    assert_eq!(gs.get("Type").and_then(Value::as_name), Some("ExtGState"));
    assert_eq!(as_number(gs.get("LW").expect("/LW")), 2.5);
    assert_eq!(gs.get("LC").and_then(Value::as_integer), Some(1));
    assert_eq!(gs.get("LJ").and_then(Value::as_integer), Some(2));
    assert_eq!(as_number(gs.get("ML").expect("/ML")), 4.0);
    let dash = gs.get("D").expect("/D").as_array().expect("/D array");
    assert_eq!(numbers(&dash.0[0]), vec![2.0, 1.0]);
    assert_eq!(as_number(&dash.0[1]), 0.5);
    assert_eq!(
        gs.get("RI").and_then(Value::as_name),
        Some("RelativeColorimetric")
    );
    assert_eq!(gs.get("OP").and_then(as_bool), Some(true));
    assert_eq!(gs.get("op").and_then(as_bool), Some(false));
    assert_eq!(gs.get("OPM").and_then(Value::as_integer), Some(1));
    assert_eq!(as_number(gs.get("FL").expect("/FL")), 50.0);
    assert_eq!(as_number(gs.get("SM").expect("/SM")), 0.25);
    assert_eq!(gs.get("SA").and_then(as_bool), Some(true));
    assert_eq!(gs.get("BM").and_then(Value::as_name), Some("Multiply"));
    assert_eq!(as_number(gs.get("CA").expect("/CA")), 0.8);
    assert_eq!(as_number(gs.get("ca").expect("/ca")), 0.6);
    assert_eq!(gs.get("AIS").and_then(as_bool), Some(true));
    assert_eq!(gs.get("TK").and_then(as_bool), Some(false));
    assert!(content_stream(&doc, 0).contains("/GS1 gs"));
}

#[test]
fn multiple_blend_modes_are_written_as_a_fallback_array() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_blend_modes(&["Normal", "Compatible"]),
    );
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let modes = gs.get("BM").expect("/BM").as_array().expect("/BM array");
    let names: Vec<&str> = modes
        .iter()
        .map(|mode| mode.as_name().expect("blend mode name"))
        .collect();
    assert_eq!(names, vec!["Normal", "Compatible"]);
}

#[test]
fn repeated_graphics_states_reuse_one_resource() {
    let state = GraphicsState::new().with_line_width(1.0);
    let other = GraphicsState::new().with_line_width(2.0);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &state);
    b.set_graphics_state(p, &state);
    b.set_graphics_state(p, &other);
    let doc = b.assemble_document().expect("assemble");

    let resources = resource_entries(&doc, 0, "ExtGState");
    assert_eq!(resources.0.len(), 2);
    assert!(resources.get("GS1").is_some());
    assert!(resources.get("GS2").is_some());

    let content = content_stream(&doc, 0);
    assert_eq!(content.matches("/GS1 gs").count(), 2);
    assert_eq!(content.matches("/GS2 gs").count(), 1);
}

#[test]
fn identical_graphics_states_share_one_object_across_pages() {
    let state = GraphicsState::new().with_stroke_alpha(0.5);
    let mut b = PdfBuilder::new();
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    b.set_graphics_state(first, &state);
    b.set_graphics_state(second, &state);
    let doc = b.assemble_document().expect("assemble");

    let first_ref = resource_entries(&doc, 0, "ExtGState")
        .get("GS1")
        .and_then(Value::as_ref)
        .expect("first page /GS1");
    let second_ref = resource_entries(&doc, 1, "ExtGState")
        .get("GS1")
        .and_then(Value::as_ref)
        .expect("second page /GS1");
    assert_eq!(first_ref, second_ref);
}

#[test]
fn graphics_state_font_entry_references_the_font_object() {
    let font = Font::helvetica(11.0);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_font(font, 18.0));
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let entry = gs
        .get("Font")
        .expect("/Font")
        .as_array()
        .expect("/Font array");
    let font_num = entry.0[0]
        .as_ref()
        .expect("font entry should be an indirect reference")
        .0;
    assert_eq!(as_number(&entry.0[1]), 18.0);

    let font_dict = doc
        .get_object(font_num)
        .expect("font lookup")
        .expect("font object")
        .value;
    let font_dict = font_dict.as_dict().expect("font dictionary");
    assert_eq!(font_dict.get("Type").and_then(Value::as_name), Some("Font"));
    assert_eq!(
        font_dict.get("BaseFont").and_then(Value::as_name),
        Some("Helvetica")
    );
    // 字体同时登记在页面 /Font 资源中。
    assert_eq!(resource_entries(&doc, 0, "Font").0.len(), 1);
}

#[test]
fn pages_without_graphics_states_have_no_ext_g_state_resource() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "Hello");
    let doc = b.assemble_document().expect("assemble");

    let resources = page_dict(&doc, 0)
        .get("Resources")
        .and_then(Value::as_dict)
        .cloned()
        .expect("resources");
    assert!(resources.get("ExtGState").is_none());
}

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
