//! 版本门禁：AcroForm 整体要求 PDF 1.2（12.7.2），字段 `/AA` 附加动作要求
//! PDF 1.3（12.7.5.2）；低版本返回类型化 `UnsupportedVersion` 且不写出任何字节，
//! 达标版本正常写出。

use super::super::*;
use super::support::acro_dict;
use crate::core::types::PdfVersion;
use crate::core::writer::WriteError;

#[test]
fn acro_form_requires_pdf_1_2() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_1);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, FormField::text("name", [72.0, 700.0, 200.0, 720.0]));
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder.build(&mut out).expect_err("1.1 rejects forms");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error: {error}"
    );
    assert!(out.is_empty());
}

#[test]
fn field_actions_require_pdf_1_3() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_2);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("amount", [72.0, 700.0, 200.0, 720.0]).with_action(
            FieldActionEvent::Keystroke,
            Action::JavaScript("recalc()".into()),
        ),
    );
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder.build(&mut out).expect_err("PDF 1.2 rejects /AA");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message) if message.contains("form field additional actions")),
        "unexpected error: {error:?}"
    );
    assert!(out.is_empty());

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("amount", [72.0, 700.0, 200.0, 720.0]).with_action(
            FieldActionEvent::Calculate,
            Action::JavaScript("recalc()".into()),
        ),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("PDF 1.3 accepts /AA");
    assert!(
        acro_dict(&doc).get("CO").is_some(),
        "/CO written for /C fields"
    );
}
