//! 页面演示条目写端测试（ISO 32000-1 12.4.4，页面字典 Table 30 / 过渡
//! 字典 Table 162）：`/Dur` 与 `/Trans` 的写出、可选性、PDF 1.1 门禁与
//! PDF 1.5 样式条目的更高门禁。

use super::*;
use crate::core::types::{PdfFeature, PdfVersion};

fn page_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("page object")
        .expect("page exists")
        .value
        .as_dict()
        .expect("page dict")
        .clone()
}

#[test]
fn page_transition_and_duration_are_written() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.set_page_duration(p0, 5.0);
    b.set_page_transition(
        p0,
        &Transition::new(TransitionStyle::Split)
            .with_duration(3.5)
            .with_dimension(TransitionDimension::Vertical)
            .with_motion(TransitionMotion::Outward),
    );
    let doc = b.assemble_document().expect("assemble");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::PagePresentation));

    let page = page_dict(&doc, 3);
    assert_eq!(page.get("Dur"), Some(&Value::Float(5.0)));
    let trans = page.get("Trans").and_then(Value::as_dict).expect("/Trans");
    assert_eq!(trans.get("Type"), Some(&Value::Name("Trans".into())));
    assert_eq!(trans.get("S"), Some(&Value::Name("Split".into())));
    assert_eq!(trans.get("D"), Some(&Value::Float(3.5)));
    assert_eq!(trans.get("Dm"), Some(&Value::Name("V".into())));
    assert_eq!(trans.get("M"), Some(&Value::Name("O".into())));
    assert!(trans.get("Di").is_none(), "未声明的 /Di 不物化");
}

#[test]
fn page_presentation_entries_are_optional() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.set_page_duration(p0, 2.5);
    // 未知页索引被忽略；重复设置以后一次为准。
    b.set_page_transition(9, &Transition::new(TransitionStyle::R));
    b.set_page_duration(9, 1.0);
    b.set_page_duration(p0, 4.0);
    let doc = b.assemble_document().expect("assemble");

    let first = page_dict(&doc, 3);
    assert_eq!(first.get("Dur"), Some(&Value::Float(4.0)));
    assert!(first.get("Trans").is_none(), "未设置过渡时不写 /Trans");

    let second = page_dict(&doc, 5);
    assert!(second.get("Dur").is_none(), "未设置时长时不写 /Dur");
    assert!(second.get("Trans").is_none());
}

#[test]
fn page_presentation_requires_pdf_1_1() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    let p0 = b.page(PageSize::A4);
    b.set_page_duration(p0, 5.0);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.0 must reject /Dur");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    let p0 = b.page(PageSize::A4);
    b.set_page_transition(p0, &Transition::new(TransitionStyle::Dissolve));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.0 must reject /Trans");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");
}

#[test]
fn page_transition_extended_entries_require_pdf_1_5() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    b.set_page_transition(
        p0,
        &Transition::new(TransitionStyle::Fly).with_direction(TransitionDirection::None),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.4 must reject Fly transitions");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    b.set_page_transition(p0, &Transition::new(TransitionStyle::Wipe));
    let doc = b.assemble_document().expect("PDF 1.1 styles work at 1.4");
    let trans = page_dict(&doc, 3)
        .get("Trans")
        .and_then(Value::as_dict)
        .expect("/Trans")
        .clone();
    assert_eq!(trans.get("S"), Some(&Value::Name("Wipe".into())));

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    b.set_page_transition(p0, &Transition::new(TransitionStyle::Fly).with_scale(0.5));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.4 must reject /SS transitions");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");
}

#[test]
fn invalid_page_presentation_values_are_rejected() {
    for seconds in [-1.0, f64::NAN, f64::INFINITY] {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.set_page_duration(p0, seconds);
        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("invalid /Dur must be rejected");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "unexpected error {error:?}"
        );
        assert!(out.is_empty(), "非法取值必须零字节");
    }

    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.set_page_transition(
        p0,
        &Transition::new(TransitionStyle::Wipe).with_dimension(TransitionDimension::Horizontal),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("style-specific entries must be validated");
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "非法取值必须零字节");
}
