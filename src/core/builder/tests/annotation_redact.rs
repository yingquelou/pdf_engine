//! Redact 注解写端（12.5.6.23，Table 192）：`/OverlayText` 必带 `/DA`、
//! `/Q` 限 0–2、`/RO` 与被它忽略的文本条目互斥、`/IC` 限 DeviceRGB 三分量、
//! 条目只许挂在 `/Redact` 上、子类型 PDF 1.7 门禁；违反一律 `InvalidObject`
//! 且零字节输出。

use super::support::{annot_object, assert_text_entry};
use super::*;
use crate::core::content::ContentStream;
use crate::core::types::Value;

/// 注册一个最小 Form XObject 并返回句柄。
fn form(b: &mut PdfBuilder) -> usize {
    b.form_xobject(&FormXObject::new(
        [0.0, 0.0, 10.0, 10.0],
        ContentStream::new(),
    ))
}

fn redact(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Redact, rect)
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
    let error = result.expect_err("redact contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "unexpected message: {message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

/// Table 192 的叠加文本条目一次性写出：`/OverlayText` `/DA` `/Q` `/Repeat`
/// 与共用的 `/IC`。
#[test]
fn redact_annotation_writes_overlay_text_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        redact([72.0, 700.0, 200.0, 720.0])
            .with_redact_overlay_text("REDACTED")
            .with_redact_appearance_string("/F1 12 Tf 0 g")
            .with_redact_quadding(1)
            .with_redact_overlay_repeated(true)
            .with_interior_color(vec![0.0, 0.0, 0.0]),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Redact".into())));
    assert_text_entry(&dict, "OverlayText");
    assert_text_entry(&dict, "DA");
    assert_eq!(dict.get("Q"), Some(&Value::Integer(1)));
    assert_eq!(dict.get("Repeat"), Some(&Value::Bool(true)));
    assert!(
        matches!(dict.get("IC"), Some(Value::Array(_))),
        "/IC 应与通用条目同键"
    );
}

/// `/RO` 写成 Form XObject 间接引用，且被引用的表单即使未被页面绘制也分配对象号。
#[test]
fn redact_overlay_appearance_writes_form_reference() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let handle = form(&mut b);
    b.add_annotation(
        p0,
        redact([72.0, 700.0, 200.0, 720.0]).with_redact_overlay_appearance(handle),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let (num, _) = dict
        .get("RO")
        .and_then(|value| value.as_ref())
        .expect("/RO must be an indirect reference");
    let overlay = doc.get_object(num).expect("object").expect("exists");
    match &overlay.value {
        Value::Stream(overlay_dict, _) => {
            assert_eq!(
                overlay_dict.get("Subtype"),
                Some(&Value::Name("Form".into())),
                "/RO 指向的应是 Form XObject"
            );
        }
        other => panic!("/RO references a non-stream object: {other:?}"),
    }
}

/// `/OverlayText` 存在时 `/DA` 必填（Table 192）。
#[test]
fn redact_overlay_text_requires_appearance_string() {
    expect_rejected(
        redact([0.0, 0.0, 10.0, 10.0]).with_redact_overlay_text("REDACTED"),
        "/OverlayText requires /DA",
    );
}

/// `/Q` 只接受 0/1/2。
#[test]
fn redact_quadding_must_be_a_valid_code() {
    expect_rejected(
        redact([0.0, 0.0, 10.0, 10.0]).with_redact_quadding(3),
        "/Q must be 0, 1 or 2",
    );
}

/// `/RO` 优先于叠加文本条目（Table 192「ignored if the RO entry is present」），
/// 同时声明视为矛盾字典。
#[test]
fn redact_overlay_appearance_excludes_text_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let handle = form(&mut b);
    b.add_annotation(
        p0,
        redact([0.0, 0.0, 10.0, 10.0])
            .with_redact_overlay_appearance(handle)
            .with_redact_overlay_repeated(true),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("/RO with /Repeat");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains("/RO takes precedence"), "{message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

/// Redact 的 `/IC` 须是 0.0–1.0 的三分量 DeviceRGB（Table 192）。
#[test]
fn redact_interior_color_must_be_three_rgb_components() {
    // 分量数不是 1/3/4 时先被通用 `/IC` 校验拒绝。
    let (result, out) = build_one(
        redact([0.0, 0.0, 10.0, 10.0]).with_interior_color(vec![0.5, 0.5]),
        PdfVersion::V1_7,
    );
    let message = result.expect_err("two components").to_string();
    assert!(message.contains("/IC"), "{message}");
    assert_eq!(out.len(), 0);

    expect_rejected(
        redact([0.0, 0.0, 10.0, 10.0]).with_interior_color(vec![0.0, 0.0, 0.0, 1.0]),
        "/IC must have 3 DeviceRGB components",
    );
    expect_rejected(
        redact([0.0, 0.0, 10.0, 10.0]).with_interior_color(vec![0.0, 0.5, 1.5]),
        "/IC components must be within 0.0..=1.0",
    );
}

/// Table 192 的条目只许挂在 `/Redact` 上。
#[test]
fn redact_entries_are_specific_to_redact_annotations() {
    let (result, out) = build_one(
        PdfAnnotation::new(AnnotationKind::Stamp, [0.0, 0.0, 10.0, 10.0])
            .with_redact_overlay_text("x")
            .with_redact_appearance_string("/F1 12 Tf"),
        PdfVersion::V1_7,
    );
    let error = result.expect_err("overlay text on Stamp");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains("specific to Redact"), "{message}");
    assert_eq!(out.len(), 0);
}

/// `/Redact` 自 PDF 1.7 引入（Table 169），低版本拒绝。
#[test]
fn redact_annotations_require_pdf_1_7() {
    let (result, out) = build_one(
        redact([0.0, 0.0, 10.0, 10.0])
            .with_redact_overlay_text("x")
            .with_redact_appearance_string("/F1 12 Tf"),
        PdfVersion::V1_6,
    );
    let error = result.expect_err("redact on PDF 1.6");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "expected UnsupportedVersion, got {message}"
    );
    assert!(message.contains("require PDF 1.7"), "{message}");
    assert_eq!(out.len(), 0);
}

/// `/QuadPoints` 是 Table 192 的可选条目，Redact 与文本标记注解同样可挂。
#[test]
fn redact_quad_points_denote_the_removal_region() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        redact([0.0, 0.0, 10.0, 10.0])
            .with_quad_points(vec![0.0, 0.0, 5.0, 0.0, 0.0, 5.0, 5.0, 5.0]),
    );
    let doc = b.assemble_document().expect("redact /QuadPoints");
    let dict = annot_object(&doc, 3, 0);
    let quad = dict
        .get("QuadPoints")
        .and_then(|value| value.as_array())
        .expect("/QuadPoints array");
    assert_eq!(quad.0.len(), 8);

    // 既非标记类也非 Redact 的子类型仍按原契约拒绝 `/QuadPoints`。
    let (result, out) = build_one(
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_quad_points(vec![0.0; 8]),
        PdfVersion::V1_7,
    );
    let error = result.expect_err("quad points on Square");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(out.len(), 0);
}
