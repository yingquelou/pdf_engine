//! Square/Circle/Polygon/PolyLine 专有条目读取（12.5.6.8–12.5.6.9，Table 177/178）。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Value};

use super::support::*;

#[test]
fn square_and_circle_annotations_read_border_effect_and_rect_differences() {
    let dict = base_annotation("Square")
        .insert("IC", Value::Array(Array(vec![Value::Float(0.2)])))
        .insert(
            "BE",
            Value::Dict(
                Dict::new()
                    .insert("S", Value::Name("C".into()))
                    .insert("I", Value::Float(1.5)),
            ),
        )
        .insert(
            "RD",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
                Value::Integer(4),
            ])),
        );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("square annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Square);
    assert_eq!(annotation.interior_color, Some(vec![0.2]));
    let effect = annotation.border_effect.expect("/BE");
    assert_eq!(effect.style, BorderEffectStyle::Cloudy);
    assert_eq!(effect.intensity, Some(1.5));
    assert_eq!(annotation.text_rectangle, Some([1.0, 2.0, 3.0, 4.0]));
    assert!(annotation.vertices.is_none());
    assert!(annotation.polygon_intent.is_none());

    // Circle 共用同一组键。
    let circle =
        base_annotation("Circle").insert("RD", Value::Array(Array(vec![Value::Integer(0); 4])));
    let annotation = parse_annotation(&doc(vec![catalog_object()]), 0, None, &circle)
        .expect("circle annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Circle);
    assert_eq!(annotation.text_rectangle, Some([0.0, 0.0, 0.0, 0.0]));
    assert!(annotation.border_effect.is_none());
}

#[test]
fn polygon_annotation_reads_vertices_and_cloud_intent() {
    let dict = base_annotation("Polygon")
        .insert(
            "Vertices",
            Value::Array(Array(vec![
                Value::Integer(10),
                Value::Integer(20),
                Value::Float(30.5),
                Value::Integer(40),
                Value::Integer(50),
                Value::Integer(60),
            ])),
        )
        .insert("IT", Value::Name("PolygonCloud".into()))
        .insert(
            "BE",
            Value::Dict(Dict::new().insert("S", Value::Name("C".into()))),
        );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("polygon annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Polygon);
    assert_eq!(
        annotation.vertices,
        Some(vec![10.0, 20.0, 30.5, 40.0, 50.0, 60.0])
    );
    assert_eq!(annotation.polygon_intent, Some(PolygonIntent::PolygonCloud));
    assert!(annotation.line_intent.is_none());
    assert!(annotation.free_text_intent.is_none());
    assert_eq!(
        annotation.border_effect.map(|effect| effect.style),
        Some(BorderEffectStyle::Cloudy)
    );
}

#[test]
fn polyline_annotation_reads_vertices_and_line_endings() {
    let dict = base_annotation("PolyLine")
        .insert(
            "Vertices",
            Value::Array(Array(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(10),
                Value::Integer(10),
            ])),
        )
        .insert(
            "LE",
            Value::Array(Array(vec![
                Value::Name("OpenArrow".into()),
                Value::Name("Slash".into()),
            ])),
        )
        .insert("IT", Value::Name("PolyLineDimension".into()));
    let annotation = parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict)
        .expect("polyline annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::PolyLine);
    assert_eq!(annotation.vertices, Some(vec![0.0, 0.0, 10.0, 10.0]));
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::OpenArrow, LineEndStyle::Slash])
    );
    // 数组 `/LE` 不落入 FreeText 的单名字字段。
    assert!(annotation.callout_ending.is_none());
    assert_eq!(
        annotation.polygon_intent,
        Some(PolygonIntent::PolyLineDimension)
    );
    assert!(annotation.line_intent.is_none());
}

#[test]
fn polygon_intent_names_route_to_their_owning_family() {
    // 已知名字先按集合归属，不依赖声明的 `/Subtype`。
    for (name, expected) in [
        ("PolygonDimension", PolygonIntent::PolygonDimension),
        ("PolygonCloud", PolygonIntent::PolygonCloud),
    ] {
        let dict = base_annotation("Polygon").insert("IT", Value::Name(name.into()));
        let annotation =
            parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("polygon intent");
        assert_eq!(annotation.polygon_intent, Some(expected));
        assert!(annotation.line_intent.is_none());
    }

    // 三家都不认识的名字按声明的 `/Subtype` 落进对应 `Other`。
    for (subtype, check) in [
        ("Polygon", 0u8),
        ("PolyLine", 1),
        ("Square", 2),
        ("FreeText", 3),
    ] {
        let dict = base_annotation(subtype).insert("IT", Value::Name("MysteryIntent".into()));
        let annotation =
            parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("unknown intent");
        match check {
            0 | 1 => assert_eq!(
                annotation.polygon_intent,
                Some(PolygonIntent::Other("MysteryIntent".into())),
                "{subtype}"
            ),
            3 => assert_eq!(
                annotation.free_text_intent,
                Some(FreeTextIntent::Other("MysteryIntent".into())),
                "{subtype}"
            ),
            _ => assert_eq!(
                annotation.line_intent,
                Some(LineIntent::Other("MysteryIntent".into())),
                "{subtype}"
            ),
        }
    }
}

#[test]
fn malformed_geometry_entries_are_typed_errors() {
    let cases: Vec<(&str, Dict)> = vec![
        (
            "vertices 奇数个",
            base_annotation("Polygon")
                .insert("Vertices", Value::Array(Array(vec![Value::Integer(0); 5]))),
        ),
        (
            "vertices 少于两个顶点",
            base_annotation("Polygon")
                .insert("Vertices", Value::Array(Array(vec![Value::Integer(0); 2]))),
        ),
        (
            "vertices 非数值",
            base_annotation("PolyLine").insert(
                "Vertices",
                Value::Array(Array(vec![Value::Name("x".into()), Value::Integer(0)])),
            ),
        ),
        (
            "vertices 非数组",
            base_annotation("Polygon").insert("Vertices", Value::Integer(1)),
        ),
        (
            "BE 非字典",
            base_annotation("Square").insert("BE", Value::Integer(1)),
        ),
        (
            "BE /I 越界",
            base_annotation("Circle").insert(
                "BE",
                Value::Dict(
                    Dict::new()
                        .insert("S", Value::Name("C".into()))
                        .insert("I", Value::Float(2.5)),
                ),
            ),
        ),
        (
            "RD 为负",
            base_annotation("Square")
                .insert("RD", Value::Array(Array(vec![Value::Integer(-1); 4]))),
        ),
        (
            "RD 长度不是 4",
            base_annotation("Circle").insert("RD", Value::Array(Array(vec![Value::Integer(0); 3]))),
        ),
        (
            "LE 元素非名字",
            base_annotation("PolyLine").insert(
                "LE",
                Value::Array(Array(vec![Value::Integer(1), Value::Integer(2)])),
            ),
        ),
    ];
    for (label, dict) in cases {
        let error = parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict)
            .expect_err(&format!("{label} 应报错"));
        assert!(
            matches!(error, AnnotationError::Malformed(_)),
            "{label}: {error:?}"
        );
    }
}
