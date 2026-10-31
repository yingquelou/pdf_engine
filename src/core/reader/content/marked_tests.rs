//! Marked-content reading semantics tests (C4).

use crate::core::reader::content::{
    MAX_MARKED_DEPTH, MarkedContentError, MarkedSection, Op, collect_marked_sections,
    resolve_marked_visibility,
};
use crate::core::types::{
    Array, Dict, Document, Object, StartXref, StreamData, Trailer, Value, Xref,
};

fn collect(ops: &[Op]) -> Result<Vec<MarkedSection>, MarkedContentError> {
    let resolve_property_name = |_name: &str| -> Option<Dict> { None };
    let resolve_ocg_name = |_name: &str| -> Option<i64> { None };
    let is_visible = |_object: i64| -> Option<bool> { None };
    collect_marked_sections(ops, &resolve_property_name, &resolve_ocg_name, &is_visible)
}

#[test]
fn pairs_begin_and_end_with_spans() {
    let ops = vec![
        Op::BeginMarkedContent("P".into()),
        Op::ShowText(b"hi".to_vec()),
        Op::EndMarkedContent,
    ];
    let sections = collect(&ops).expect("balanced");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].tag, "P");
    assert_eq!(sections[0].start, 0);
    assert_eq!(sections[0].end, 3);
    assert!(!sections[0].point);
    assert!(sections[0].properties.is_none());
}

#[test]
fn nests_sections_and_closes_inner_first() {
    let ops = vec![
        Op::BeginMarkedContent("Art".into()),
        Op::BeginMarkedContent("Span".into()),
        Op::EndMarkedContent,
        Op::EndMarkedContent,
    ];
    let sections = collect(&ops).expect("balanced");
    // inner closes first (push order follows EMC positions)
    assert_eq!(sections[0].tag, "Span");
    assert_eq!(sections[0].start, 1);
    assert_eq!(sections[0].end, 3);
    assert_eq!(sections[1].tag, "Art");
    assert_eq!(sections[1].start, 0);
    assert_eq!(sections[1].end, 4);
    assert_eq!(sections[0].parent, Some(1));
    assert_eq!(sections[1].parent, None);
}

#[test]
fn parent_links_follow_nesting_and_points() {
    let ops = vec![
        Op::BeginMarkedContent("Outer".into()),
        Op::MarkedContentPoint("P".into()),
        Op::BeginMarkedContent("Inner".into()),
        Op::MarkedContentPointWithProperties("Q".into(), Value::Dict(Dict::new())),
        Op::EndMarkedContent,
        Op::EndMarkedContent,
    ];
    let sections = collect(&ops).expect("balanced");
    // 关闭顺序：P、Q、Inner、Outer；父下标指向返回 Vec 中的位置。
    let tags: Vec<&str> = sections
        .iter()
        .map(|section| section.tag.as_str())
        .collect();
    assert_eq!(tags, ["P", "Q", "Inner", "Outer"]);
    assert_eq!(sections[0].parent, Some(3), "point P 的父区段是 Outer");
    assert_eq!(sections[1].parent, Some(2), "point Q 的父区段是 Inner");
    assert_eq!(sections[2].parent, Some(3));
    assert_eq!(sections[3].parent, None);
}

#[test]
fn point_operators_are_zero_length_sections() {
    let ops = vec![
        Op::MarkedContentPoint("Figure".into()),
        Op::MarkedContentPointWithProperties("Table".into(), Value::Name("Props".into())),
    ];
    let resolve_property_name =
        Box::new(|name: &str| -> Option<Dict> { (name == "Props").then(Dict::new) });
    let resolve_ocg_name = Box::new(|_: &str| -> Option<i64> { None });
    let is_visible = Box::new(|_: i64| -> Option<bool> { None });
    let sections =
        collect_marked_sections(&ops, &resolve_property_name, &resolve_ocg_name, &is_visible)
            .expect("points never need EMC");
    assert_eq!(sections.len(), 2);
    assert!(sections[0].point);
    assert_eq!(sections[0].start, 0);
    assert_eq!(sections[0].end, 1);
    assert!(sections[1].point);
    assert!(sections[1].properties.is_some());
}

#[test]
fn bdc_resolves_inline_dictionary_properties() {
    let properties = Dict::new().insert("ActualText", Value::String("replaced".into()));
    let ops = vec![
        Op::BeginMarkedContentWithProperties("Span".into(), Value::Dict(properties)),
        Op::EndMarkedContent,
    ];
    let sections = collect(&ops).expect("balanced");
    assert_eq!(sections[0].actual_text().as_deref(), Some("replaced"));
}

#[test]
fn bdc_resolves_property_name_through_resources() {
    let ops = vec![
        Op::BeginMarkedContentWithProperties("Span".into(), Value::Name("P0".into())),
        Op::EndMarkedContent,
    ];
    let resolve_property_name = Box::new(|name: &str| -> Option<Dict> {
        (name == "P0").then(|| Dict::new().insert("E", Value::String("abbr".into())))
    });
    let resolve_ocg_name = Box::new(|_: &str| -> Option<i64> { None });
    let is_visible = Box::new(|_: i64| -> Option<bool> { None });
    let sections =
        collect_marked_sections(&ops, &resolve_property_name, &resolve_ocg_name, &is_visible)
            .expect("balanced");
    assert_eq!(sections[0].element_text().as_deref(), Some("abbr"));
}

#[test]
fn bdc_name_pointing_at_ocg_dictionary_marks_group() {
    // The builder's pattern: `/OC /OC2 BDC` where /Properties /OC2 is the
    // OCG dictionary itself rather than a property list with /OC.
    let ops = vec![
        Op::BeginMarkedContentWithProperties("OC".into(), Value::Name("OC2".into())),
        Op::EndMarkedContent,
    ];
    let resolve_property_name = Box::new(|name: &str| -> Option<Dict> {
        (name == "OC2").then(|| {
            Dict::new()
                .insert("Type", Value::Name("OCG".into()))
                .insert("Name", Value::String("Layer".into()))
        })
    });
    let resolve_ocg_name = Box::new(|name: &str| -> Option<i64> { (name == "OC2").then_some(7) });
    let is_visible = Box::new(|_: i64| -> Option<bool> { Some(false) });
    let sections =
        collect_marked_sections(&ops, &resolve_property_name, &resolve_ocg_name, &is_visible)
            .expect("balanced");
    assert_eq!(sections[0].visibility, Some(false));
}

#[test]
fn unmatched_emc_reports_operator_index() {
    let ops = vec![Op::ShowText(b"x".to_vec()), Op::EndMarkedContent];
    let error = collect(&ops).expect_err("EMC without BMC");
    assert_eq!(error, MarkedContentError::UnmatchedEnd { index: 1 });
}

#[test]
fn unclosed_section_reports_outermost_tag() {
    let ops = vec![
        Op::BeginMarkedContent("Outer".into()),
        Op::BeginMarkedContent("Inner".into()),
        Op::EndMarkedContent,
    ];
    let error = collect(&ops).expect_err("Outer never closed");
    assert_eq!(
        error,
        MarkedContentError::UnclosedSection {
            tag: "Outer".into(),
            start: 0,
        }
    );
}

#[test]
fn depth_limit_is_enforced() {
    let ops = vec![Op::BeginMarkedContent("D".into()); MAX_MARKED_DEPTH + 1];
    let error = collect(&ops).expect_err("nesting exceeds the limit");
    assert_eq!(
        error,
        MarkedContentError::DepthExceeded {
            index: MAX_MARKED_DEPTH
        }
    );
}

#[test]
fn text_entries_decode_literal_and_hex_strings() {
    // FE FF 00 41 = UTF-16BE "A"; hex digits are stored uppercase.
    let properties = Dict::new()
        .insert("ActualText", Value::HexString("FEFF0041".into()))
        .insert("Alt", Value::String("plain".into()))
        .insert("Lang", Value::Name("en-US".into()));
    let section = MarkedSection {
        tag: "Span".into(),
        properties: Some(properties),
        visibility: None,
        start: 0,
        end: 1,
        point: false,
        parent: None,
    };
    assert_eq!(section.actual_text().as_deref(), Some("A"));
    assert_eq!(section.alternate_text().as_deref(), Some("plain"));
    assert_eq!(section.language().as_deref(), Some("en-US"));
    assert_eq!(section.element_text(), None);
}

#[test]
fn visibility_resolves_oc_name_and_reference() {
    let by_name = Dict::new().insert("OC", Value::Name("MC0".into()));
    let by_ref = Dict::new().insert("OC", Value::Ref(7, 0));
    let by_dict = Dict::new().insert("OC", Value::Dict(Dict::new()));
    let resolve_ocg_name = Box::new(|name: &str| -> Option<i64> { (name == "MC0").then_some(7) });
    let is_visible = Box::new(|object: i64| -> Option<bool> { (object == 7).then_some(false) });
    assert_eq!(
        resolve_marked_visibility(Some(&by_name), &resolve_ocg_name, &is_visible),
        Some(false)
    );
    assert_eq!(
        resolve_marked_visibility(Some(&by_ref), &resolve_ocg_name, &is_visible),
        Some(false)
    );
    // An empty dictionary is neither an OCG nor an OCMD: unresolved.
    assert_eq!(
        resolve_marked_visibility(Some(&by_dict), &resolve_ocg_name, &is_visible),
        None
    );
    assert_eq!(
        resolve_marked_visibility(None, &resolve_ocg_name, &is_visible),
        None
    );
}

#[test]
fn visibility_evaluates_inline_membership_dictionary() {
    let ocmd = Dict::new().insert(
        "OC",
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("OCMD".into()))
                .insert(
                    "OCGs",
                    Value::Array(Array::new().push(Value::Ref(7, 0)).push(Value::Ref(8, 0))),
                )
                .insert("P", Value::Name("AnyOff".into())),
        ),
    );
    let resolve_ocg_name = Box::new(|_: &str| -> Option<i64> { None });
    let is_visible = Box::new(|object: i64| -> Option<bool> {
        match object {
            7 => Some(true),
            8 => Some(false),
            _ => None,
        }
    });
    assert_eq!(
        resolve_marked_visibility(Some(&ocmd), &resolve_ocg_name, &is_visible),
        Some(true),
        "/AnyOff is true when at least one group is off"
    );
}

// ── Document::page_marked_content fixture ─────────────────

fn ocg_object() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("OCG".into()))
            .insert("Name", Value::String("Hidden group".into())),
    )
}

fn marked_page_document() -> Document {
    let content = b"q /P <</OC /MC0 /ActualText (secret-replacement)>> BDC \
                      BT (shown) Tj ET EMC Q";
    let objects = vec![
        Object::new(
            1,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Catalog".into()))
                    .insert("Pages", Value::Ref(2, 0))
                    .insert(
                        "OCProperties",
                        Value::Dict(
                            Dict::new()
                                .insert("OCGs", Value::Array(Array::new().push(Value::Ref(5, 0))))
                                .insert(
                                    "D",
                                    Value::Dict(
                                        Dict::new()
                                            .insert("BaseState", Value::Name("ON".into()))
                                            .insert(
                                                "OFF",
                                                Value::Array(Array::new().push(Value::Ref(5, 0))),
                                            ),
                                    ),
                                ),
                        ),
                    ),
            ),
        ),
        Object::new(
            2,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Pages".into()))
                    .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                    .insert("Count", Value::Integer(1)),
            ),
        ),
        Object::new(
            3,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Page".into()))
                    .insert(
                        "Contents",
                        Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                    )
                    .insert(
                        "Resources",
                        Value::Dict(Dict::new().insert(
                            "Properties",
                            Value::Dict(Dict::new().insert("MC0", Value::Ref(5, 0))),
                        )),
                    ),
            ),
        ),
        Object::new(5, 0, ocg_object()),
    ];
    let mut trailer = Trailer::new();
    trailer.dict = Dict::new().insert("Root", Value::Ref(1, 0));
    Document::from_objects(objects, Xref::new(), trailer, StartXref(0))
}

#[test]
fn page_marked_content_resolves_properties_and_visibility() {
    let document = marked_page_document();
    let sections = document.page_marked_content(0).expect("balanced page");
    assert_eq!(sections.len(), 1);
    let section = &sections[0];
    assert_eq!(section.tag, "P");
    assert_eq!(section.visibility, Some(false));
    assert_eq!(section.actual_text().as_deref(), Some("secret-replacement"));
}
