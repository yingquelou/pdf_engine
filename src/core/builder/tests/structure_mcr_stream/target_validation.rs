//! 句柄/目标校验拒绝：`/Stm` 与 `/StmOwn` 句柄越界、`/StmOwn` 的 `/AP`
//! 不指向 `/Stm`（无 `/AP` 或指向另一表单）在公开 API 上零字节拒绝；
//! 篡改分支（`assemble_document` 后直接 `navigation::apply`）覆盖 `/Stm` 目标
//! 对象不存在与不是 `/Subtype /Form` 流的拒绝。

use super::*;
use crate::core::types::{Object, PdfVersion, Value};

// —— 拒绝路径（公开 API，零字节）——

#[test]
fn stream_mcr_form_handle_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: 7,
        owner: None,
        page: Some(page),
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "structure MCR /Stm form XObject handle 7 is out of range",
    );
}

#[test]
fn stream_mcr_stmown_handle_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(3),
        page: Some(page),
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "structure MCR /StmOwn annotation handle 3 is out of range",
    );
}

#[test]
fn stream_mcr_stmown_without_matching_appearance_is_rejected() {
    for with_other_form in [false, true] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let page = b.page(PageSize::A4);
        let form = b.form_xobject(&marked_form());
        b.draw_form(page, form, IDENTITY_AT_72);
        let annotation = if with_other_form {
            // /AP 指向另一个表单：不是 /Stm 流的所有者。
            let other = b.form_xobject(&marked_form());
            b.add_annotation(page, marker_annotation().with_appearance(other))
        } else {
            // 无 /AP：Table 324 的「owning the stream」无从成立。
            b.add_annotation(page, marker_annotation())
        };
        b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
            stream: form,
            owner: Some(annotation),
            page: Some(page),
            mcid: 0,
        }));
        assert_invalid(&mut b, "does not own the /Stm object");
    }
}

#[test]
fn failed_stream_mcr_validation_writes_no_bytes() {
    // 零字节契约：`/StmOwn` 与 `/Stm` 不一致的失败路径不 flush 任何字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(annotation),
        page: Some(page),
        mcid: 0,
    }));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("mismatched /StmOwn should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());
}

// —— 篡改分支（公开 API 不可达：目标对象不存在 / 类别错配 / 字典已带条目）——

#[test]
fn stream_mcr_rejects_missing_stm_object() {
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

    // 解析表把句柄指到文档里不存在的对象号。
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            form_nums: &[9_024],
            ..Default::default()
        },
    );
    assert_message(message, "structure MCR /Stm object 9024 0 is missing");
}

#[test]
fn stream_mcr_rejects_stm_that_is_not_a_form_stream() {
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

    // 篡改一：解析目标变成普通字典（不是流）。
    doc.set_object(Object::new(form_num, 0, Value::Dict(Dict::new())));
    let message = apply_structure_error(&b, &mut doc, real_targets(&b));
    assert_message(
        message,
        &format!("structure MCR /Stm object {form_num} 0 is not a content stream"),
    );

    // 篡改二：流还在，但 /Subtype 不再是 Form。
    let mut doc = b.assemble_document().expect("assemble");
    let dict = object_dict(&doc, form_num).insert("Subtype", Value::Name("Image".into()));
    doc.set_object(Object::new(
        form_num,
        0,
        Value::Stream(dict, StreamData::Owned(Vec::new())),
    ));
    let message = apply_structure_error(&b, &mut doc, real_targets(&b));
    assert_message(message, "is not a /Subtype /Form content stream");
}
