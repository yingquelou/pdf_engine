//! 注解目标的构建期拒绝路径与零字节契约：句柄越界、同一注解被兄弟元素或
//! 同一元素的两个 `/K` 子项重复声称时，`build` 报 `WriteError::InvalidObject`
//! 且整档不写出任何字节（14.7.4.3、14.7.4.4）。

use super::super::*;
use super::support::*;
use crate::core::types::PdfVersion;

#[test]
fn object_reference_handle_out_of_range_is_rejected() {
    // 未注册任何注解：句柄 0 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(objr_tree(0));
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 0 is out of range (count 0)",
    );

    // 注册一条注解：句柄 3 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(3));
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 3 is out of range (count 1)",
    );
}

#[test]
fn object_reference_duplicate_annotation_is_rejected() {
    // 两个兄弟元素声称同一注解。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("Figure").with_object_reference(annotation))
                .with_element(StructureElement::new("Note").with_object_reference(annotation)),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 0 is claimed by more than one structure element",
    );

    // 同一元素的两个 `/K` 子项声称同一注解同样拒绝。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_object_reference(annotation)
                .with_object_reference(annotation),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 0 is claimed by more than one structure element",
    );
}

#[test]
fn failed_object_reference_validation_writes_no_bytes() {
    // 页面上有字体与文本对象：OBJR 校验失败也必须整档不落一个字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(7));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("越界句柄应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());

    // 重复声称同样零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("Figure").with_object_reference(annotation))
                .with_element(StructureElement::new("Note").with_object_reference(annotation)),
        ),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("重复声称应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());
}
