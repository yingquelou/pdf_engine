//! Popup 关联写端（12.5.2、Table 163、Table 183）：只声明一侧时另一端自动
//! 补出；配对矛盾、`/Popup` 指向非 `Popup`、无父弹出框一律拒绝且零字节。

use super::support::{annot_object, annot_object_with_num};
use super::*;
use crate::core::types::Value;

/// 把一批注解挂到同一页并写出，返回结果与输出字节（用于断言零字节）。
fn build_all(annotations: Vec<PdfAnnotation>) -> (Result<(), WriteError>, Vec<u8>) {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    for annot in annotations {
        b.add_annotation(p0, annot);
    }
    let mut out = Vec::new();
    let result = b.build(&mut out);
    (result, out)
}

fn popup(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Popup, rect)
}

fn text(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Text, rect)
}

/// 父注解只声明 `/Popup`：弹出框自动获得 `/Parent`（Table 183 必填）。
#[test]
fn popup_parent_is_inferred_from_the_parents_popup_reference() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let popup = b.add_annotation(p0, popup([96.0, 690.0, 240.0, 750.0]));
    b.add_annotation(p0, text([72.0, 700.0, 92.0, 720.0]).with_popup(popup));
    let doc = b.assemble_document().expect("auto-paired /Parent");
    let (popup_num, popup_dict) = annot_object_with_num(&doc, 3, 0);
    let (text_num, text_dict) = annot_object_with_num(&doc, 3, 1);
    assert_eq!(
        popup_dict.get("Parent"),
        Some(&Value::Ref(text_num, 0)),
        "the popup inherits /Parent from the parent's /Popup"
    );
    assert_eq!(text_dict.get("Popup"), Some(&Value::Ref(popup_num, 0)));
}

/// 弹出框只声明 `/Parent`：父注解自动获得 `/Popup`。
#[test]
fn popup_reference_is_inferred_from_the_popups_parent() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, text([72.0, 700.0, 92.0, 720.0]));
    b.add_annotation(p0, popup([96.0, 690.0, 240.0, 750.0]).with_popup_parent(0));
    let doc = b.assemble_document().expect("auto-paired /Popup reference");
    let text_dict = annot_object(&doc, 3, 0);
    let popup_dict = annot_object(&doc, 3, 1);
    let popup_num = annot_object_with_num(&doc, 3, 1).0;
    assert_eq!(text_dict.get("Popup"), Some(&Value::Ref(popup_num, 0)));
    assert_eq!(
        popup_dict.get("Parent"),
        Some(&Value::Ref(annot_object_with_num(&doc, 3, 0).0, 0))
    );
}

/// 两端都显式声明且互相一致：正常写出，两个方向各得一条引用。
#[test]
fn explicit_pairs_on_both_sides_are_written_once() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let popup = b.add_annotation(p0, popup([96.0, 690.0, 240.0, 750.0]).with_popup_parent(1));
    b.add_annotation(p0, text([72.0, 700.0, 92.0, 720.0]).with_popup(popup));
    let doc = b
        .assemble_document()
        .expect("consistent declarations need no inference");
    let popup_num = annot_object_with_num(&doc, 3, 0).0;
    let text_num = annot_object_with_num(&doc, 3, 1).0;
    assert_eq!(
        annot_object(&doc, 3, 0).get("Parent"),
        Some(&Value::Ref(text_num, 0))
    );
    assert_eq!(
        annot_object(&doc, 3, 1).get("Popup"),
        Some(&Value::Ref(popup_num, 0))
    );
}

/// 一个弹出框被两个父注解的 `/Popup` 同时引用：配对不唯一。
#[test]
fn one_popup_cannot_pair_with_two_parents() {
    let (error, out) = build_all(vec![
        popup([96.0, 690.0, 240.0, 750.0]),
        text([72.0, 700.0, 92.0, 720.0]).with_popup(0),
        text([72.0, 600.0, 92.0, 620.0]).with_popup(0),
    ]);
    let message = error.expect_err("two parents claim one popup").to_string();
    assert!(
        message.contains("referenced by /Popup on both annotation 1 and annotation 2"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}

/// 两个弹出框都声明 `/Parent` 指向同一个没有 `/Popup` 的父注解：不唯一。
#[test]
fn one_parent_cannot_pair_with_two_popups() {
    let (error, out) = build_all(vec![
        text([72.0, 700.0, 92.0, 720.0]),
        popup([96.0, 690.0, 240.0, 750.0]).with_popup_parent(0),
        popup([96.0, 600.0, 240.0, 660.0]).with_popup_parent(0),
    ]);
    let message = error.expect_err("two popups claim one parent").to_string();
    assert!(
        message.contains("is named /Parent by both popup 1 and popup 2"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}

/// 两端声明指向不同对象：显式 `/Parent` 与父的 `/Popup` 矛盾。
#[test]
fn contradicting_pair_declarations_are_rejected() {
    let (result, out) = build_all(vec![
        text([72.0, 700.0, 92.0, 720.0]).with_popup(1),
        popup([96.0, 690.0, 240.0, 750.0]).with_popup_parent(2),
        text([72.0, 600.0, 92.0, 620.0]),
    ]);
    let error = result.expect_err("mismatched /Popup and /Parent");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(
        message.contains("whose /Parent points at annotation 2"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}

/// `/Popup` 必须指向 `Popup` 子类型（Table 163）。
#[test]
fn popup_reference_must_target_a_popup_annotation() {
    let (error, out) = build_all(vec![
        text([72.0, 700.0, 92.0, 720.0]).with_popup(1),
        text([96.0, 690.0, 240.0, 750.0]),
    ]);
    let message = error.expect_err("/Popup targets a Text").to_string();
    assert!(
        message.contains("/Popup must reference a Popup annotation"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}

/// 弹出框既无 `/Parent` 也不被任何 `/Popup` 引用：Table 183 必填项缺失。
#[test]
fn popup_annotation_requires_a_parent() {
    let (error, out) = build_all(vec![popup([96.0, 690.0, 240.0, 750.0])]);
    let message = error.expect_err("orphan popup").to_string();
    assert!(
        message.contains("requires /Parent (Table 183)"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}

/// `/Popup` 自引用无法构成配对。
#[test]
fn popup_reference_must_not_target_itself() {
    let (error, out) = build_all(vec![text([72.0, 700.0, 92.0, 720.0]).with_popup(0)]);
    let message = error.expect_err("self-referencing /Popup").to_string();
    assert!(
        message.contains("/Popup must not reference the annotation itself"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}
