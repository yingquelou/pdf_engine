//! 标记类与 Link 的原条目保留：`/QuadPoints`、色彩/不透明度与 InkList 等
//! 未建模条目，及 Link 的 `/A` 动作、`/Dest` 目的地与 `/Popup` 引用原样保留。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Value};

use super::super::support::*;

#[test]
fn markup_annotation_reads_quad_points_and_keeps_raw_entries() {
    let quad = Value::Array(Array((0..8).map(Value::Integer).collect()));
    let ink_list = Value::Array(Array(vec![Value::Array(Array(vec![
        Value::Integer(1),
        Value::Integer(2),
    ]))]));
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Highlight")
            .insert("QuadPoints", quad)
            .insert(
                "C",
                Value::Array(Array(vec![
                    Value::Integer(1),
                    Value::Integer(1),
                    Value::Integer(0),
                ])),
            )
            .insert("CA", Value::Float(0.5))
            .insert("InkList", ink_list.clone())
            .insert("M", Value::String("D:19900428".into()))
            .insert("T", Value::HexString("0048".into())),
    )
    .expect("markup");
    assert!(annotation.subtype.is_markup());
    assert_eq!(annotation.quad_points.expect("quad").len(), 8);
    assert_eq!(annotation.color, Some(vec![1.0, 1.0, 0.0]));
    assert_eq!(annotation.opacity, Some(0.5));
    assert_eq!(
        annotation.modified_date,
        Some(Value::String("D:19900428".into()))
    );
    assert_eq!(annotation.title, Some(Value::HexString("0048".into())));
    assert_eq!(annotation.dict.get("InkList"), Some(&ink_list));

    let bad_quad = base_annotation("Highlight").insert(
        "QuadPoints",
        Value::Array(Array(vec![Value::Integer(1); 3])),
    );
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_quad),
        Err(AnnotationError::Malformed(_))
    ));
}

#[test]
fn link_annotation_keeps_action_and_destination_raw() {
    let action = Value::Dict(
        Dict::new()
            .insert("S", Value::Name("URI".into()))
            .insert("URI", Value::String("https://example.com".into())),
    );
    let dest = Value::Array(Array(vec![
        Value::Ref(3, 0),
        Value::Name("XYZ".into()),
        Value::Integer(0),
    ]));
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        Some((7, 0)),
        &base_annotation("Link")
            .insert("A", action.clone())
            .insert("Dest", dest.clone())
            .insert("Popup", Value::Ref(8, 0)),
    )
    .expect("link");
    assert_eq!(annotation.action, Some(action));
    assert_eq!(annotation.destination, Some(dest));
    assert_eq!(annotation.popup, Some((8, 0)));

    let bad_popup = base_annotation("Link").insert("Popup", Value::Integer(3));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_popup),
        Err(AnnotationError::Malformed(_))
    ));
}
