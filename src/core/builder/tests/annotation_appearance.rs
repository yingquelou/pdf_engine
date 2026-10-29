//! `/AP /N` 状态子字典与 `/AS` 写端测试（12.5.5，Table 168）：
//! 多状态外观的写出、`/N` 流/字典互斥、状态名与选中状态校验、PDF 1.2 门禁。

use super::support::annot_object;
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

/// 把单条注解挂到一页文档并写出。
fn build_one(annot: PdfAnnotation, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

#[test]
fn appearance_state_dictionary_writes_states_and_as() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let on = form(&mut b);
    let off = form(&mut b);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_appearance_state("On", on)
            .with_appearance_state("Off", off)
            .with_selected_appearance_state("Off"),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    let ap = dict.get("AP").and_then(|v| v.as_dict()).expect("/AP");
    let states = ap
        .get("N")
        .and_then(|v| v.as_dict())
        .expect("/AP /N state dictionary");
    assert_eq!(states.0.len(), 2);
    assert!(matches!(states.get("On"), Some(Value::Ref(_, 0))));
    assert!(matches!(states.get("Off"), Some(Value::Ref(_, 0))));
    assert_eq!(dict.get("AS"), Some(&Value::Name("Off".into())));
}

#[test]
fn appearance_state_and_normal_stream_are_mutually_exclusive() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let handle = form(&mut b);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_appearance(handle)
            .with_appearance_state("On", handle),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("/N stream plus state dict");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(
        message.contains("cannot be both a stream and a state sub-dictionary"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn appearance_state_selection_must_be_registered() {
    let base = || PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0]);
    let cases = vec![
        // 没有状态子字典却选择 `/AS`。
        base().with_selected_appearance_state("On"),
        // 选中未注册的状态名。
        base()
            .with_appearance_state("On", 0)
            .with_selected_appearance_state("Off"),
        // 状态名重复。
        base()
            .with_appearance_state("On", 0)
            .with_appearance_state("On", 0),
        // 状态名含名字定界符。
        base().with_appearance_state("Bad Name", 0),
    ];
    for annot in cases {
        let error = build_one(annot, PdfVersion::V1_7).expect_err("invalid appearance state");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }
}

#[test]
fn appearance_state_unknown_form_handle_is_rejected() {
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_appearance_state("On", 99),
        PdfVersion::V1_7,
    )
    .expect_err("unknown form handle");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(message.contains("unknown form handle 99"), "{message}");
}

#[test]
fn appearance_states_require_pdf_1_2() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p0 = b.page(PageSize::A4);
    let on = form(&mut b);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
            .with_appearance_state("On", on),
    );
    let error = {
        let mut out = Vec::new();
        b.build(&mut out)
            .expect_err("1.1 rejects /AP state dictionary")
    };
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("appearance streams")),
        "{error}"
    );

    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    let on = form(&mut b);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
            .with_appearance_state("On", on),
    );
    b.build(&mut Vec::new())
        .expect("/AP state dictionary at 1.2");
}
