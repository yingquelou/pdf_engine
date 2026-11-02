//! 图像 XObject 作为 OBJR 的 `/Obj` 目标（页资源 `/Im<序号+1>`，14.7.4.3）：
//! `/Obj` 指向该页图像资源里的流、图像流字典追加 `/StructParent`、父树键按
//! 解析后的目标对象号升序接排；内容相同的两张贴图经图像去重池共用一个对象
//! 号，故第二次声称即重复声称（判重键取解析后对象号）；另覆盖页/资源序号
//! 越界与 XObject 目标校验失败的整档零字节契约。

use super::super::*;
use super::support::*;
use crate::core::reader::structure::ParentTreeEntry;
use crate::core::types::{PdfVersion, Value};

#[test]
fn objr_accepts_image_xobject_target() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 0),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let image_ref = page_xobject(&doc, pages[0].0, "Im1");
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(object, image_ref, "/Obj 指向该页图像资源里的流");
    assert_eq!(objr_page, Some(pages[0]), "/Pg 即图像所在页");
    let dict = object_dict(&doc, image_ref.0);
    assert_eq!(
        dict.get("Subtype"),
        Some(&Value::Name("Image".into())),
        "目标是图像流"
    );
    assert_eq!(
        dict.get("StructParent"),
        Some(&Value::Integer(0)),
        "图像流字典追加 /StructParent（Table 326 第一行）"
    );
    assert_eq!(tree.object_parent(0), Some(element_of(&tree, "Figure")));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn objr_accepts_two_distinct_images_on_one_page() {
    // 两张不同内容的图像是两个对象号：两个元素各自声称合法，键按对象号升序。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x10));
    b.image(page, 120.0, 700.0, 20.0, 20.0, &rgb_image(0x20));
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("FigureA").with_image_xobject_reference(page, 0),
                )
                .with_element(
                    StructureElement::new("FigureB").with_image_xobject_reference(page, 1),
                ),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let first = page_xobject(&doc, pages[0].0, "Im1");
    let second = page_xobject(&doc, pages[0].0, "Im2");
    assert_ne!(first.0, second.0, "内容不同 → 两个流对象");
    let (object_a, _) = only_objr(&tree, "FigureA");
    let (object_b, _) = only_objr(&tree, "FigureB");
    assert_eq!(object_a, first);
    assert_eq!(object_b, second);
    // 键按解析后的对象号升序接排。
    let low = first.0.min(second.0);
    let low_key = if object_a.0 == low { 0 } else { 1 };
    assert_eq!(
        object_dict(&doc, first.0).get("StructParent"),
        Some(&Value::Integer(low_key))
    );
    assert_eq!(
        object_dict(&doc, second.0).get("StructParent"),
        Some(&Value::Integer(if low_key == 0 { 1 } else { 0 }))
    );
    assert_eq!(tree.root().parent_tree_next_key, Some(2));
}

#[test]
fn objr_duplicate_claim_is_keyed_by_resolved_object_number() {
    // 同一图像内容在同页放两次：去重池让 `/Im1` 与 `/Im2` 共用一个对象号，
    // 于是两个资源序号指向同一对象——第二次声称即重复声称。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x80));
    b.image(page, 120.0, 700.0, 20.0, 20.0, &rgb_image(0x80));
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("FigureA").with_image_xobject_reference(page, 0),
                )
                .with_element(
                    StructureElement::new("FigureB").with_image_xobject_reference(page, 1),
                ),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR image XObject page 0 index 1 is claimed by more than one structure element",
    );
}

#[test]
fn objr_image_target_page_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(3, 0),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR image XObject page 3 is out of range (page count 1)",
    );
}

#[test]
fn objr_image_target_index_out_of_range_is_rejected() {
    // 页上有一张图像：序号 1 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 1),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR image XObject index 1 on page 0 is out of range (count 1)",
    );

    // 页上没有图像：序号 0 即越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(0, 0),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR image XObject index 0 on page 0 is out of range (count 0)",
    );
}

#[test]
fn objr_image_and_form_targets_mix_with_annotation_target() {
    // 一页同时有标记内容、图像、表单与注解：父树键先是页面键（按页升序），
    // 后接全部 OBJR 键（按解析后的目标对象号升序）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x10));
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Figure").with_image_xobject_reference(page, 0))
                .with_element(StructureElement::new("Logo").with_object_reference(
                    StructureObjectReference::form_xobject_on_page(form, page),
                ))
                .with_element(StructureElement::new("Note").with_object_reference(annotation)),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let targets = [
        (
            "Figure",
            page_xobject(&doc, pages[0].0, "Im1").0,
            element_of(&tree, "Figure"),
        ),
        (
            "Logo",
            page_xobject(&doc, pages[0].0, "Fm1").0,
            element_of(&tree, "Logo"),
        ),
        (
            "Note",
            doc.page_annotations(0).expect("annotations")[0]
                .reference
                .expect("indirect annotation")
                .0,
            element_of(&tree, "Note"),
        ),
    ];
    // 页面键 0 之后，OBJR 键按对象号升序 1、2、3。
    let mut ordered = targets.to_vec();
    ordered.sort_by_key(|(_, num, _)| *num);
    for (rank, (_, num, element)) in ordered.iter().enumerate() {
        let key = rank as i64 + 1;
        assert_eq!(
            object_dict(&doc, *num).get("StructParent"),
            Some(&Value::Integer(key)),
            "对象 {num} 的父树键应接在页面键之后按对象号升序排为 {key}"
        );
        match tree.parent_tree_entry(key) {
            Some(ParentTreeEntry::Object(found)) => assert_eq!(
                *found, *element,
                "父树键 {key} 的单元条目应指向声称它的元素"
            ),
            other => panic!("父树键 {key} 应为单元条目，实际 {other:?}"),
        }
    }
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "页面键仍为 0");
    assert_eq!(tree.root().parent_tree_next_key, Some(4));
}

#[test]
fn failed_objr_xobject_validation_writes_no_bytes() {
    // 页上有文字、图像与表单：XObject 目标的句柄越界也必须整档零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    b.image(page, 72.0, 600.0, 20.0, 20.0, &rgb_image(0x40));
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 9),
    ));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("越界的图像资源序号应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());

    // 表单渲染页谎报同样零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    b.text(first, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    let form = b.form_xobject(&simple_form());
    b.draw_form(second, form, IDENTITY_AT_72);
    b.add_annotation(first, marker_annotation());
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, first),
    ));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未绘制的页应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());
}
