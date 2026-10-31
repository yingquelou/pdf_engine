//! Printer's mark 注解写端（14.11.3，Table 362/363）：`/F` 恒为 68、
//! `/MN` 与 `Table 363` 表单条目的写出、PDF 1.4 门禁与零字节校验契约。

use super::support::annot_object;
use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::content::ContentStream;
use crate::core::types::{PdfFeature, Value};

fn separation(colorant: &str) -> ColorSpace {
    ColorSpace::Separation(Separation::new(
        colorant,
        ColorSpace::DeviceGray,
        Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
    ))
}

fn form() -> FormXObject {
    FormXObject::new([0.0, 0.0, 100.0, 100.0], ContentStream::new())
}

fn printer_mark(rect: [f64; 4]) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::PrinterMark, rect)
}

/// 构建单页文档并按指定版本、可选外观写出。
fn build_one(
    annot: PdfAnnotation,
    form: Option<FormXObject>,
    version: PdfVersion,
) -> (Result<(), WriteError>, Vec<u8>) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    let annot = match form {
        Some(form) => {
            let handle = b.form_xobject(&form);
            annot.with_appearance(handle)
        }
        None => annot,
    };
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    let result = b.build(&mut out);
    (result, out)
}

/// 断言构建失败、消息命中且未写出任何字节。
fn expect_rejected(annot: PdfAnnotation, fragment: &str) {
    let (result, out) = build_one(annot, None, PdfVersion::V1_4);
    let error = result.expect_err("printer's mark contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "unexpected message: {message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

/// 表单条目校验失败同样在写出字节前拒绝。
fn expect_form_rejected(form: FormXObject, annot: PdfAnnotation, fragment: &str) {
    let (result, out) = build_one(annot, Some(form), PdfVersion::V1_4);
    let error = result.expect_err("printer's mark form contract violation");
    let message = error.to_string();
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "expected InvalidObject, got {message}"
    );
    assert!(message.contains(fragment), "unexpected message: {message}");
    assert_eq!(out.len(), 0, "rejected build must not emit bytes");
}

#[test]
fn printer_mark_writes_flags_appearance_and_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    let handle = b.form_xobject(
        &form()
            .with_mark_style("Cut")
            .with_colorant("Cyan", separation("Cyan")),
    );
    b.add_annotation(
        p0,
        printer_mark([10.0, 10.0, 20.0, 20.0])
            .with_printer_mark_name("Registration")
            .with_appearance(handle),
    );
    let doc = b.assemble_document().expect("assemble");
    assert!(
        doc.features.borrow().contains(PdfFeature::PrinterMarks),
        "Printer's mark 应写入特性画像"
    );

    let annot = annot_object(&doc, 3, 0);
    assert_eq!(
        annot.get("Subtype"),
        Some(&Value::Name("PrinterMark".into()))
    );
    assert_eq!(
        annot.get("F"),
        Some(&Value::Integer(68)),
        "未显式设置标志位时仍须写出 Print | ReadOnly"
    );
    assert_eq!(annot.get("MN"), Some(&Value::Name("Registration".into())));
    let normal = annot
        .get("AP")
        .and_then(Value::as_dict)
        .and_then(|ap| ap.get("N"))
        .and_then(Value::as_ref)
        .expect("/AP /N reference");
    let object = doc
        .get_object(normal.0)
        .expect("form object")
        .expect("form exists");
    let Value::Stream(form_dict, _) = &object.value else {
        panic!("appearance must be a form stream");
    };
    assert_eq!(form_dict.get("Subtype"), Some(&Value::Name("Form".into())));
    assert_eq!(
        form_dict.get("MarkStyle"),
        Some(&text_string_value("Cut")),
        "Table 363 的 /MarkStyle 为文本串"
    );
    let colorants = form_dict
        .get("Colorants")
        .and_then(Value::as_dict)
        .expect("/Colorants dictionary");
    let cyan = colorants
        .get("Cyan")
        .and_then(Value::as_array)
        .expect("/Colorants Cyan");
    assert_eq!(cyan.0[0], Value::Name("Separation".into()));
    assert_eq!(cyan.0[1], Value::Name("Cyan".into()));
    assert_eq!(cyan.0[2], Value::Name("DeviceGray".into()));
    assert!(matches!(cyan.0[3], Value::Dict(_)), "{cyan:?}");
}

#[test]
fn printer_mark_requires_pdf_1_4() {
    // 注解子类型自 PDF 1.4 引入（Table 169）。
    let (result, out) = build_one(
        printer_mark([0.0, 0.0, 10.0, 10.0]),
        Some(form()),
        PdfVersion::V1_3,
    );
    let error = result.expect_err("PrinterMark below PDF 1.4");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("PrinterMark annotations require PDF 1.4")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    // Table 363 表单条目独立门禁：1.3 的普通外观表单也不能带 /MarkStyle。
    let (result, out) = build_one(
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]),
        Some(form().with_mark_style("Cut")),
        PdfVersion::V1_3,
    );
    let error = result.expect_err("form mark entries below PDF 1.4");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("printer's mark form entries")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    let (result, out) = build_one(
        printer_mark([0.0, 0.0, 10.0, 10.0]),
        Some(form()),
        PdfVersion::V1_4,
    );
    result.expect("PrinterMark is writable from PDF 1.4");
    assert!(!out.is_empty());
}

#[test]
fn invalid_printer_mark_entries_are_rejected_without_output() {
    // `/F` 只允许 0（由写出补 68）或 68。
    expect_rejected(
        printer_mark([0.0, 0.0, 10.0, 10.0]).with_flags(4),
        "/F must be 0 or 68",
    );
    // `/AP` 正常外观必填。
    expect_rejected(
        printer_mark([0.0, 0.0, 10.0, 10.0]),
        "requires a normal appearance",
    );
    // 多状态外观必须声明 `/AS`。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    let on = b.form_xobject(&form());
    let off = b.form_xobject(&form());
    b.add_annotation(
        p0,
        printer_mark([0.0, 0.0, 10.0, 10.0])
            .with_appearance_state("On", on)
            .with_appearance_state("Off", off),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("multiple appearances without /AS");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("multiple appearances"),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    // `/MN` 只属于 PrinterMark 注解。
    expect_form_rejected(
        form(),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0])
            .with_printer_mark_name("Registration"),
        "specific to PrinterMark",
    );

    // `/Colorants` 值须为 Separation，色料名与键一致且不得重复。
    expect_form_rejected(
        form().with_colorant("Cyan", ColorSpace::DeviceRGB),
        printer_mark([0.0, 0.0, 10.0, 10.0]),
        "must be a Separation color space",
    );
    expect_form_rejected(
        form().with_colorant("Magenta", separation("Cyan")),
        printer_mark([0.0, 0.0, 10.0, 10.0]),
        "does not match its Separation colorant",
    );
    expect_form_rejected(
        form()
            .with_colorant("Cyan", separation("Cyan"))
            .with_colorant("Cyan", separation("Cyan")),
        printer_mark([0.0, 0.0, 10.0, 10.0]),
        "registered twice",
    );
    expect_form_rejected(
        form().with_colorant("Bad Name", separation("Bad Name")),
        printer_mark([0.0, 0.0, 10.0, 10.0]),
        "printer's mark form colorant name",
    );
}
