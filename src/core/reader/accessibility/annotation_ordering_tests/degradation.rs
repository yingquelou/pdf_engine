//! 跨页声称、没有结构树与畸形/环状写法的类型化降级（原则 6）。

use super::support::*;
use crate::core::reader::accessibility::{
    AnnotationOrderBasis, AnnotationOrderingError, ContentOrderItem,
};
use crate::core::types::Value;

#[test]
fn element_declared_for_another_page_leaves_its_sequence_unclaimed() {
    // 元素的 `/Pg` 指向别页、MCR 自身不写 `/Pg`：Table 324 的继承让该序列落在
    // 他页，本页区段条目只剩「未被结构认领」。
    let doc = one_page_document(vec![Value::Ref(FIRST, 0)], &content(&[0, 1]), &[ANNOT]);
    insert(
        &doc,
        FIRST,
        Value::Dict(element_on("P", DOC, PAGE2, vec![mcr_unpaged(0)])),
    );
    let items = order(&doc, 0);
    assert_eq!(
        shape(&items),
        vec!["MC0", "MC1", "A0"],
        "他页元素认领的序列在本页仍是区段，只是没有结构落点"
    );
    let ContentOrderItem::MarkedContent(first) = &items[0] else {
        panic!("首条应是区段");
    };
    assert_eq!(first.element, None, "他页元素的认领不落到本页");
    assert!(!first.positioning_anchor);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::Undetermined, None)],
        "本页注解没有结构声称时仍是 `Undetermined`"
    );
}

#[test]
fn annotation_claimed_for_another_page_is_not_interleaved() {
    // OBJR 的 `/Pg` 覆盖元素的 `/Pg`：声称落在页 1 的注解不参与页 0 排序。
    let doc = two_page(
        Some(tree(vec![Value::Ref(DOC, 0)])),
        &content(&[0]),
        &[ANNOT],
        &content(&[0]),
        &[],
    );
    insert(
        &doc,
        DOC,
        Value::Dict(root_element("Document", vec![Value::Ref(FIRST, 0)])),
    );
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Note", DOC, vec![objr_on(PAGE2, ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(
        shape(&items),
        vec!["MC0", "A0"],
        "他页声称的注解不应插在页内序列里"
    );
    let ContentOrderItem::Annotation(position) = &items[1] else {
        panic!("末尾条目应是注解");
    };
    assert_eq!(position.basis, AnnotationOrderBasis::Undetermined);
    assert_eq!(position.element, None, "本页没有对该注解的结构声称");
}

#[test]
fn document_without_structure_tree_yields_empty_content_order() {
    let doc = document(None, &content(&[0]), &[ANNOT]);
    assert_eq!(order(&doc, 0), vec![], "没有逻辑结构就没有可导出的位置");
}

#[test]
fn unknown_page_index_returns_typed_page_error() {
    let doc = document(Some(tree(vec![])), &content(&[0]), &[ANNOT]);
    let error = doc
        .annotation_content_order(9)
        .expect_err("越界页码应是类型化错误");
    assert!(matches!(error, AnnotationOrderingError::Page(_)));
}

#[test]
fn cyclic_structure_reference_returns_typed_error() {
    // 元素 `/K` 自指：结构树解析报环，读端把失败原样上抛，绝不 panic。
    let doc = document(
        Some(tree(vec![Value::Ref(DOC, 0)])),
        &content(&[0]),
        &[ANNOT],
    );
    insert(
        &doc,
        DOC,
        Value::Dict(root_element("Document", vec![Value::Ref(FIRST, 0)])),
    );
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Annot", DOC, vec![Value::Ref(FIRST, 0)])),
    );
    let error = doc
        .annotation_content_order(0)
        .expect_err("环状结构应报类型化错误");
    assert!(matches!(error, AnnotationOrderingError::Structure(_)));
}

#[test]
fn corrupt_annots_array_returns_typed_annotation_error() {
    let doc = document(
        Some(tree(vec![Value::Ref(DOC, 0)])),
        &content(&[0]),
        &[ANNOT],
    );
    insert(&doc, DOC, Value::Dict(root_element("Document", vec![])));
    // `/Annots` 里混入非字典条目，注解读取失败。
    insert(
        &doc,
        PAGE,
        Value::Dict(page_dict(&[], &content(&[0])).insert("Annots", arr(vec![Value::Integer(3)]))),
    );
    let error = doc
        .annotation_content_order(0)
        .expect_err("损坏的 /Annots 应报类型化错误");
    assert!(matches!(error, AnnotationOrderingError::Annotations(_)));
}

#[test]
fn unclaimed_page_annotation_is_appended_as_undetermined() {
    // 页上两个注解，只有其中一个被结构声称。
    let doc = one_page_document(
        vec![Value::Ref(FIRST, 0)],
        &content(&[0, 1]),
        &[ANNOT, ANNOT2],
    );
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Annot", DOC, vec![mcr(0), objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "A0", "MC1", "A1"]);
    assert_eq!(
        bases(&items),
        vec![
            (0, AnnotationOrderBasis::ExplicitAssociation, Some(0)),
            (1, AnnotationOrderBasis::Undetermined, None),
        ],
        "未被声称的注解只有 `Undetermined` 一档，且不显示结构落点"
    );
    let ContentOrderItem::Annotation(position) = &items[3] else {
        panic!("末条应是未被声称的注解");
    };
    assert_eq!(position.kid, None, "没有 OBJR 子项可记录");
    assert_eq!(
        position.element, None,
        "未声称的注解同时展示 `AnnotationPosition::element` 为空"
    );
}

#[test]
fn ordering_error_display_carries_the_cause() {
    // 类型化错误带 `source`，调用方既能匹配变体也能读出根因。
    let doc = document(Some(tree(vec![])), &content(&[0]), &[ANNOT]);
    let error = doc
        .annotation_content_order(9)
        .expect_err("越界页码应是类型化错误");
    let text = error.to_string();
    assert!(
        text.contains("page content order"),
        "文案应说明是哪个输入: {text}"
    );
    assert!(
        std::error::Error::source(&error).is_some(),
        "错误应带上底层原因"
    );
}
