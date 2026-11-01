//! 显式关联档（14.8.2.3.2 第二句）：`Annot`（PDF 1.5）/`Link`/`Form` 元素
//! 明示「一个标记内容序列 ↔ 一个注解」的对应关系。

use super::support::*;
use crate::core::reader::accessibility::{AnnotationOrderBasis, ContentOrderItem};
use crate::core::types::{Dict, Value};

#[test]
fn annot_element_places_annotation_after_its_marked_content_sequence() {
    // `/Annot` 元素 `/K [MCR 0, OBJR]`：显式关联，OBJR 在区段子项之后。
    let doc = two_sequences_and_one_annotation(vec![Value::Ref(FIRST, 0)]);
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Annot", DOC, vec![mcr(0), objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(
        shape(&items),
        vec!["MC0", "A0", "MC1"],
        "显式关联应把注解贴在关联区段之后"
    );
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::ExplicitAssociation, Some(0))],
        "依据应是 Annot 元素的显式关联"
    );
}

#[test]
fn link_element_before_marked_content_sequence_places_annotation_first() {
    // `/Link` 元素 `/K [OBJR, MCR 1]`：显式关联，OBJR 在区段子项之前。
    let doc = two_sequences_and_one_annotation(vec![Value::Ref(FIRST, 0)]);
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Link", DOC, vec![objr(ANNOT), mcr(1)])),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "A0", "MC1"]);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::ExplicitAssociation, Some(1))],
        "关联的区段是 OBJR 之后的 MCID 1"
    );
    assert!(items[1].basis().unwrap().is_explicit());
    assert_eq!(items[0].section(), Some(0));
    assert_eq!(items[1].annotation(), Some(0));
}

#[test]
fn form_element_without_own_sequence_falls_back_to_adjacency() {
    // `/Form` 元素只有 OBJR、子树里也没有区段：显式关联落空，退到邻接档。
    let doc = two_sequences_and_one_annotation(vec![Value::Ref(FIRST, 0), Value::Ref(SECOND, 0)]);
    insert(&doc, FIRST, Value::Dict(element("P", DOC, vec![mcr(0)])));
    insert(
        &doc,
        SECOND,
        Value::Dict(element("Form", DOC, vec![objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "A0", "MC1"]);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::AdjacentFollows, Some(0))],
        "Form 元素本身没有可关联的序列时应按邻接在后处理"
    );
}

#[test]
fn role_mapped_annot_element_is_treated_as_explicit_association() {
    // `/RoleMap` 把用户类型映到 `Annot`：显式关联档按生效标准类型判定。
    let root = Dict::new()
        .insert("Type", name("StructTreeRoot"))
        .insert(
            "RoleMap",
            Value::Dict(Dict::new().insert("Anno", name("Annot"))),
        )
        .insert("K", arr(vec![Value::Ref(DOC, 0)]));
    let doc = two_page(Some(root), &content(&[0, 1]), &[ANNOT], &[], &[]);
    insert(
        &doc,
        DOC,
        Value::Dict(root_element("Document", vec![Value::Ref(FIRST, 0)])),
    );
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Anno", DOC, vec![mcr(0), objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "A0", "MC1"]);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::ExplicitAssociation, Some(0))]
    );
}

#[test]
fn integer_mcid_shorthand_with_ancestor_page_claims_sequence() {
    // 整数 MCID 简写沿用元素 `/Pg`；元素自身没有 `/Pg` 时沿父链取声明。
    let doc = two_sequences_and_one_annotation(vec![Value::Ref(FIRST, 0)]);
    insert(
        &doc,
        DOC,
        Value::Dict(
            root_element("Document", vec![Value::Ref(FIRST, 0)]).insert("Pg", Value::Ref(PAGE, 0)),
        ),
    );
    insert(
        &doc,
        FIRST,
        Value::Dict(element_unpaged(
            "Link",
            DOC,
            vec![Value::Integer(0), objr(ANNOT)],
        )),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "A0", "MC1"]);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::ExplicitAssociation, Some(0))],
        "祖先声明的页应让整数 MCID 简写与 OBJR 都落在本页"
    );
}

#[test]
fn stream_marked_content_reference_is_not_page_content() {
    // MCR 带 `/Stm` 时序列在表单 XObject 里，不参与本页排序与邻接判定。
    let doc = one_page_document(
        vec![Value::Ref(FIRST, 0), Value::Ref(SECOND, 0)],
        &content(&[0]),
        &[ANNOT],
    );
    insert(&doc, FIRST, Value::Dict(element("P", DOC, vec![mcr(0)])));
    insert(
        &doc,
        SECOND,
        Value::Dict(element("Note", DOC, vec![mcr_in_stream(1), objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "A0"]);
    let ContentOrderItem::Annotation(position) = items.last().expect("至少两条") else {
        panic!("末尾应是注解");
    };
    assert_eq!(
        position.basis,
        AnnotationOrderBasis::AdjacentFollows,
        "只认领取表单流的元素不算区段元素，邻接仍取前一个 /P"
    );
    assert_eq!(position.associated_sequence, Some(0));
    assert_eq!(position.element, Some(2), "声称元素仍是 /Note");
}

#[test]
fn duplicated_object_reference_claims_annotation_once() {
    // 畸形写法：同一注解被两个元素声称，只认结构序中第一次。
    let doc = two_sequences_and_one_annotation(vec![Value::Ref(FIRST, 0), Value::Ref(SECOND, 0)]);
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Annot", DOC, vec![mcr(0), objr(ANNOT)])),
    );
    insert(
        &doc,
        SECOND,
        Value::Dict(element("Annot", DOC, vec![mcr(1), objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(bases(&items).len(), 1, "重复声称应被忽略");
    assert_eq!(
        items
            .iter()
            .filter_map(|item| match item {
                ContentOrderItem::Annotation(position) => Some(position.element),
                _ => None,
            })
            .next()
            .and_then(|element| element),
        Some(1),
        "落点来自 arena 下标 1 的元素（结构序中第一次声称）"
    );
}
