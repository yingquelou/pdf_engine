//! 图形状态资源写出与复用：全标量参数（`LW`/`LC`/`LJ`/`ML`/`D`/`RI`/`OP`/`op`/
//! `OPM`/`FL`/`SM`/`SA`/`BM`/`CA`/`ca`/`AIS`/`TK`）与 `gs` 操作符写出、混合模式
//! 兜底数组、`/Font` 条目引用、同名状态复用单一资源、同一状态跨页共享对象与
//! 未使用状态不登记 `/ExtGState`。

use super::super::*;
use super::{as_bool, as_number, content_stream, ext_g_state, page_dict, resource_entries};

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
