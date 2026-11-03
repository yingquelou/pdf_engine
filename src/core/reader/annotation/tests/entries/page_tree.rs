//! 页面树注解读取与错误契约：直接/间接 `Annots` 条目、空页与页越界、
//! 数量上限、父子环与深度、以及非字典条目引用的类型化错误。

use crate::core::reader::annotation::*;
use crate::core::types::{Dict, Object, Value};

use super::super::support::*;

#[test]
fn reads_direct_and_indirect_page_annotations() {
    let direct = Value::Dict(base_annotation("Caret"));
    let indirect = Value::Dict(
        base_annotation("Stamp")
            .insert("Contents", Value::String("rubber stamp".into()))
            .insert("NM", Value::String("annot-7".into())),
    );
    let document = doc_with_annots(
        vec![direct, Value::Ref(9, 0)],
        vec![Object::new(9, 0, indirect)],
    );
    let annotations = document.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);
    assert_eq!(annotations[0].subtype, AnnotationSubtype::Caret);
    assert_eq!(annotations[0].reference, None);
    assert_eq!(annotations[0].page_index, 0);
    assert_eq!(annotations[0].rect, [10.0, 20.5, 30.0, 40.0]);
    assert_eq!(annotations[1].subtype, AnnotationSubtype::Stamp);
    assert_eq!(annotations[1].reference, Some((9, 0)));
    assert_eq!(
        annotations[1].contents,
        Some(Value::String("rubber stamp".into()))
    );
    assert_eq!(annotations[1].name, Some(Value::String("annot-7".into())));
}

#[test]
fn pages_without_annots_are_empty_and_page_index_is_checked() {
    let document = doc(vec![
        catalog_object(),
        pages_object(vec![Value::Ref(3, 0)]),
        page_object(None),
    ]);
    assert!(
        document
            .page_annotations(0)
            .expect("page exists")
            .is_empty()
    );
    assert_eq!(
        document.page_annotations(1),
        Err(AnnotationError::PageNotFound)
    );
}

#[test]
fn annotation_count_limit_is_enforced() {
    let annots: Vec<Value> = (0..MAX_ANNOTATIONS_PER_PAGE + 1)
        .map(|_| Value::Dict(base_annotation("Text")))
        .collect();
    let document = doc_with_annots(annots, Vec::new());
    assert_eq!(
        document.page_annotations(0),
        Err(AnnotationError::LimitExceeded("annotations per page"))
    );
}

#[test]
fn page_tree_cycles_and_depth_are_typed_errors() {
    // 环：Pages 节点 2 的子节点指回自身。
    let cyclic = doc(vec![
        catalog_object(),
        pages_object(vec![Value::Ref(2, 0)]),
        page_object(None),
    ]);
    assert!(matches!(
        cyclic.page_annotations(0),
        Err(AnnotationError::Malformed(_))
    ));

    // 非父子形状：/Kids 不是数组。
    let bad_kids = doc(vec![
        catalog_object(),
        Object::new(
            2,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Pages".into()))
                    .insert("Kids", Value::Integer(1)),
            ),
        ),
        page_object(None),
    ]);
    assert!(matches!(
        bad_kids.page_annotations(0),
        Err(AnnotationError::Malformed(_))
    ));

    // 目录缺失。
    let no_catalog = doc(vec![
        pages_object(vec![Value::Ref(3, 0)]),
        page_object(None),
    ]);
    assert_eq!(
        no_catalog.page_annotations(0),
        Err(AnnotationError::CatalogMissing)
    );
}

#[test]
fn annotation_entry_referencing_non_dictionary_is_malformed() {
    let document = doc_with_annots(
        vec![Value::Ref(9, 0)],
        vec![Object::new(9, 0, Value::Integer(4))],
    );
    assert!(matches!(
        document.page_annotations(0),
        Err(AnnotationError::Malformed(_))
    ));
}
