//! 页面来源冲突拒绝：三路（引用 `/Pg`、`/StmOwn` 绑定页、元素 `/Pg`）都取
//! 不到页面、显式页句柄越界、`/StmOwn` 注解绑定页与引用 `/Pg` 冲突。

use super::*;
use crate::core::types::PdfVersion;

#[test]
fn stream_mcr_without_any_page_source_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: None,
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "needs a page: the reference gives no /Pg, there is no /StmOwn page to fall back to, and the element has no /Pg",
    );
}

#[test]
fn stream_mcr_page_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: Some(page + 5),
        mcid: 0,
    }));
    assert_invalid(&mut b, "structure MCR /Stm page 5 is out of range");
}

#[test]
fn stream_mcr_stmown_page_conflict_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(first, form, IDENTITY_AT_72);
    let annotation = b.add_annotation(first, marker_annotation().with_appearance(form));
    // 引用谎报渲染页：注解绑定页是 first，/Pg 却给 second。
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(annotation),
        page: Some(second),
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "is attached to page 0 but the reference names page 1",
    );
}
