//! Rubber Stamp（Table 181）图标名类型化，与文本标记 `/QuadPoints`（Table 179）
//! 的 8 数一组类型化访问、缺省形态与子类型归属分派。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Value};

use super::super::support::*;
use super::quad_array;

#[test]
fn text_markup_quad_points_group_by_eight_in_document_order() {
    let document = doc(vec![catalog_object()]);
    let values: Vec<i64> = (0..16).collect();
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Highlight").insert("QuadPoints", quad_array(&values)),
    )
    .expect("two-group highlight");
    let groups = annotation.text_markup_quad_groups().expect("quad groups");
    assert_eq!(
        groups,
        vec![
            [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            [8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0],
        ]
    );
    assert_eq!(annotation.quad_groups(), groups);

    // 四型共用 Table 179 条目，类型化访问一致。
    for subtype in ["Underline", "Squiggly", "StrikeOut"] {
        let annotation = parse_annotation(
            &document,
            0,
            None,
            &base_annotation(subtype).insert("QuadPoints", quad_array(&values[..8])),
        )
        .expect("single group");
        assert!(annotation.subtype.is_text_markup(), "{subtype}");
        assert_eq!(
            annotation.text_markup_quad_groups().expect("groups"),
            vec![[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]]
        );
    }
}

#[test]
fn malformed_quad_point_shapes_are_rejected_for_text_markup() {
    let document = doc(vec![catalog_object()]);
    for broken in [
        quad_array(&[]),
        quad_array(&[1, 2, 3]),
        Value::Array(Array(vec![
            Value::Integer(0),
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
            Value::Integer(4),
            Value::Integer(5),
            Value::Integer(6),
            Value::Name("x".into()),
        ])),
        Value::Array(Array(vec![Value::Float(f64::NAN); 8])),
        Value::Integer(7),
    ] {
        assert!(
            matches!(
                parse_annotation(
                    &document,
                    0,
                    None,
                    &base_annotation("Highlight").insert("QuadPoints", broken.clone())
                ),
                Err(AnnotationError::Malformed(_))
            ),
            "expect malformed for {broken:?}"
        );
    }
}

#[test]
fn stamp_annotation_reads_standard_icon_name_as_typed_variant() {
    let document = doc(vec![catalog_object()]);
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Stamp").insert("Name", Value::Name("TopSecret".into())),
    )
    .expect("stamp annotation");
    assert_eq!(annotation.icon_name.as_deref(), Some("TopSecret"));
    let icon = annotation.stamp_icon().expect("typed stamp icon");
    assert_eq!(icon, StampIcon::TopSecret);
    assert!(icon.is_standard());

    // Table 181 全 14 个标准名均可判别。
    for name in [
        "Approved",
        "Experimental",
        "NotApproved",
        "AsIs",
        "Expired",
        "NotForPublicRelease",
        "Confidential",
        "Final",
        "Sold",
        "Departmental",
        "ForComment",
        "TopSecret",
        "Draft",
        "ForPublicRelease",
    ] {
        let icon = StampIcon::from_name(name);
        assert!(icon.is_standard(), "{name}");
        assert_eq!(icon.name(), name);
    }
}

#[test]
fn stamp_annotation_keeps_unknown_icon_name_distinguishable() {
    let document = doc(vec![catalog_object()]);
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Stamp").insert("Name", Value::Name("RubberDuck".into())),
    )
    .expect("custom stamp");
    assert_eq!(
        annotation.stamp_icon(),
        Some(StampIcon::Other("RubberDuck".into()))
    );
    assert!(!annotation.stamp_icon().unwrap().is_standard());
    assert_eq!(annotation.stamp_icon().unwrap().name(), "RubberDuck");
}

#[test]
fn stamp_icon_requires_stamp_subtype_and_present_name() {
    let document = doc(vec![catalog_object()]);
    // Stamp 无 /Name：读端不代填 Table 181 缺省 Draft。
    let bare = parse_annotation(&document, 0, None, &base_annotation("Stamp")).expect("bare stamp");
    assert!(bare.stamp_icon().is_none());
    // Text 的 /Name 归通用 icon_name，不类型化为图章图标。
    let text = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("Name", Value::Name("Comment".into())),
    )
    .expect("text annotation");
    assert_eq!(text.icon_name.as_deref(), Some("Comment"));
    assert!(text.stamp_icon().is_none());
}

#[test]
fn quad_group_access_dispatches_by_subtype_attribution() {
    let document = doc(vec![catalog_object()]);
    let link = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Link").insert("QuadPoints", quad_array(&[0, 1, 2, 3, 4, 5, 6, 7])),
    )
    .expect("link with quad points");
    assert!(link.text_markup_quad_groups().is_none());
    assert_eq!(link.quad_groups().len(), 1);
    // 文本标记无 /QuadPoints：宽容缺省，类型化访问返回 None。
    let bare = parse_annotation(&document, 0, None, &base_annotation("Highlight")).expect("bare");
    assert!(bare.text_markup_quad_groups().is_none());
    assert!(bare.quad_groups().is_empty());
}

#[test]
fn is_text_markup_matches_the_four_table_179_subtypes() {
    for name in ["Highlight", "Underline", "Squiggly", "StrikeOut"] {
        assert!(
            AnnotationSubtype::from_name(name).is_text_markup(),
            "{name} should be text markup"
        );
    }
    for name in [
        "Text", "Stamp", "Link", "Redact", "FreeText", "Ink", "Caret", "Weird",
    ] {
        assert!(
            !AnnotationSubtype::from_name(name).is_text_markup(),
            "{name} should not be text markup"
        );
    }
}
