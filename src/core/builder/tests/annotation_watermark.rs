//! Watermark 注解的固定打印字典 `/FixedPrint`（12.5.6.22，Table 190/191）写端：
//! 写成直接字典 `/Type /FixedPrint` + `/Matrix`（六项有限数值）+ `/H` `/V`，
//! 缺省条目不物化；条目只许挂在 `/Watermark` 上，且 12.5.6.22 要求水印
//! 「无弹出窗口与其他交互元素」，故 `/Popup`、`/A`、`/Dest` 一并拒绝。
//! 违反一律 `InvalidObject` 且零字节输出；子类型自 PDF 1.6 引入（Table 169）。

use super::support::annot_object;
use super::*;
use crate::core::builder::{Action, AnnotationDestination, FixedPrint};
use crate::core::types::Value;

fn watermark(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Watermark, rect)
}

/// 把单条注解挂到一页文档并按指定版本写出。
fn build_one(annot: PdfAnnotation, version: PdfVersion) -> (Result<(), WriteError>, Vec<u8>) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    let result = b.build(&mut out);
    (result, out)
}

/// 断言构建失败、消息命中且未写出任何字节。
fn expect_rejected(annot: PdfAnnotation, fragment: &str) {
    let (result, out) = build_one(annot, PdfVersion::V1_7);
    let error = result.expect_err("watermark contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "unexpected message: {message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

/// 取写出的注解字典里的 `/FixedPrint` 子字典。
fn written_fixed_print(doc: &crate::core::types::Document) -> crate::core::types::Dict {
    let dict = annot_object(doc, 3, 0);
    dict.get("FixedPrint")
        .and_then(|value| value.as_dict())
        .expect("/FixedPrint dictionary")
        .clone()
}

/// Table 191 全三项条目写成直接字典，`/Type` 恒为 `FixedPrint`。
#[test]
fn watermark_annotation_writes_fixed_print_dictionary() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        watermark([0.0, 0.0, 100.0, 100.0]).with_fixed_print(
            FixedPrint::new()
                .with_matrix([1.0, 0.0, 0.0, 1.0, 72.0, -72.0])
                .with_horizontal_offset(0.0)
                .with_vertical_offset(1.0),
        ),
    );
    let doc = b.assemble_document().expect("assemble");
    let annot = annot_object(&doc, 3, 0);
    assert_eq!(annot.get("Subtype"), Some(&Value::Name("Watermark".into())));
    let fixed = written_fixed_print(&doc);
    assert_eq!(fixed.get("Type"), Some(&Value::Name("FixedPrint".into())));
    let matrix = fixed
        .get("Matrix")
        .and_then(|value| value.as_array())
        .expect("/Matrix array");
    assert_eq!(matrix.0.len(), 6, "Table 191 的矩阵是六项");
    assert_eq!(matrix.0[4], Value::Float(72.0));
    assert_eq!(matrix.0[5], Value::Float(-72.0));
    assert_eq!(fixed.get("H"), Some(&Value::Float(0.0)));
    assert_eq!(fixed.get("V"), Some(&Value::Float(1.0)));
}

/// 缺省条目不物化：空字典只写 `/Type`，未声明时不写 `/FixedPrint`。
#[test]
fn fixed_print_defaults_are_not_written() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        watermark([0.0, 0.0, 100.0, 100.0]).with_fixed_print(FixedPrint::new()),
    );
    let fixed = written_fixed_print(&b.assemble_document().expect("empty dictionary"));
    assert_eq!(fixed.get("Type"), Some(&Value::Name("FixedPrint".into())));
    for key in ["Matrix", "H", "V"] {
        assert!(
            fixed.get(key).is_none(),
            "Table 191 的缺省条目 /{key} 不物化"
        );
    }

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, watermark([0.0, 0.0, 100.0, 100.0]));
    let plain = annot_object(&b.assemble_document().expect("plain watermark"), 3, 0);
    assert!(
        plain.get("FixedPrint").is_none(),
        "Table 190 的 /FixedPrint 是可选条目"
    );
}

/// `/Matrix` 与两个平移量须为有限数值。
#[test]
fn invalid_fixed_print_values_are_rejected() {
    expect_rejected(
        watermark([0.0, 0.0, 100.0, 100.0]).with_fixed_print(FixedPrint::new().with_matrix([
            f64::NAN,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
        ])),
        "/Matrix entries must be finite",
    );
    expect_rejected(
        watermark([0.0, 0.0, 100.0, 100.0])
            .with_fixed_print(FixedPrint::new().with_horizontal_offset(f64::INFINITY)),
        "/H must be a finite number",
    );
    expect_rejected(
        watermark([0.0, 0.0, 100.0, 100.0])
            .with_fixed_print(FixedPrint::new().with_vertical_offset(f64::NEG_INFINITY)),
        "/V must be a finite number",
    );
}

/// 固定打印字典只许挂在 Watermark 注解上（Table 190）。
#[test]
fn fixed_print_entries_are_rejected_on_other_subtypes() {
    for kind in [
        AnnotationKind::Text,
        AnnotationKind::Stamp,
        AnnotationKind::Screen,
    ] {
        let (result, out) = build_one(
            PdfAnnotation::new(kind, [0.0, 0.0, 10.0, 10.0])
                .with_fixed_print(FixedPrint::new().with_vertical_offset(0.5)),
            PdfVersion::V1_7,
        );
        let error = result.expect_err("/FixedPrint on a non-Watermark annotation");
        let message = error.to_string();
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {message}"
        );
        assert!(message.contains("specific to Watermark"), "{message}");
        assert_eq!(out.len(), 0);
    }
}

/// 12.5.6.22：水印注解无弹出窗口与其他交互元素。
#[test]
fn watermark_annotation_rejects_interactive_entries() {
    expect_rejected(
        watermark([0.0, 0.0, 100.0, 100.0]).with_popup(0),
        "shall not have a pop-up window",
    );
    expect_rejected(
        watermark([0.0, 0.0, 100.0, 100.0]).with_action(Action::Uri("https://example.com".into())),
        "shall not have interactive elements",
    );
    expect_rejected(
        watermark([0.0, 0.0, 100.0, 100.0])
            .with_destination(AnnotationDestination::Named("stamp".into())),
        "shall not have interactive elements",
    );
}

/// `/Watermark` 自 PDF 1.6 引入（Table 169），低版本拒绝且不写字节。
#[test]
fn watermark_annotations_require_pdf_1_6() {
    for version in [PdfVersion::V1_0, PdfVersion::V1_5] {
        let (result, out) = build_one(
            watermark([0.0, 0.0, 100.0, 100.0])
                .with_fixed_print(FixedPrint::new().with_horizontal_offset(0.25)),
            version,
        );
        let error = result.expect_err("watermark below PDF 1.6");
        let message = error.to_string();
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "expected UnsupportedVersion, got {message}"
        );
        assert!(message.contains("require PDF 1.6"), "{message}");
        assert_eq!(out.len(), 0);
    }
    let (result, _) = build_one(
        watermark([0.0, 0.0, 100.0, 100.0]).with_fixed_print(FixedPrint::new()),
        PdfVersion::V1_6,
    );
    result.expect("watermark is writable from PDF 1.6");
}
