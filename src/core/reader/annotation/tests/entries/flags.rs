//! 注解 `/F` 标志位解码：标准隐藏/不旋转/锁定等位的读取、缺省与
//! `no-view` 可见性，以及非整数标志的类型化错误。

use crate::core::reader::annotation::*;
use crate::core::types::Value;

use super::super::support::*;

#[test]
fn annotation_flags_decode_standard_bits() {
    let annotation = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Text").insert("F", Value::Integer(0b10_1001_0010)),
    )
    .expect("flags parse");
    assert!(annotation.flags.hidden());
    assert!(annotation.flags.no_rotate());
    assert!(annotation.flags.locked());
    assert!(annotation.flags.locked_contents());
    assert!(!annotation.flags.no_view());
    assert!(!annotation.flags.read_only());
    assert!(!annotation.visible());

    let default_flags = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Text"),
    )
    .expect("defaults");
    assert_eq!(default_flags.flags, AnnotationFlags::default());
    assert!(default_flags.visible());

    let invisible = parse_annotation(
        &doc(vec![catalog_object()]),
        0,
        None,
        &base_annotation("Text").insert("F", Value::Integer(AnnotationFlags::NO_VIEW as i64)),
    )
    .expect("no-view");
    assert!(!invisible.visible());

    let bad_flags = base_annotation("Text").insert("F", Value::Name("hidden".into()));
    assert!(matches!(
        parse_annotation(&doc(vec![]), 0, None, &bad_flags),
        Err(AnnotationError::Malformed(_))
    ));
}
