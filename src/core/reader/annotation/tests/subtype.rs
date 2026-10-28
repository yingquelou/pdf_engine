//! 子类型专有条目读取（12.5.6）。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Value};

use super::support::*;

#[test]
fn text_annotation_reads_icon_open_flag_and_markup_state() {
    let dict = base_annotation("Text")
        .insert("Name", Value::Name("Paragraph".into()))
        .insert("Open", Value::Bool(false))
        .insert("State", Value::String("Accepted".into()))
        .insert("StateModel", Value::HexString("4D61726B6564".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("text annotation");
    assert_eq!(annotation.icon_name.as_deref(), Some("Paragraph"));
    assert_eq!(annotation.is_open, Some(false));
    assert_eq!(annotation.state.as_deref(), Some("Accepted"));
    assert_eq!(annotation.state_model.as_deref(), Some("Marked"));
    assert_eq!(annotation.symbol, None);
    assert!(annotation.ink_paths.is_empty());
}

#[test]
fn caret_annotation_reads_symbol_and_absent_entries_stay_none() {
    let dict = base_annotation("Caret").insert("Symbol", Value::Name("NewLine".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("caret annotation");
    assert_eq!(annotation.symbol.as_deref(), Some("NewLine"));
    assert!(annotation.icon_name.is_none());
    assert!(annotation.is_open.is_none());
    assert!(annotation.state.is_none());
    assert!(annotation.state_model.is_none());

    // `/Symbol` 也可写成文本串（真实文件偶见），按文本解码。
    let loose = base_annotation("Caret").insert("Symbol", Value::String("Cross".into()));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &loose).expect("loose symbol");
    assert_eq!(annotation.symbol.as_deref(), Some("Cross"));
}

#[test]
fn ink_annotation_reads_ink_list_paths() {
    let dict = base_annotation("Ink").insert(
        "InkList",
        Value::Array(Array(vec![
            Value::Array(Array(vec![
                Value::Integer(10),
                Value::Float(20.5),
                Value::Integer(30),
                Value::Float(40.0),
            ])),
            Value::Array(Array(vec![
                Value::Float(1.5),
                Value::Float(2.5),
                Value::Integer(3),
                Value::Integer(4),
                Value::Integer(5),
                Value::Integer(6),
            ])),
        ])),
    );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("ink annotation");
    assert_eq!(
        annotation.ink_paths,
        vec![
            vec![10.0, 20.5, 30.0, 40.0],
            vec![1.5, 2.5, 3.0, 4.0, 5.0, 6.0]
        ]
    );
}

#[test]
fn line_annotation_reads_all_table_175_entries() {
    let dict = base_annotation("Line")
        .insert(
            "L",
            Value::Array(Array(vec![
                Value::Integer(72),
                Value::Float(700.5),
                Value::Integer(300),
                Value::Integer(650),
            ])),
        )
        .insert(
            "LE",
            Value::Array(Array(vec![
                Value::Name("ClosedArrow".into()),
                Value::Name("OpenArrow".into()),
            ])),
        )
        .insert(
            "IC",
            Value::Array(Array(vec![Value::Float(1.0), Value::Integer(0)])),
        )
        .insert("LL", Value::Float(12.5))
        .insert("LLE", Value::Integer(3))
        .insert("Cap", Value::Bool(true))
        .insert("IT", Value::Name("LineDimension".into()))
        .insert("LLO", Value::Float(6.0))
        .insert("CP", Value::Name("Top".into()))
        .insert(
            "CO",
            Value::Array(Array(vec![Value::Float(-1.5), Value::Integer(20)])),
        );
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("line annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Line);
    assert_eq!(annotation.line_endpoints, Some([72.0, 700.5, 300.0, 650.0]));
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::ClosedArrow, LineEndStyle::OpenArrow])
    );
    assert_eq!(annotation.interior_color, Some(vec![1.0, 0.0]));
    assert_eq!(annotation.leader_length, Some(12.5));
    assert_eq!(annotation.leader_extension, Some(3.0));
    assert_eq!(annotation.caption, Some(true));
    assert_eq!(annotation.line_intent, Some(LineIntent::LineDimension));
    assert_eq!(annotation.leader_line_offset, Some(6.0));
    assert_eq!(annotation.caption_position, Some(CaptionPosition::Top));
    assert_eq!(annotation.caption_offset, Some([-1.5, 20.0]));
}

#[test]
fn line_annotation_fills_absent_arrow_end_style_and_tolerates_unknown_names() {
    let document = doc(vec![catalog_object()]);

    // `/LE` 只写一端：另一端按 Table 175 的缺省 `/None` 补齐。
    let dict =
        base_annotation("Line").insert("LE", Value::Array(Array(vec![Value::Name("Butt".into())])));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("single arrow ending");
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::Butt, LineEndStyle::None])
    );

    // 非标准名保留原文；端点样式、意图与题注位置各自宽容读取。
    let dict = base_annotation("Line")
        .insert(
            "LE",
            Value::Array(Array(vec![
                Value::Name("HexArrow".into()),
                Value::Name("Slash".into()),
            ])),
        )
        .insert("IT", Value::Name("LineCloud".into()))
        .insert("CP", Value::String("Below".into()));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("unknown names");
    assert_eq!(
        annotation.line_endings,
        Some([LineEndStyle::Other("HexArrow".into()), LineEndStyle::Slash])
    );
    assert_eq!(
        annotation.line_intent,
        Some(LineIntent::Other("LineCloud".into()))
    );
    assert_eq!(
        annotation.caption_position,
        Some(CaptionPosition::Other("Below".into()))
    );
    assert!(
        annotation.line_endpoints.is_none(),
        "/L 缺失时不报错，归属校验由写端负责"
    );
    assert!(annotation.leader_length.is_none());
    assert_eq!(annotation.caption, None);
}

#[test]
fn malformed_line_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    let cases = vec![
        // /L 项数与类型
        base_annotation("Line").insert(
            "L",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])),
        ),
        base_annotation("Line").insert("L", Value::Integer(4)),
        base_annotation("Line").insert(
            "L",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
                Value::Float(f64::NAN),
            ])),
        ),
        // /LE 项数与名字
        base_annotation("Line").insert("LE", Value::Array(Array(Vec::new()))),
        base_annotation("Line").insert(
            "LE",
            Value::Array(Array(vec![
                Value::Name("Square".into()),
                Value::Name("Circle".into()),
                Value::Name("Diamond".into()),
            ])),
        ),
        base_annotation("Line").insert(
            "LE",
            Value::Array(Array(vec![Value::Integer(1), Value::Integer(2)])),
        ),
        // /Cap 必须是布尔
        base_annotation("Line").insert("Cap", Value::Integer(1)),
        // /LL /LLE /LLO 必须是数值
        base_annotation("Line").insert("LL", Value::Name("long".into())),
        base_annotation("Line").insert("LLE", Value::String("3".into())),
        base_annotation("Line").insert("LLO", Value::Array(Array(vec![Value::Integer(1)]))),
        // /CO 恰好两项
        base_annotation("Line").insert(
            "CO",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])),
        ),
    ];
    for dict in cases {
        assert!(
            matches!(
                parse_annotation(&document, 0, None, &dict),
                Err(AnnotationError::Malformed(_))
            ),
            "expect malformed for {dict:?}"
        );
    }
}

#[test]
fn malformed_subtype_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    let cases = vec![
        base_annotation("Text").insert("Open", Value::Integer(1)),
        base_annotation("Text").insert("State", Value::Integer(2)),
        base_annotation("Caret").insert("Symbol", Value::Integer(0)),
        base_annotation("Ink").insert("InkList", Value::Integer(3)),
        base_annotation("Ink").insert("InkList", Value::Array(Array(vec![Value::Integer(1)]))),
        base_annotation("Ink").insert(
            "InkList",
            Value::Array(Array(vec![Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ]))])),
        ),
        base_annotation("Ink").insert(
            "InkList",
            Value::Array(Array(vec![Value::Array(Array(vec![
                Value::Integer(1),
                Value::Float(f64::NAN),
            ]))])),
        ),
    ];
    for dict in cases {
        assert!(
            matches!(
                parse_annotation(&document, 0, None, &dict),
                Err(AnnotationError::Malformed(_))
            ),
            "expect malformed for {dict:?}"
        );
    }
}
