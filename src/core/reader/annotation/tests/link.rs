//! Link 注解专有条目读取（12.5.6.5，Table 173）：`/H` 与 `/PA`。

use crate::core::reader::action::Action;
use crate::core::reader::annotation::*;
use crate::core::types::{Dict, Object, Value};

use super::support::*;

#[test]
fn link_annotation_reads_highlight_mode_and_pressed_action() {
    let pressed = Dict::new()
        .insert("S", Value::Name("URI".into()))
        .insert("URI", Value::String("https://example.com/legacy".into()));
    let dict = base_annotation("Link")
        .insert("H", Value::Name("O".into()))
        .insert("PA", Value::Ref(4, 0));
    let document = doc(vec![
        catalog_object(),
        Object::new(4, 0, Value::Dict(pressed)),
    ]);
    let annotation = parse_annotation(&document, 0, None, &dict).expect("link annotation");
    assert_eq!(annotation.link_highlight, Some(LinkHighlightMode::Outline));
    assert_eq!(annotation.pressed_action, Some(Value::Ref(4, 0)));
    assert_eq!(
        document
            .annotation_pressed_action(&annotation)
            .expect("pressed action"),
        Some(Action::Uri("https://example.com/legacy".into()))
    );

    // 缺省：`/H` 的规范缺省值 `Invert` 由渲染端应用，读端保留 `None`。
    let plain = parse_annotation(&document, 0, None, &base_annotation("Link")).expect("plain link");
    assert!(plain.link_highlight.is_none());
    assert!(plain.pressed_action.is_none());
    assert!(
        document
            .annotation_pressed_action(&plain)
            .expect("absent /PA")
            .is_none()
    );

    // 非标准高亮模式名保留原文。
    let unknown = base_annotation("Link").insert("H", Value::Name("Blink".into()));
    let annotation = parse_annotation(&document, 0, None, &unknown).expect("unknown highlight");
    assert_eq!(
        annotation.link_highlight,
        Some(LinkHighlightMode::Other("Blink".into()))
    );
    assert_eq!(
        LinkHighlightMode::Other("Blink".into()).name(),
        "Blink",
        "Other 变体保留原文，写完再读可往返"
    );
}

#[test]
fn malformed_link_entries_are_typed_errors() {
    // `/H` 必须是名字。
    let error = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Link").insert("H", Value::Integer(1)),
    )
    .expect_err("/H integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/H")),
        "{error:?}"
    );

    // `/PA` 非字典：读端原样保留，动作解析时才报类型化错误。
    let document = doc(vec![catalog_object()]);
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Link").insert("PA", Value::Integer(7)),
    )
    .expect("raw /PA is retained");
    let error = document
        .annotation_pressed_action(&annotation)
        .expect_err("non-dict /PA");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("action")),
        "{error:?}"
    );

    // `/PA` 动作字典缺 `/S` 同样是 Malformed。
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Link").insert("PA", Value::Dict(Dict::new())),
    )
    .expect("raw /PA is retained");
    let error = document
        .annotation_pressed_action(&annotation)
        .expect_err("missing /S");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/S")),
        "{error:?}"
    );
}
