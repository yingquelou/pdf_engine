//! 图形状态参数（ExtGState）资源与内容操作符测试。

use super::*;
use crate::core::content::ContentStream;
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

/// 供 `/SMask` 引用的透明组表单；不绘制到页面，只由图形状态引用。
fn mask_form() -> FormXObject {
    let mut content = ContentStream::new();
    content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 100.0, 100.0)
        .fill();
    FormXObject::new([0.0, 0.0, 100.0, 100.0], content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray))
}

#[test]
fn soft_mask_writes_dictionary_and_form_reference() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_fill_alpha(0.5)
            .with_soft_mask(SoftMask::luminosity(mask).with_backdrop([0.0, 0.5, 1.0])),
    );
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Hi");
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let mask_dict = gs
        .get("SMask")
        .expect("/SMask")
        .as_dict()
        .expect("/SMask dictionary");
    assert_eq!(
        mask_dict.get("S").and_then(Value::as_name),
        Some("Luminosity")
    );
    assert_eq!(
        numbers(mask_dict.get("BC").expect("/BC")),
        vec![0.0, 0.5, 1.0]
    );
    let (group_num, _) = mask_dict
        .get("G")
        .expect("/G")
        .as_ref()
        .expect("/G should be an indirect reference");

    let group = doc
        .get_object(group_num)
        .expect("group lookup")
        .expect("group object");
    let Value::Stream(group_dict, _) = &group.value else {
        panic!("/G must reference a Form XObject stream");
    };
    assert_eq!(
        group_dict.get("Subtype").and_then(Value::as_name),
        Some("Form")
    );
    assert!(
        group_dict.get("Group").and_then(Value::as_dict).is_some(),
        "/SMask 表单应为透明组"
    );

    // 软蒙版表单只为 /G 写出，不登记为页面 XObject 资源。
    let resources = page_dict(&doc, 0)
        .get("Resources")
        .and_then(Value::as_dict)
        .cloned()
        .expect("resources");
    assert!(
        resources.get("XObject").is_none(),
        "软蒙版表单不应登记为页面 XObject: {resources:?}"
    );
    assert!(content_stream(&doc, 0).contains("/GS1 gs\n"));
}

#[test]
fn alpha_soft_mask_without_backdrop_omits_bc() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask)),
    );
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let mask_dict = gs
        .get("SMask")
        .expect("/SMask")
        .as_dict()
        .expect("/SMask dictionary");
    assert_eq!(mask_dict.get("S").and_then(Value::as_name), Some("Alpha"));
    assert!(!mask_dict.0.contains_key("BC"), "未设置背景色时应省略 /BC");
}

#[test]
fn invalid_soft_mask_parameters_fail_the_build() {
    let assert_invalid = |mut b: PdfBuilder, expected: &str| {
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法软蒙版应被拒绝");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    };

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_soft_mask(SoftMask::alpha(42)));
    assert_invalid(b, "unknown form handle 42");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mut content = ContentStream::new();
    content.rect(0.0, 0.0, 10.0, 10.0).fill();
    let handle = b.form_xobject(&FormXObject::new([0.0, 0.0, 10.0, 10.0], content));
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(handle)),
    );
    assert_invalid(b, "transparency group Form XObject");

    for backdrop in [Vec::new(), vec![1.5], vec![f64::NAN]] {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let mask = b.form_xobject(&mask_form());
        b.set_graphics_state(
            p,
            &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask).with_backdrop(backdrop)),
        );
        assert_invalid(b, "/SMask /BC");
    }
}

#[test]
fn soft_mask_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask)),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject /SMask");
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
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask)),
    );
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports /SMask");
    assert!(String::from_utf8_lossy(&out).contains("/SMask <<"));
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

fn four_input_function() -> Function {
    Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![0.0, 1.0],
        vec![2, 2, 2, 2],
        8,
        (0..16)
            .map(|value| value as f64 * 17.0)
            .collect::<Vec<f64>>(),
    ))
}

#[test]
fn black_generation_and_undercolor_functions_are_written() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.function(&four_input_function());
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_black_generation(handle)
            .with_black_generation_2(handle)
            .with_undercolor_removal(handle)
            .with_undercolor_removal_2(handle),
    );
    let doc = b.assemble_document().expect("assemble");
    let (_, gs) = ext_g_state(&doc, 0, "GS1");

    let mut function_num = None;
    for key in ["BG", "BG2", "UCR", "UCR2"] {
        let num = gs
            .get(key)
            .and_then(Value::as_ref)
            .map(|(num, _)| num)
            .unwrap_or_else(|| panic!("missing /{key} function reference"));
        assert_eq!(*function_num.get_or_insert(num), num, "/{key} should share");
        let object = doc
            .get_object(num)
            .expect("lookup")
            .expect("function object");
        let dict = object.value.as_dict().expect("function stream dictionary");
        assert_eq!(
            dict.get("FunctionType").and_then(Value::as_integer),
            Some(0)
        );
        assert_eq!(numbers(dict.get("Domain").expect("/Domain")).len(), 8);
        assert_eq!(numbers(dict.get("Range").expect("/Range")), vec![0.0, 1.0]);
    }

    let function_objects = (1..=32)
        .filter(|num| {
            matches!(
                doc.get_object(*num),
                Ok(Some(object)) if object
                    .value
                    .as_dict()
                    .and_then(|dict| dict.get("FunctionType"))
                    .is_some()
            )
        })
        .count();
    assert_eq!(function_objects, 1, "shared handle should be written once");
}

#[test]
fn invalid_black_generation_functions_fail_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.function(&Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![0.0, 0.0, 0.0])
            .with_c1(vec![1.0, 1.0, 1.0]),
    ));
    b.set_graphics_state(p, &GraphicsState::new().with_black_generation(handle));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("three-output /BG should be rejected");
    assert!(
        error
            .to_string()
            .contains("/BG function must produce 1 output"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn unknown_black_generation_handle_fails_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_black_generation(99));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("unknown handle should be rejected");
    assert!(
        error
            .to_string()
            .contains("/BG references unknown function handle 99"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}
