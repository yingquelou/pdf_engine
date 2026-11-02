//! 注解目标的 OBJR 写出形态与父树联动（14.7.4.3 Table 325、14.7.4.4）：
//! `/K` 的 `<< /Type /OBJR /Pg /Obj >>` 字典、`/Obj` 指向注解间接对象、
//! `/Pg` 取注解所在页，对象引用内容项在父树里是裸间接引用（不是数组），
//! OBJR 键接在页面键之后连续分配，并与 MCR 子项/页面数组键混排。

use super::super::*;
use super::support::*;
use crate::core::reader::structure::{ParentTreeEntry, StructureKid as ReaderKid};
use crate::core::types::{PdfVersion, Value};

#[test]
fn object_reference_writes_objr_dict_and_struct_parent() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let annotation_ref = doc.page_annotations(0).expect("annotations")[0]
        .reference
        .expect("annotation is an indirect object");
    let figure = tree
        .elements()
        .iter()
        .find(|element| element.structure_type == "Figure")
        .expect("Figure element");
    match &figure.kids[..] {
        [ReaderKid::ObjectReference(objr)] => {
            assert_eq!(
                objr.object, annotation_ref,
                "/Obj 引用注解对象（Table 325）"
            );
            assert_eq!(
                objr.dict.get("Type"),
                Some(&Value::Name("OBJR".into())),
                "{:?}",
                objr.dict
            );
            let page_ref = objr.page.expect("/Pg 必须是间接引用");
            let page_dict = doc
                .get_object(page_ref.0)
                .expect("page lookup")
                .expect("page exists")
                .value
                .as_dict()
                .expect("page dict")
                .clone();
            assert_eq!(page_dict.get("Type"), Some(&Value::Name("Page".into())));
        }
        other => panic!("Figure 应只有一个 OBJR 子项，实际 {other:?}"),
    }
    let annotation_dict = &doc.page_annotations(0).expect("annotations")[0].dict;
    assert_eq!(
        annotation_dict.get("StructParent"),
        Some(&Value::Integer(0)),
        "无页面键时 OBJR 父树键为 0（Table 326）"
    );
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn object_reference_parent_tree_holds_single_reference_not_array() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let figure_index = tree
        .elements()
        .iter()
        .position(|element| element.structure_type == "Figure")
        .expect("Figure element");
    // 14.7.4.4：对象引用内容项的父树条目是裸间接引用，不是数组。
    match tree.parent_tree_entry(0) {
        Some(ParentTreeEntry::Object(index)) => {
            assert_eq!(*index, figure_index);
            assert_eq!(tree.object_parent(0), Some(figure_index));
        }
        other => panic!("父树键 0 应为单元条目，实际 {other:?}"),
    }
    assert!(tree.mcid_parent(0, 0).is_none(), "单元条目无 MCID 下标");
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn object_reference_key_follows_page_keys_and_advances_next_key() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    b.begin_structure_content(first, "P", 0);
    b.end_structure_content(first);
    b.begin_structure_content(second, "P", 0);
    b.end_structure_content(second);
    let annotation = b.add_annotation(second, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(first, 0))
                .with_element(StructureElement::new("P").with_marked_content(second, 0))
                .with_element(StructureElement::new("Figure").with_object_reference(annotation)),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    // 页面键 0、1 之后，OBJR 键接为 2；下一键 3。
    assert_eq!(
        tree.root().parent_tree_next_key,
        Some(3),
        "父树键 0..2 连续分配"
    );
    for page in [first, second] {
        let parents = doc
            .page_struct_parents(page)
            .expect("page parents")
            .expect("/StructParents");
        assert_eq!(parents.key, page as i64, "第 {page} 页键为 {page}");
    }
    let annotation_dict = &doc.page_annotations(second).expect("annotations")[0].dict;
    assert_eq!(
        annotation_dict.get("StructParent"),
        Some(&Value::Integer(2)),
        "OBJR 键接在页面键之后"
    );
    assert!(matches!(
        tree.parent_tree_entry(2),
        Some(ParentTreeEntry::Object(_))
    ));
}

#[test]
fn object_reference_mixes_with_mcid_kids() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("P")
                    .with_marked_content(page, 0)
                    .with_object_reference(annotation),
            ),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let p_index = tree
        .elements()
        .iter()
        .position(|element| element.structure_type == "P")
        .expect("P element");
    match &tree.elements()[p_index].kids[..] {
        [
            ReaderKid::MarkedContent(mcr),
            ReaderKid::ObjectReference(objr),
        ] => {
            assert_eq!(mcr.mcid, 0);
            assert_eq!(mcr.page, objr.page, "同页的 MCR 与 OBJR `/Pg` 一致");
            let annotation_ref = doc.page_annotations(0).expect("annotations")[0]
                .reference
                .expect("indirect annotation");
            assert_eq!(objr.object, annotation_ref);
        }
        other => panic!("P 应为 MCR + OBJR 两个子项，实际 {other:?}"),
    }
    // 页面数组键 0 与 OBJR 单元键 1 并存。
    assert!(matches!(
        tree.parent_tree_entry(0),
        Some(ParentTreeEntry::Page(_))
    ));
    assert_eq!(tree.mcid_parent(0, 0), Some(p_index));
    assert!(matches!(
        tree.parent_tree_entry(1),
        Some(ParentTreeEntry::Object(_))
    ));
    assert_eq!(tree.object_parent(1), Some(p_index));
    assert_eq!(tree.mcid_parent(1, 0), None);
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0);
    let annotation_dict = &doc.page_annotations(0).expect("annotations")[0].dict;
    assert_eq!(
        annotation_dict.get("StructParent"),
        Some(&Value::Integer(1))
    );
    assert_eq!(tree.root().parent_tree_next_key, Some(2));
}

#[test]
fn object_reference_uses_annotation_page_for_pg() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    // 注解绑定在第 2 页（0 基索引 1），结构元素不携带任何页信息。
    let annotation = b.add_annotation(second, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    assert_eq!(pages.len(), 2, "两页文档");
    let figure = tree
        .elements()
        .iter()
        .find(|element| element.structure_type == "Figure")
        .expect("Figure element");
    match &figure.kids[..] {
        [ReaderKid::ObjectReference(objr)] => {
            assert_eq!(
                objr.page,
                Some(pages[1]),
                "/Pg 取注解所在页（第 2 页），不是第 1 页 {first} 或整页顺序"
            );
            assert_ne!(objr.page, Some(pages[0]));
        }
        other => panic!("Figure 应只有一个 OBJR 子项，实际 {other:?}"),
    }
}
