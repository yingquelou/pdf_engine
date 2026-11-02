//! Form XObject 作为 OBJR 的 `/Obj` 目标（14.7.4.3「an entire PDF object,
//! such as an XObject or an annotation」+ Table 325 的 `/Obj` 必填间接引用）：
//! 写出形态与 `/Pg` 的三档解析——引用给出的页覆盖元素 `/Pg`（Table 325
//! 「This entry overrides any Pg entry in the structure element containing
//! the object reference」），缺页时沿用元素 `/Pg`，整对象内容项在流字典追加
//! `/StructParent`（Table 326 + 14.7.4.4 Example 1）；另覆盖句柄越界、表单
//! 未在声称页绘制、从未绘制、两路都取不到页、页越界与重复声称的拒绝路径。

use super::super::*;
use super::support::*;
use crate::core::types::{PdfVersion, Value};

#[test]
fn objr_accepts_form_xobject_target() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, page),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let form_ref = page_xobject(&doc, pages[0].0, "Fm1");
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(object, form_ref, "/Obj 须指向绘制页资源里的表单流");
    assert_eq!(objr_page, Some(pages[0]), "/Pg 是表单渲染页（Table 325）");
    let dict = object_dict(&doc, form_ref.0);
    assert_eq!(
        dict.get("Subtype"),
        Some(&Value::Name("Form".into())),
        "目标是表单流"
    );
    assert_eq!(
        dict.get("StructParent"),
        Some(&Value::Integer(0)),
        "整对象内容项在流字典追加 /StructParent（Table 326 + 14.7.4.4 Example 1）"
    );
    assert_eq!(tree.object_parent(0), Some(element_of(&tree, "Figure")));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn objr_form_target_pg_falls_back_to_element_pg() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    // 引用不带页：Table 325「it shall be used if the structure element has
    // no such entry」的相反面——缺页时取元素 `/Pg`。
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(page),
        StructureObjectReference::form_xobject(form),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(objr_page, Some(pages[0]), "/Pg 沿用元素 /Pg");
    assert_eq!(
        tree.elements()[element_of(&tree, "Figure")].page,
        Some(pages[0]),
        "元素 /Pg 自身也写出"
    );
    assert_eq!(
        object_dict(&doc, object.0).get("StructParent"),
        Some(&Value::Integer(0))
    );
}

#[test]
fn objr_form_target_pg_overrides_element_pg() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(first, form, IDENTITY_AT_72);
    b.draw_form(second, form, IDENTITY_AT_72);
    // 元素 /Pg 指第一页，引用指第二页：Table 325「This entry overrides any
    // Pg entry in the structure element containing the object reference」。
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(first),
        StructureObjectReference::form_xobject_on_page(form, second),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    assert_eq!(pages.len(), 2, "两页文档");
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(objr_page, Some(pages[1]), "引用的 /Pg 覆盖元素 /Pg");
    assert_eq!(
        tree.elements()[element_of(&tree, "Figure")].page,
        Some(pages[0]),
        "元素 /Pg 保持第一页，不被改写"
    );
    assert_eq!(
        object,
        page_xobject(&doc, pages[0].0, "Fm1"),
        "同一表单在两页各登记一次资源名，对象号仍是同一个"
    );
    assert_eq!(
        object_dict(&doc, object.0).get("StructParent"),
        Some(&Value::Integer(0))
    );
}

#[test]
fn objr_form_target_page_must_draw_the_form() {
    // 显式 /Pg 谎报：表单只画在第 2 页，引用却声称第 1 页。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(second, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, first),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is not drawn on page 0",
    );

    // 沿用元素 /Pg 的那一路同样核对渲染页（两路共用一次检查）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(second, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(first),
        StructureObjectReference::form_xobject(form),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is not drawn on page 0",
    );
}

#[test]
fn objr_form_target_never_drawn_is_rejected() {
    // 注册但从未绘制的表单没有对象号，也没有任何渲染页：先按「未绘制」拒绝，
    // 而不是让句柄解析出对象号 0 后报一个难以理解的「object 0 0 is missing」。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, page),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is not drawn on page 0",
    );
}

#[test]
fn objr_form_target_without_any_page_is_rejected() {
    // 引用不给页且元素无 /Pg：Table 325 的 /Pg 无从写出。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject(form),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 needs a page: the reference gives no /Pg and the element has no /Pg (Table 325)",
    );
}

#[test]
fn objr_form_handle_out_of_range_is_rejected() {
    // 未注册任何表单。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(0),
        StructureObjectReference::form_xobject(0),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is out of range (count 0)",
    );

    // 注册并绘制一个表单：句柄 3 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(page),
        StructureObjectReference::form_xobject_on_page(3, page),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 3 is out of range (count 1)",
    );
    // 句柄未使用，错误按声明的类别与句柄报告。
}

#[test]
fn objr_form_target_page_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(0, 5),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 names page 5 out of range (page count 1)",
    );
}

#[test]
fn objr_form_target_claimed_twice_is_rejected() {
    // 同一表单在两页各绘制一次，但写端只出一个对象号：第二次声称按对象号
    // 判重拒绝（14.7.4.3 NOTE 2 要求每次渲染各用一个对象引用，此处无法满足）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(first, form, IDENTITY_AT_72);
    b.draw_form(second, form, IDENTITY_AT_72);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("Figure").with_form_xobject_reference(form, Some(first)),
                )
                .with_element(
                    StructureElement::new("Logo").with_form_xobject_reference(form, Some(second)),
                ),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is claimed by more than one structure element",
    );
}
