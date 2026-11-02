//! MCID 与声称互斥拒绝：负 `/MCID`、同容器 MCID 重复声称、`/Stm` 容器与
//! OBJR 声称互斥（Table 326「not both」shall）、篡改分支的容器字典已带
//! `/StructParent(s)` 与 `/StmOwn` 句柄被解析到流对象。

use super::*;
use crate::core::types::{Object, PdfVersion, Value};

#[test]
fn stream_mcr_negative_mcid_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: Some(page),
        mcid: -1,
    }));
    assert_invalid(&mut b, "structure MCR MCID must be non-negative, got -1");
}

#[test]
fn stream_mcr_duplicate_mcid_in_container_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    let kid = || StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: Some(page),
        mcid: 0,
    };
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("Figure").with_kid(kid()))
                .with_element(StructureElement::new("Sect").with_kid(kid())),
        ),
    );
    assert_invalid(
        &mut b,
        "MCID 0 is claimed by more than one structure element",
    );
}

#[test]
fn stream_mcr_container_conflicts_with_objr_claim_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    // 同一表单既被 OBJR 声称（写出 /StructParent）又作 /Stm 容器
    // （写出 /StructParents）：Table 326「not both」拒绝。OBJR 子项先被
    // 展开，故报错来自流形态分支。
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("Widget").with_form_xobject_reference(form, Some(page)),
                )
                .with_element(StructureElement::new("Figure").with_kid(
                    StructureKid::StreamMarkedContent {
                        stream: form,
                        owner: None,
                        page: Some(page),
                        mcid: 0,
                    },
                )),
        ),
    );
    assert_invalid(&mut b, "is also claimed as an OBJR content item");
}

#[test]
fn stream_mcr_rejects_container_with_existing_struct_entries() {
    for entry in ["StructParent", "StructParents"] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let page = b.page(PageSize::A4);
        let form = b.form_xobject(&marked_form());
        b.draw_form(page, form, IDENTITY_AT_72);
        b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
            stream: form,
            owner: None,
            page: Some(page),
            mcid: 0,
        }));
        let mut doc = b.assemble_document().expect("assemble");
        let form_num = b.form_nums[form];
        let dict = object_dict(&doc, form_num).insert(entry, Value::Integer(9));
        doc.set_object(Object::new(
            form_num,
            0,
            Value::Stream(dict, StreamData::Owned(Vec::new())),
        ));

        let message = apply_structure_error(&b, &mut doc, real_targets(&b));
        assert_message(
            &message,
            &format!("/Stm object {form_num} 0 already has /{entry}"),
        );
        assert_message(
            message,
            "at most one of /StructParent and /StructParents may appear in an object (Table 326)",
        );
    }
}

#[test]
fn stream_mcr_rejects_stmown_pointing_at_stream_object() {
    // /StmOwn 必须是普通注解字典：句柄被解析到流对象（篡改表）时拒绝。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(0),
        page: Some(page),
        mcid: 0,
    }));
    let mut doc = b.assemble_document().expect("assemble");
    let form_num = b.form_nums[form];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[form_num],
            annotation_pages: &[0],
            form_nums: &[form_num],
            ..Default::default()
        },
    );
    assert_message(&message, "/StmOwn object");
    assert_message(message, "is not an annotation dictionary");
}
