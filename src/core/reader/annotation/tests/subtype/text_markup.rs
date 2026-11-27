//! Text/Caret/Popup/Ink 子类型专有条目读取，以及跨类的类型化 malformed 错误。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Value};

use super::super::support::*;

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
fn popup_annotation_reads_parent_reference_and_open_flag() {
    let dict = base_annotation("Popup")
        .insert("Parent", Value::Ref(9, 0))
        .insert("Open", Value::Bool(true));
    let annotation =
        parse_annotation(&doc(vec![catalog_object()]), 0, None, &dict).expect("popup annotation");
    assert_eq!(annotation.parent, Some((9, 0)));
    assert_eq!(annotation.is_open, Some(true));

    let plain = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Popup"),
    )
    .expect("popup without /Parent");
    assert!(plain.parent.is_none());
    assert!(plain.is_open.is_none());

    let bad_parent = base_annotation("Popup").insert("Parent", Value::Dict(Dict::new()));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_parent),
        Err(AnnotationError::Malformed(_))
    ));
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

#[test]
fn text_annotation_standard_state_classifies_table_171_combinations() {
    let doc = doc(vec![catalog_object()]);
    let state = |model: Option<&str>, value: Option<&str>| {
        let mut dict = base_annotation("Text");
        if let Some(model) = model {
            dict = dict.insert("StateModel", Value::String(model.into()));
        }
        if let Some(value) = value {
            dict = dict.insert("State", Value::String(value.into()));
        }
        parse_annotation(&doc, 0, None, &dict)
            .expect("annotation")
            .standard_state()
    };

    assert_eq!(
        state(Some("Review"), Some("Accepted")),
        Some((
            StandardStateModel::Review,
            StandardAnnotationState::Accepted
        ))
    );
    assert_eq!(
        state(Some("Marked"), Some("Unmarked")),
        Some((
            StandardStateModel::Marked,
            StandardAnnotationState::Unmarked
        ))
    );
    // 缺省模型为 Marked；缺省状态由 Table 171 给出。
    assert_eq!(
        StandardStateModel::Marked.default_state(),
        StandardAnnotationState::Unmarked
    );
    assert_eq!(
        StandardStateModel::Review.default_state(),
        StandardAnnotationState::None
    );
    // 跨模型组合、自定义模型、自定义状态与缺 /State 均不分类，原始串保留。
    assert_eq!(state(Some("Marked"), Some("Accepted")), None);
    assert_eq!(state(Some("Custom"), Some("Accepted")), None);
    assert_eq!(state(Some("Review"), Some("CustomState")), None);
    assert_eq!(state(Some("Review"), None), None);

    let cross = parse_annotation(
        &doc,
        0,
        None,
        &base_annotation("Text")
            .insert("State", Value::String("Accepted".into()))
            .insert("StateModel", Value::String("Marked".into())),
    )
    .expect("annotation");
    assert_eq!(cross.state.as_deref(), Some("Accepted"));
    assert_eq!(cross.state_model.as_deref(), Some("Marked"));
}
