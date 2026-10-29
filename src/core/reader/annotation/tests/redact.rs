//! Redact 注解专有条目读取（12.5.6.23，Table 192）：`/OverlayText`、
//! `/Repeat`、`/RO` 与共用的 `/DA` `/Q` `/IC`。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, StreamData, Value};

use super::support::*;

#[test]
fn redact_annotation_reads_overlay_entries() {
    let dict = base_annotation("Redact")
        .insert("OverlayText", Value::String("REDACTED".into()))
        .insert("DA", Value::String("/F1 12 Tf".into()))
        .insert("Q", Value::Integer(1))
        .insert("Repeat", Value::Bool(true))
        .insert(
            "IC",
            Value::Array(Array(vec![
                Value::Float(0.25),
                Value::Float(0.5),
                Value::Float(0.75),
            ])),
        );
    let document = doc(vec![catalog_object()]);
    let annotation = parse_annotation(&document, 0, None, &dict).expect("redact annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Redact);
    assert_eq!(annotation.overlay_text.as_deref(), Some("REDACTED"));
    assert_eq!(annotation.overlay_appearance, None);
    assert_eq!(annotation.default_appearance.as_deref(), Some("/F1 12 Tf"));
    assert_eq!(annotation.quadding, Some(1));
    assert_eq!(annotation.repeat_overlay_text, Some(true));
    assert_eq!(
        annotation.interior_color,
        Some(vec![0.25, 0.5, 0.75]),
        "Table 192 的 /IC 与通用背景色条目同键，复用 interior_color"
    );
}

#[test]
fn redact_overlay_appearance_keeps_the_form_xobject_value() {
    let document = doc(vec![catalog_object()]);
    let referenced = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Redact").insert("RO", Value::Ref(9, 0)),
    )
    .expect("/RO 间接引用");
    assert_eq!(referenced.overlay_appearance, Some(Value::Ref(9, 0)));

    let inline = base_annotation("Redact").insert(
        "RO",
        Value::Stream(
            Dict::new().insert("Subtype", Value::Name("Form".into())),
            StreamData::Owned(Vec::new()),
        ),
    );
    let direct = parse_annotation(&document, 0, None, &inline).expect("/RO 直接流");
    assert!(
        matches!(
            direct.overlay_appearance,
            Some(Value::Stream(ref stream, _)) if stream.get("Subtype") == Some(&Value::Name("Form".into()))
        ),
        "直接流形态的 /RO 原样保留"
    );

    // 缺省：未声明的条目一律 `None`，规范缺省值由调用方应用。
    let plain = parse_annotation(&document, 0, None, &base_annotation("Redact")).expect("plain");
    assert!(plain.overlay_text.is_none());
    assert!(plain.repeat_overlay_text.is_none());
    assert!(plain.overlay_appearance.is_none());
}

#[test]
fn malformed_redact_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);

    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Redact").insert("Repeat", Value::Integer(1)),
    )
    .expect_err("/Repeat integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/Repeat")),
        "{error:?}"
    );

    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Redact").insert("RO", Value::Integer(4)),
    )
    .expect_err("/RO integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/RO")),
        "{error:?}"
    );

    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Redact").insert("OverlayText", Value::Integer(42)),
    )
    .expect_err("/OverlayText integer");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/OverlayText")),
        "{error:?}"
    );
}
