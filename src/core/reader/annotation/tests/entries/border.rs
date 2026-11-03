//! 边框条目：`/BS` 字典优先于 `/Border` 遗留数组，`/BM` 混合模式、
//! 缺角半径与 dash 解析，以及非法 `Border`/`CA` 的类型化错误。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Value};

use super::super::support::*;

#[test]
fn border_prefers_bs_dictionary_over_legacy_array() {
    let bs_dict = Dict::new()
        .insert("W", Value::Float(2.0))
        .insert("S", Value::Name("Dashed".into()))
        .insert(
            "D",
            Value::Array(Array(vec![Value::Integer(3), Value::Integer(2)])),
        );
    let with_bs = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Square")
            .insert("BS", Value::Dict(bs_dict))
            .insert("BM", Value::Name("Multiply".into()))
            .insert(
                "Border",
                Value::Array(Array(vec![Value::Integer(4), Value::Integer(9)])),
            ),
    )
    .expect("bs");
    let border = with_bs.border.expect("border");
    assert_eq!(border.width, Some(2.0));
    assert_eq!(border.style.as_deref(), Some("Dashed"));
    assert_eq!(border.dash, vec![3.0, 2.0]);
    assert_eq!(border.blend_mode.as_deref(), Some("Multiply"));
    assert_eq!(border.corner_radii, None);

    let legacy = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Circle").insert(
            "Border",
            Value::Array(Array(vec![
                Value::Integer(2),
                Value::Integer(1),
                Value::Integer(4),
                Value::Array(Array(vec![Value::Integer(5), Value::Integer(5)])),
            ])),
        ),
    )
    .expect("legacy border");
    let border = legacy.border.expect("border");
    assert_eq!(border.corner_radii, Some([2.0, 1.0]));
    assert_eq!(border.width, Some(4.0));
    assert_eq!(border.dash, vec![5.0, 5.0]);
    assert_eq!(border.style, None);

    let blend_only = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Line").insert("BM", Value::Name("Normal".into())),
    )
    .expect("blend only");
    let border = blend_only.border.expect("border");
    assert_eq!(border.width, None);
    assert_eq!(border.blend_mode.as_deref(), Some("Normal"));

    for bad in [
        Value::Array(Array(vec![Value::Integer(0), Value::Integer(0)])),
        Value::Array(Array((0..5).map(Value::Integer).collect())),
        Value::Array(Array(vec![
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(1),
            Value::Integer(2),
        ])),
        Value::Array(Array(vec![
            Value::Name("wide".into()),
            Value::Integer(0),
            Value::Integer(1),
        ])),
    ] {
        let bad_border = base_annotation("Circle").insert("Border", bad);
        assert!(matches!(
            parse_annotation(&doc(vec![]), 0, None, &bad_border),
            Err(AnnotationError::Malformed(_))
        ));
    }

    let bad_ca = base_annotation("Text").insert("CA", Value::Float(1.5));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_ca),
        Err(AnnotationError::Malformed(_))
    ));
}
