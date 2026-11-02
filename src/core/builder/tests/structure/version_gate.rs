//! 结构树与元素文本条目的版本门禁：结构树 PDF 1.3、`/Lang` `/ActualText`
//! PDF 1.4、`/E` PDF 1.5；不满足时拒绝构建且零字节输出，满足时正常写出。

use super::super::*;
use super::support::{assert_version_error, one_page};
use crate::core::types::PdfVersion;

fn simple_tree() -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("P").with_marked_content(0, 0)),
    )
}

#[test]
fn structure_tree_requires_pdf_1_3() {
    let mut b = one_page(PdfVersion::V1_2);
    b.set_structure_tree(simple_tree());
    assert_version_error(&mut b, "structure trees require PDF 1.3");

    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(simple_tree());
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.3 supports structure trees");
    assert!(String::from_utf8_lossy(&out).contains("/Type /StructTreeRoot"));
}

#[test]
fn structure_language_and_actual_text_require_pdf_1_4() {
    let lang =
        || StructureTree::new().with_element(StructureElement::new("Document").with_lang("en-US"));
    let actual = || {
        StructureTree::new()
            .with_element(StructureElement::new("Document").with_actual_text("Hello"))
    };

    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(lang());
    assert_version_error(&mut b, "/Lang and /ActualText require PDF 1.4");

    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(actual());
    assert_version_error(&mut b, "/Lang and /ActualText require PDF 1.4");

    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(lang());
    b.build(&mut Vec::new()).expect("PDF 1.4 supports /Lang");
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(actual());
    b.build(&mut Vec::new())
        .expect("PDF 1.4 supports /ActualText");
}

#[test]
fn structure_expanded_text_requires_pdf_1_5() {
    let tree = || {
        StructureTree::new()
            .with_element(StructureElement::new("Document").with_expanded("Internationalization"))
    };
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(tree());
    assert_version_error(&mut b, "/E requires PDF 1.5");

    let mut b = one_page(PdfVersion::V1_5);
    b.set_structure_tree(tree());
    b.build(&mut Vec::new()).expect("PDF 1.5 supports /E");
}
