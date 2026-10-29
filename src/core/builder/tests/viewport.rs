//! 页面视口字典（页面 `/VP`，12.9.2 Table 260，PDF 1.6）写端测试：
//! 数组写出、`/BBox` `/Name` `/Measure` 条目、版本门禁与取值校验。

use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::types::{PdfFeature, PdfVersion};

fn minimal_measure() -> Measure {
    let formats = || vec![NumberFormat::new("pt", 1.0)];
    Measure::new("1:1", formats(), formats(), formats())
}

fn page_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("page object")
        .expect("page exists")
        .value
        .as_dict()
        .expect("page dict")
        .clone()
}

fn build_error(viewport: Viewport) -> (WriteError, usize) {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p = b.page(PageSize::A4);
    b.set_page_viewports(p, &[viewport]);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("invalid viewport must fail");
    (error, out.len())
}

#[test]
fn page_viewports_write_vp_array_with_measure() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    let p1 = b.page(PageSize::A4);
    b.text(p1, &Font::helvetica(12.0), 72.0, 700.0, "Second");
    b.set_page_viewports(
        p0,
        &[
            Viewport::new([10.0, 20.0, 300.0, 400.0])
                .with_name("Plan")
                .with_measure(minimal_measure()),
            Viewport::new([0.0, 0.0, 50.0, 60.0]),
        ],
    );
    let doc = b.assemble_document().expect("assemble");

    let vp = page_dict(&doc, 3)
        .get("VP")
        .and_then(Value::as_array)
        .expect("page /VP")
        .clone();
    assert_eq!(vp.0.len(), 2);
    let first = vp.0[0].as_dict().expect("/VP[0]");
    assert_eq!(first.get("Type"), Some(&Value::Name("Viewport".into())));
    let bbox = first.get("BBox").and_then(Value::as_array).expect("/BBox");
    assert_eq!(
        bbox.0,
        vec![
            Value::Float(10.0),
            Value::Float(20.0),
            Value::Float(300.0),
            Value::Float(400.0)
        ]
    );
    assert_eq!(first.get("Name"), Some(&text_string_value("Plan")));
    let measure = first
        .get("Measure")
        .and_then(Value::as_dict)
        .expect("/Measure");
    assert_eq!(measure.get("Type"), Some(&Value::Name("Measure".into())));
    assert_eq!(measure.get("Subtype"), Some(&Value::Name("RL".into())));
    assert_eq!(measure.get("R"), Some(&text_string_value("1:1")));

    let second = vp.0[1].as_dict().expect("/VP[1]");
    assert_eq!(second.get("Type"), Some(&Value::Name("Viewport".into())));
    assert!(second.get("Name").is_none(), "未设置名称时不写 /Name");
    assert!(second.get("Measure").is_none(), "未设置测量时不写 /Measure");

    assert!(
        page_dict(&doc, 5).get("VP").is_none(),
        "未设置视口的页不应写 /VP"
    );
    assert!(doc.features.borrow().contains(PdfFeature::PageViewports));
}

#[test]
fn empty_viewport_array_leaves_page_without_vp() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p = b.page(PageSize::A4);
    b.set_page_viewports(p, &[]);
    let doc = b.assemble_document().expect("assemble");
    assert!(page_dict(&doc, 3).get("VP").is_none());
    assert!(!doc.features.borrow().contains(PdfFeature::PageViewports));
}

#[test]
fn page_viewports_replace_previous_array() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p = b.page(PageSize::A4);
    b.set_page_viewports(p, &[Viewport::new([0.0, 0.0, 1.0, 1.0])]);
    b.set_page_viewports(
        p,
        &[
            Viewport::new([0.0, 0.0, 2.0, 2.0]),
            Viewport::new([0.0, 0.0, 3.0, 3.0]),
        ],
    );
    // 未知页索引被忽略。
    b.set_page_viewports(9, &[Viewport::new([0.0, 0.0, 4.0, 4.0])]);
    let doc = b.assemble_document().expect("assemble");
    let vp = page_dict(&doc, 3)
        .get("VP")
        .and_then(Value::as_array)
        .expect("/VP")
        .clone();
    assert_eq!(vp.0.len(), 2);
    let second = vp.0[1]
        .as_dict()
        .and_then(|dict| dict.get("BBox"))
        .and_then(Value::as_array)
        .expect("/BBox");
    assert_eq!(
        second.0,
        vec![
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(3.0),
            Value::Float(3.0)
        ]
    );
}

#[test]
fn page_viewports_require_pdf_1_6() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p = b.page(PageSize::A4);
    b.set_page_viewports(p, &[Viewport::new([0.0, 0.0, 10.0, 10.0])]);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.5 rejects /VP");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("page viewport") && message.contains("1.6")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p = b.page(PageSize::A4);
    b.set_page_viewports(p, &[Viewport::new([0.0, 0.0, 10.0, 10.0])]);
    let mut out = Vec::new();
    b.build(&mut out).expect("/VP at 1.6");
    assert!(!out.is_empty());
}

#[test]
fn invalid_page_viewports_fail_the_build() {
    let cases = [
        Viewport::new([f64::NAN, 0.0, 10.0, 10.0]),
        Viewport::new([0.0, 0.0, f64::INFINITY, 10.0]),
        Viewport::new([10.0, 0.0, 0.0, 10.0]),
        Viewport::new([0.0, 10.0, 10.0, 0.0]),
        Viewport::new([0.0, 0.0, 10.0, 10.0]).with_name(""),
        Viewport::new([0.0, 0.0, 10.0, 10.0]).with_measure(Measure::new(
            "",
            vec![],
            vec![],
            vec![],
        )),
    ];
    for viewport in cases {
        let (error, written) = build_error(viewport);
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("viewport"), "{message}");
        assert_eq!(written, 0);
    }
}
