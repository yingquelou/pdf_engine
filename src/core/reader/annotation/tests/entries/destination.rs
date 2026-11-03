//! 注解 `/Dest` 目的地：数组、PDF 1.1 的 `/D` 字典与命名目标经目录 `/Dests`
//! 名称树解析为导航层目标，及非法 `/Dest` 的类型化错误。

use crate::core::reader::annotation::*;
use crate::core::types::{Array, Dict, Object, Value};

use super::super::support::*;

/// `/Dest` 目的地数组、PDF 1.1 的 `/D` 字典与命名目标均解析为导航层目标模型。
#[test]
fn annotation_destination_resolves_array_dictionary_and_named_forms() {
    use crate::core::reader::navigation::{Destination, OutlineDestination};

    let array = Value::Array(Array(vec![
        Value::Ref(3, 0),
        Value::Name("XYZ".into()),
        Value::Null,
        Value::Null,
        Value::Integer(0),
    ]));
    let explicit = Value::Dict(base_annotation("Link").insert("Dest", array.clone()));
    let legacy = Value::Dict(
        base_annotation("Link").insert("Dest", Value::Dict(Dict::new().insert("D", array))),
    );
    let named = Value::Dict(base_annotation("Link").insert("Dest", Value::Name("intro".into())));
    let absent = Value::Dict(base_annotation("Link"));
    let document = doc_with_annots(vec![explicit, legacy, named, absent], Vec::new());
    let annotations = document.page_annotations(0).expect("annotations");

    let expected = OutlineDestination::Explicit(Destination::Xyz {
        page: 0,
        left: None,
        top: None,
        zoom: Some(0.0),
    });
    for index in 0..2 {
        assert_eq!(
            document
                .annotation_destination(&annotations[index])
                .expect("destination parses"),
            Some(expected.clone()),
            "array form and /D dictionary form must resolve identically"
        );
    }
    assert_eq!(
        document
            .annotation_destination(&annotations[2])
            .expect("named destination parses"),
        Some(OutlineDestination::Named("intro".into()))
    );
    assert_eq!(
        document
            .annotation_destination(&annotations[3])
            .expect("annotation without /Dest"),
        None
    );
}

/// 命名目标可继续经目录 `/Dests` 名称树查得实际页面目标（12.3.2.1）。
#[test]
fn annotation_named_destination_key_resolves_through_dests_tree() {
    use crate::core::reader::navigation::{Destination, OutlineDestination};

    let dests = Value::Dict(Dict::new().insert(
        "Names",
        Value::Array(Array(vec![
            Value::String("intro".into()),
            Value::Array(Array(vec![Value::Ref(3, 0), Value::Name("Fit".into())])),
        ])),
    ));
    let catalog = Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("Names", Value::Dict(Dict::new().insert("Dests", dests))),
        ),
    );
    let document = doc_with_annots(
        vec![Value::Dict(
            base_annotation("Link").insert("Dest", Value::String("intro".into())),
        )],
        vec![catalog],
    );
    // 自定义 catalog 占用对象号 1，与默认辅助冲突时以最后一份为准。
    let annotation = &document.page_annotations(0).expect("annotations")[0];
    let target = document
        .annotation_destination(annotation)
        .expect("destination parses")
        .expect("/Dest present");
    assert_eq!(target, OutlineDestination::Named("intro".into()));
    assert_eq!(
        document.named_destination("intro").expect("name tree"),
        Some(Destination::Fit { page: 0 })
    );
}

/// 非法 `/Dest`（类型错误、缺 `/D`、页面不在树中）返回类型化错误。
#[test]
fn malformed_annotation_destinations_are_typed_errors() {
    let cases = vec![
        base_annotation("Link").insert("Dest", Value::Integer(3)),
        base_annotation("Link").insert("Dest", Value::Dict(Dict::new())),
        base_annotation("Link").insert(
            "Dest",
            Value::Array(Array(vec![Value::Ref(99, 0), Value::Name("Fit".into())])),
        ),
        base_annotation("Link").insert(
            "Dest",
            Value::Array(Array(vec![Value::Ref(3, 0), Value::Name("Bogus".into())])),
        ),
    ];
    let document = doc_with_annots(cases.iter().cloned().map(Value::Dict).collect(), Vec::new());
    let annotations = document.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 4);
    for annotation in &annotations {
        let error = document
            .annotation_destination(annotation)
            .expect_err("malformed /Dest must error");
        assert!(
            matches!(
                error,
                AnnotationError::Malformed(_) | AnnotationError::PageNotFound
            ),
            "unexpected error {error:?}"
        );
    }
}
