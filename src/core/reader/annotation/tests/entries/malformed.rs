//! 注解字典形态的类型化错误：缺 `/Type`、`/Type` 非 Annot、缺 `/Subtype`、
//! `/Rect` 缺项或含名字、`/Contents`/`/AS` 非文本/名称等。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Value};

use super::super::support::*;

#[test]
fn malformed_annotation_dictionaries_are_typed_errors() {
    let document = doc(vec![]);
    let cases = vec![
        // 缺 /Type
        Dict::new().insert("Subtype", Value::Name("Text".into())),
        // /Type 非 Annot
        base_annotation("Text").insert("Type", Value::Name("Annot2".into())),
        // 缺 /Subtype
        Dict::new()
            .insert("Type", Value::Name("Annot".into()))
            .insert("Rect", Value::Array(Array(vec![Value::Integer(0); 4]))),
        // /Rect 缺项
        Dict::new()
            .insert("Type", Value::Name("Annot".into()))
            .insert("Subtype", Value::Name("Text".into()))
            .insert("Rect", Value::Array(Array(vec![Value::Integer(0); 3]))),
        // /Rect 含名称
        Dict::new()
            .insert("Type", Value::Name("Annot".into()))
            .insert("Subtype", Value::Name("Text".into()))
            .insert(
                "Rect",
                Value::Array(Array(vec![
                    Value::Integer(0),
                    Value::Integer(0),
                    Value::Name("wide".into()),
                    Value::Integer(0),
                ])),
            ),
        // /Contents 非文本串
        base_annotation("Text").insert("Contents", Value::Dict(Dict::new())),
        // /AS 非名称
        base_annotation("Text").insert("AS", Value::Integer(1)),
    ];
    for case in cases {
        assert!(
            matches!(
                parse_annotation(&document, 0, None, &case),
                Err(AnnotationError::Malformed(_))
            ),
            "expected malformed error for {case:?}"
        );
    }
}
