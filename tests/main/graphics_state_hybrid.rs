//! `/CS` 混合色彩空间与 `/CSS` 风格读写双侧建模的往返测试。
//!
//! 覆盖：builder 写入单名形态与数组形态 `/CS`、`/CSS` 风格 → 序列化为字节
//! → `Parser` 重新解析 → 通过 `Document::ext_g_state` 取回读端类型化视图，
//! 断言类型化字段与写入值一致。另外覆盖 PDF 1.3 拒绝与 PDF 1.4+ 通过。

use pdf_engine::core::builder::{GraphicsState, PageSize, PdfBuilder};
use pdf_engine::core::reader::graphics_state::{
    ExtGState, HybridColorSpace, HybridColorSpaceElement, HybridColorSpaceStyle,
};
use pdf_engine::core::types::{PdfVersion, Value};

use super::*;

/// 从解析后的文档中定位第一页 `/Resources/ExtGState/<name>` 引用的字典并
/// 返回其读端类型化视图；缺失时 panic。
fn first_page_ext_g_state(doc: &Document, name: &str) -> ExtGState {
    let page = first_page_dict(doc);
    let ext_g_states = page
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("ExtGState"))
        .and_then(Value::as_dict)
        .unwrap_or_else(|| panic!("first page has no /ExtGState resource"));
    let reference = ext_g_states
        .get(name)
        .and_then(Value::as_ref)
        .unwrap_or_else(|| panic!("no /{name} entry in /ExtGState"));
    let value = Value::Ref(reference.0, reference.1);
    doc.ext_g_state(&value)
        .unwrap_or_else(|error| panic!("parse {name}: {error}"))
}

#[test]
fn hybrid_color_space_name_round_trips_through_parser() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.set_graphics_state(
        page,
        &GraphicsState::new().with_hybrid_color_space_name("CalRGB"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let state = first_page_ext_g_state(&reparsed, "GS1");
    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Name("CalRGB".into()))
    );
    assert!(state.hybrid_color_space_style.is_none());
    assert!(state.issues.is_empty(), "issues: {:?}", state.issues);
    assert!(
        !state.other.contains_key("CS"),
        "typed /CS must not also leak into `other`",
    );
}

#[test]
fn hybrid_color_space_pair_of_names_round_trips_through_parser() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.set_graphics_state(
        page,
        &GraphicsState::new().with_hybrid_color_space_pair_names("CalRGB", "DeviceCMYK"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let state = first_page_ext_g_state(&reparsed, "GS1");
    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Pair(
            HybridColorSpaceElement::Name("CalRGB".into()),
            HybridColorSpaceElement::Name("DeviceCMYK".into()),
        ))
    );
    assert!(state.issues.is_empty(), "issues: {:?}", state.issues);
}

#[test]
fn hybrid_color_space_name_number_pair_round_trips_through_parser() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.set_graphics_state(
        page,
        &GraphicsState::new().with_hybrid_color_space_name_number("CalGray", 0.5),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let state = first_page_ext_g_state(&reparsed, "GS1");
    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Pair(
            HybridColorSpaceElement::Name("CalGray".into()),
            HybridColorSpaceElement::Number(0.5),
        ))
    );
}

#[test]
fn hybrid_color_space_style_round_trips_through_parser() {
    for (style, expected) in [
        ("Knockout", HybridColorSpaceStyle::Knockout),
        ("Native", HybridColorSpaceStyle::Native),
    ] {
        let mut builder = PdfBuilder::new();
        let page = builder.page(PageSize::A4);
        builder.set_graphics_state(
            page,
            &GraphicsState::new()
                .with_hybrid_color_space_name("CalRGB")
                .with_hybrid_color_space_style(style),
        );
        let mut buf = Vec::new();
        builder.build(&mut buf).expect("build {style}");
        let reparsed = parse_buf(&buf);

        let state = first_page_ext_g_state(&reparsed, "GS1");
        assert_eq!(
            state.hybrid_color_space,
            Some(HybridColorSpace::Name("CalRGB".into()))
        );
        assert_eq!(state.hybrid_color_space_style, Some(expected));
        assert!(
            state.issues.is_empty(),
            "issues for {style}: {:?}",
            state.issues
        );
    }
}

#[test]
fn hybrid_color_space_is_rejected_below_pdf_1_4() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    let page = builder.page(PageSize::A4);
    builder.set_graphics_state(
        page,
        &GraphicsState::new().with_hybrid_color_space_name("CalRGB"),
    );
    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("PDF 1.3 must reject /CS");
    assert!(
        matches!(
            error,
            pdf_engine::core::writer::WriteError::UnsupportedVersion(_)
        ),
        "expected UnsupportedVersion, got {error:?}",
    );
    assert!(
        error.to_string().contains("transparency"),
        "transparency must be named: {error}",
    );
    assert!(buf.is_empty(), "no bytes on version rejection");

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let page = builder.page(PageSize::A4);
    builder.set_graphics_state(
        page,
        &GraphicsState::new()
            .with_hybrid_color_space_name("CalRGB")
            .with_hybrid_color_space_style("Native"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("PDF 1.4 accepts /CS + /CSS");
    let reparsed = parse_buf(&buf);
    let state = first_page_ext_g_state(&reparsed, "GS1");
    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Name("CalRGB".into()))
    );
    assert_eq!(
        state.hybrid_color_space_style,
        Some(HybridColorSpaceStyle::Native)
    );
}

#[test]
fn preserve_and_reports_malformed_cs_from_raw_dictionary() {
    // Reader-side contract: an ExtGState dictionary whose `/CS` is an
    // integer and `/CSS` is an unknown name must still parse successfully
    // with the raw values preserved in `ExtGState::other` under the same
    // keys and matching diagnostics in `ExtGState::issues`.
    use pdf_engine::core::types::{Dict, Object};

    let raw_dict = Dict::new()
        .insert("Type", Value::Name("ExtGState".into()))
        .insert("LW", Value::Integer(3))
        .insert("CS", Value::Integer(7))
        .insert("CSS", Value::Name("Surrender".into()));
    let doc = Document::new();
    let num = 500i64;
    doc.set_object(Object::new(num, 0, Value::Dict(raw_dict)));

    let state = doc
        .ext_g_state(&Value::Ref(num, 0))
        .expect("malformed /CS and /CSS must not fail the parse");
    assert_eq!(state.line_width, Some(3.0));
    assert_eq!(state.hybrid_color_space, None);
    assert_eq!(state.hybrid_color_space_style, None);
    assert_eq!(state.other.get("CS"), Some(&Value::Integer(7)));
    assert_eq!(
        state.other.get("CSS"),
        Some(&Value::Name("Surrender".into())),
    );
    assert_eq!(state.issues.len(), 2, "both /CS and /CSS must be reported");
    let keys: Vec<&str> = state
        .issues
        .iter()
        .map(|issue| issue.key.as_str())
        .collect();
    assert_eq!(keys, vec!["CS", "CSS"]);
}
