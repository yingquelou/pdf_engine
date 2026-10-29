//! 旧式注解边框数组 `/Border`（Table 164）与通用 `/BS` 边框样式字典
//! （12.5.4）写端测试：数组写出、`/BS` 互斥、PDF 1.1/1.2 版本门禁。

use super::support::annot_object;
use super::*;
use crate::core::types::{Array, PdfFeature, PdfVersion, Value};

fn assemble_one(annot: PdfAnnotation, version: PdfVersion) -> (Document, Dict) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    (doc, dict)
}

fn build_one(annot: PdfAnnotation, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

fn circle(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Circle, rect)
}

fn link(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, rect)
}

#[test]
fn legacy_border_array_is_written() {
    let (doc, dict) = assemble_one(
        circle([0.0, 0.0, 100.0, 100.0])
            .with_border_array(BorderArray::new(2.0, 1.0, 0.5).with_dash(vec![3.0, 2.0])),
        PdfVersion::V1_7,
    );
    let border = dict
        .get("Border")
        .and_then(Value::as_array)
        .expect("/Border");
    assert_eq!(
        border.0,
        vec![
            Value::Float(2.0),
            Value::Float(1.0),
            Value::Float(0.5),
            Value::Array(Array(vec![Value::Float(3.0), Value::Float(2.0)])),
        ]
    );
    assert!(dict.get("BS").is_none(), "未设置 /BS 时不写");
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::AnnotationBorderDash)
    );
}

#[test]
fn legacy_border_without_dash_requires_only_pdf_1_0() {
    let (doc, dict) = assemble_one(
        link([0.0, 0.0, 10.0, 10.0]).with_border_array(BorderArray::new(0.0, 0.0, 1.0)),
        PdfVersion::V1_0,
    );
    let border = dict
        .get("Border")
        .and_then(Value::as_array)
        .expect("/Border");
    assert_eq!(
        border.0,
        vec![Value::Float(0.0), Value::Float(0.0), Value::Float(1.0)]
    );
    assert!(
        !doc.features
            .borrow()
            .contains(PdfFeature::AnnotationBorderDash)
    );
}

#[test]
fn border_array_and_border_style_are_mutually_exclusive() {
    let style_entries = [
        circle([0.0, 0.0, 10.0, 10.0]).with_border_width(1.0),
        circle([0.0, 0.0, 10.0, 10.0]).with_border_style(AnnotationBorderStyle::Dashed),
        circle([0.0, 0.0, 10.0, 10.0]).with_border_dash(vec![2.0, 1.0]),
    ];
    for annot in style_entries {
        let missing = annot.with_border_array(BorderArray::new(0.0, 0.0, 1.0));
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p = b.page(PageSize::A4);
        b.add_annotation(p, missing);
        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("/BS and /Border must conflict");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains("mutually exclusive"), "{error}");
        assert_eq!(out.len(), 0);
    }
}

#[test]
fn invalid_border_arrays_are_rejected() {
    let cases = [
        BorderArray::new(f64::NAN, 0.0, 1.0),
        BorderArray::new(0.0, f64::INFINITY, 1.0),
        BorderArray::new(-1.0, 0.0, 1.0),
        BorderArray::new(0.0, 0.0, -1.0),
        BorderArray::new(0.0, 0.0, 1.0).with_dash(vec![1.0]),
        BorderArray::new(0.0, 0.0, 1.0).with_dash(vec![1.0, -2.0]),
    ];
    for border in cases {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p = b.page(PageSize::A4);
        b.add_annotation(p, circle([0.0, 0.0, 10.0, 10.0]).with_border_array(border));
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid /Border values");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains("/Border"), "{error}");
        assert_eq!(out.len(), 0);
    }
}

#[test]
fn legacy_border_dash_requires_pdf_1_1() {
    let dashed = || {
        link([0.0, 0.0, 10.0, 10.0])
            .with_border_array(BorderArray::new(0.0, 0.0, 1.0).with_dash(vec![2.0, 1.0]))
    };
    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    let p = b.page(PageSize::A4);
    b.add_annotation(p, dashed());
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.0 rejects dash array");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("border dash") && message.contains("1.1")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    build_one(dashed(), PdfVersion::V1_1).expect("/Border dash at 1.1");
}
