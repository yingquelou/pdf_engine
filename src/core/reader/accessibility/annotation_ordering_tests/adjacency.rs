//! 邻接档（14.8.2.3.2 第三句）与规范 NOTE 的空标记内容序列定位锚点。

use super::support::*;
use crate::core::reader::accessibility::{AnnotationOrderBasis, ContentOrderItem};
use crate::core::types::Value;

#[test]
fn structure_element_before_marked_content_element_precedes_it() {
    // 非关联型元素 `/Note` 带 OBJR，紧接其后的是带区段的 `/P`：注解在前。
    let doc = two_sequences_and_one_annotation(vec![Value::Ref(FIRST, 0), Value::Ref(SECOND, 0)]);
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Note", DOC, vec![objr(ANNOT)])),
    );
    insert(&doc, SECOND, Value::Dict(element("P", DOC, vec![mcr(0)])));
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["A0", "MC0", "MC1"]);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::AdjacentPrecedes, Some(0))],
        "14.8.2.3.2 的邻接句：注解元素在前则注解在序列之前"
    );
}

#[test]
fn structure_element_after_marked_content_element_follows_it() {
    // 紧邻在后：注解位于该序列之后。
    let doc = two_sequences_and_one_annotation(vec![Value::Ref(FIRST, 0), Value::Ref(SECOND, 0)]);
    insert(&doc, FIRST, Value::Dict(element("P", DOC, vec![mcr(0)])));
    insert(
        &doc,
        SECOND,
        Value::Dict(element("Note", DOC, vec![objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "A0", "MC1"]);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::AdjacentFollows, Some(0))]
    );
    assert!(
        bases(&items)[0].1.is_deterministic(),
        "邻接档是规范承认的位置，不该与 `Undetermined` 混同"
    );
    assert!(!items[1].basis().unwrap().is_explicit());
    assert_eq!(items[0].section(), Some(0), "首位是序列，注解在其后");
}

#[test]
fn empty_marked_content_sequence_anchors_adjacent_annotation() {
    // 规范 NOTE：写端放一段空标记内容序列只为定位相邻注解。
    let stream = b"/T << /MCID 0 >> BDC BT (x) Tj EMC\n/Anchor << /MCID 1 >> BDC EMC\n/T << /MCID 2 >> BDC BT (y) Tj EMC\n";
    let doc = one_page_document(
        vec![
            Value::Ref(FIRST, 0),
            Value::Ref(SECOND, 0),
            Value::Ref(THIRD, 0),
        ],
        stream,
        &[ANNOT],
    );
    insert(&doc, FIRST, Value::Dict(element("P", DOC, vec![mcr(0)])));
    insert(&doc, SECOND, Value::Dict(element("P", DOC, vec![mcr(1)])));
    insert(
        &doc,
        THIRD,
        Value::Dict(element("Note", DOC, vec![objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(
        shape(&items),
        vec!["MC0", "MC1", "A0", "MC2"],
        "空序列照样占位，注解紧接其后"
    );
    let ContentOrderItem::MarkedContent(position) = &items[1] else {
        panic!("第二条应是定位锚点");
    };
    assert!(position.positioning_anchor, "MCID 1 是空标记内容序列");
    assert_eq!(position.element, Some(2), "锚点由第二个元素认领");
    let ContentOrderItem::MarkedContent(first) = &items[0] else {
        panic!("第一条应是实内容区段");
    };
    assert!(!first.positioning_anchor);
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::AdjacentFollows, Some(1))],
        "锚点序列就是注解的定位依据"
    );
}

#[test]
fn marked_content_point_sequence_is_recognised_as_anchor() {
    // `MP`/`DP` 点本身就是空标记内容序列（14.6.2），同样识别为锚点。
    let stream = b"/T << /MCID 0 >> BDC BT (x) Tj EMC\n/Anchor <</MCID 1>> DP\n";
    let doc = one_page_document(
        vec![
            Value::Ref(FIRST, 0),
            Value::Ref(SECOND, 0),
            Value::Ref(THIRD, 0),
        ],
        stream,
        &[ANNOT],
    );
    insert(&doc, FIRST, Value::Dict(element("P", DOC, vec![mcr(0)])));
    insert(&doc, SECOND, Value::Dict(element("P", DOC, vec![mcr(1)])));
    insert(
        &doc,
        THIRD,
        Value::Dict(element("Note", DOC, vec![objr(ANNOT)])),
    );
    let items = order(&doc, 0);
    assert_eq!(shape(&items), vec!["MC0", "MC1", "A0"]);
    let ContentOrderItem::MarkedContent(anchor) = &items[1] else {
        panic!("第二条应是 DP 锚点");
    };
    assert!(anchor.positioning_anchor);
    assert_eq!(anchor.element, Some(2), "锚点由第二个元素认领");
    assert_eq!(anchor.kid, Some(0));
    assert_eq!(
        bases(&items),
        vec![(0, AnnotationOrderBasis::AdjacentFollows, Some(1))],
        "DP 点序列同样能给注解定位"
    );
}

#[test]
fn two_annotations_sharing_one_sequence_follow_annots_order() {
    // 同一元素的两个 OBJR 都邻接同一序列：按本页 `/Annots` 下标升序。
    let doc = one_page_document(
        vec![Value::Ref(FIRST, 0), Value::Ref(SECOND, 0)],
        &content(&[0, 1]),
        &[ANNOT2, ANNOT],
    );
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Note", DOC, vec![objr(ANNOT2), objr(ANNOT)])),
    );
    insert(&doc, SECOND, Value::Dict(element("P", DOC, vec![mcr(0)])));
    let items = order(&doc, 0);
    assert_eq!(
        shape(&items),
        vec!["A0", "A1", "MC0", "MC1"],
        "同槽位两个注解按 /Annots 下标升序（对象号 10 在 9 之前）"
    );
    assert_eq!(
        bases(&items),
        vec![
            (0, AnnotationOrderBasis::AdjacentPrecedes, Some(0)),
            (1, AnnotationOrderBasis::AdjacentPrecedes, Some(0)),
        ],
        "两个注解共享同一邻接落点"
    );
}

#[test]
fn intervening_annotation_element_breaks_adjacency() {
    // 注解元素与带区段的元素之间夹着另一个「只声称注解」的元素：
    // 邻接句的「紧接」不成立，位置退回无从判定。
    let doc = one_page_document(
        vec![Value::Ref(FIRST, 0), Value::Ref(SECOND, 0)],
        &content(&[0, 1]),
        &[ANNOT, ANNOT2],
    );
    insert(
        &doc,
        FIRST,
        Value::Dict(element("Note", DOC, vec![objr(ANNOT)])),
    );
    insert(
        &doc,
        SECOND,
        Value::Dict(element("Note", DOC, vec![objr(ANNOT2)])),
    );
    let items = order(&doc, 0);
    assert_eq!(
        shape(&items),
        vec!["MC0", "MC1", "A0", "A1"],
        "两侧都无邻接可依据时统一排在末尾"
    );
    assert_eq!(
        bases(&items),
        vec![
            (0, AnnotationOrderBasis::Undetermined, None),
            (1, AnnotationOrderBasis::Undetermined, None),
        ],
        "被声称但没有可紧邻的序列仍是 `Undetermined`"
    );
    let ContentOrderItem::Annotation(position) = &items[2] else {
        panic!("第三条应是注解");
    };
    assert_eq!(
        position.element,
        Some(1),
        "与未被任何元素声称的注解不同：这里仍记录声称元素"
    );
}
