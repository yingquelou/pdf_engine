//! Table 326 `/StructParent` ↔ `/StructParents` 互斥与 OBJR 目标形态校验
//! （14.7.4.3、14.7.4.4）：目标对象须存在、须是字典，注解目标不得是流、
//! XObject 目标须带对应 `/Subtype`，目标字典已带 `/StructParent` 或
//! `/StructParents`（含「将写 `/StructParents` 的页内容流」）时不再追加另一个。
//!
//! 这些分支在公开 API 上不可达（写端不给目标字典预置这两个条目，也不会让
//! 句柄解析到别的类别的对象），故沿用 `action_ocg.rs` 等的既有模式：先
//! `assemble_document` 取得真实对象号，再篡改 `Document` 或解析表并直接调用
//! `navigation::apply`。校验仍在结构对象号分配之前。

use super::super::*;
use super::support::*;
use crate::core::builder::navigation::structure::StructureTargets;
use crate::core::types::{Object, PdfVersion, Value};

#[test]
fn objr_rejects_missing_target_object() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));
    let mut doc = b.assemble_document().expect("assemble");

    // 解析表给出文档里不存在的对象号。
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[9_024],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        "structure OBJR annotation handle 0 object 9024 0 is missing",
    );

    // 句柄在对象号表里存在、但页表短于对象号表（表本身不一致）。
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[b.annotation_nums[annotation]],
            annotation_pages: &[],
            ..Default::default()
        },
    );
    assert_message(
        message,
        "structure OBJR annotation handle 0 is out of range (count 0)",
    );
}

#[test]
fn objr_rejects_struct_parent_and_struct_parents_on_same_object() {
    // Table 326 shall：「At most one of these two entries shall be present in
    // a given object」——目标字典已带任一条目时不再追加另一个，直接拒绝。
    for entry in ["StructParent", "StructParents"] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let page = b.page(PageSize::A4);
        let annotation = b.add_annotation(page, marker_annotation());
        b.set_structure_tree(objr_tree(annotation));
        let mut doc = b.assemble_document().expect("assemble");

        let annotation_num = b.annotation_nums[annotation];
        let dict = object_dict(&doc, annotation_num).insert(entry, Value::Integer(9));
        doc.set_object(Object::new(annotation_num, 0, Value::Dict(dict)));

        let message = apply_structure_error(
            &b,
            &mut doc,
            StructureTargets {
                annotation_nums: &b.annotation_nums,
                annotation_pages: &b.annotation_pages,
                ..Default::default()
            },
        );
        assert_message(&message, &format!("already has /{entry}"));
        assert_message(
            message,
            "at most one of /StructParent and /StructParents may appear in an object (Table 326)",
        );
    }
}

#[test]
fn objr_rejects_target_that_will_carry_struct_parents() {
    // 目标对象号落在「将写 /StructParents 的页对象」上：该对象是含标记内容
    // 序列的内容流，不得再作为整对象内容项追加 /StructParent。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Figure").with_object_reference(0)),
        ),
    );
    let mut doc = b.assemble_document().expect("assemble");
    let pages = page_references(&doc);
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[pages[0].0],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        "is also a content stream carrying /StructParents; at most one of /StructParent and /StructParents may appear in an object (Table 326)",
    );
}

#[test]
fn objr_rejects_target_that_is_not_a_dictionary() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));
    let mut doc = b.assemble_document().expect("assemble");

    let annotation_num = b.annotation_nums[annotation];
    doc.set_object(Object::new(annotation_num, 0, Value::Integer(3)));
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[annotation_num],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR annotation handle 0 object {annotation_num} 0 is not a dictionary"
        ),
    );
}

#[test]
fn objr_rejects_annotation_target_that_is_a_stream() {
    // 句柄解析到表单流对象，却按注解声称：对象形态与类别必须一致。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_tree(0));
    let mut doc = b.assemble_document().expect("assemble");

    let form_num = b.form_nums[form];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[form_num],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR annotation handle 0 object {form_num} 0 is a stream, not an annotation dictionary (Table 325)"
        ),
    );
}

#[test]
fn objr_rejects_xobject_target_with_wrong_subtype() {
    // 表单目标解析到图像流。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(page),
        StructureObjectReference::form_xobject(0),
    ));
    let mut doc = b.assemble_document().expect("assemble");
    let image_num = b.image_nums[page][0];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            form_nums: &[image_num],
            form_pages: &[vec![page]],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR form XObject handle 0 object {image_num} 0 is not a /Subtype /Form XObject stream (Table 325)"
        ),
    );

    // 图像目标解析到表单流。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 0),
    ));
    let mut doc = b.assemble_document().expect("assemble");
    let form_num = b.form_nums[form];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            image_nums: &[vec![form_num]],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR image XObject page 0 index 0 object {form_num} 0 is not a /Subtype /Image XObject stream (Table 325)"
        ),
    );
}
