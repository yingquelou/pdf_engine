//! Table 220 `/TU` 替代字段名（12.7.2，`Optional; PDF 1.3`）：`with_alternate_name`
//! 写出替代字段名（非 ASCII 走 UTF-16BE）、`with_tooltip` 别名与逐字段类型化
//! 回读、`has_alternate_names` 覆盖子字段与无名 Widget 表示的门禁谓词，以及
//! PDF 1.2 门禁谓词与端到端零字节契约。

use super::super::*;
use super::support::{acro_dict, assert_text_entry, object_dict};
use crate::core::types::PdfVersion;
use crate::core::writer::WriteError;

/// Table 220 `/TU`：`with_alternate_name` 写出替代字段名（非 ASCII 走
/// UTF-16BE），`with_tooltip` 为其别名；解析回读逐字段类型化，缺省不物化。
#[test]
fn alternate_name_is_written_as_tu_and_read_back() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 200.0, 720.0]).with_alternate_name("Full name"),
    );
    form.add_field(
        page,
        FormField::text("alias", [72.0, 660.0, 200.0, 680.0]).with_tooltip("Tooltip alias"),
    );
    form.add_field(
        page,
        FormField::text("plain", [72.0, 620.0, 200.0, 640.0]).with_alternate_name("姓名 ✓"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let fields = acro_dict(&doc)
        .get("Fields")
        .and_then(|value| value.as_array())
        .expect("/Fields")
        .clone();
    for entry in &fields.0 {
        let (num, _) = entry.as_ref().expect("field reference");
        assert_text_entry(&object_dict(&doc, num), "TU");
    }

    let parsed = doc.acro_form().expect("read").expect("form present");
    assert_eq!(
        parsed
            .fields
            .iter()
            .map(|field| field.alternate_name.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Full name"), Some("Tooltip alias"), Some("姓名 ✓")],
        "/TU round-trips in declaration order; alias matches the primary setter"
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, FormField::text("plain", [72.0, 700.0, 200.0, 720.0]));
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");
    let parsed = doc.acro_form().expect("read").expect("form present");
    assert_eq!(
        parsed.fields[0].alternate_name, None,
        "without /TU the model stays None"
    );
}

/// `has_alternate_names` 须覆盖子字段与无名 Widget 表示（12.7.3.2）。
#[test]
fn has_alternate_names_walks_children_and_widget_representations() {
    let rect = [72.0, 700.0, 200.0, 720.0];
    let mut nested = AcroForm::new();
    nested.add_field(
        0,
        FormField::group("person").with_child(
            FormField::text("name", rect)
                .with_child(FormField::widget(rect).with_alternate_name("Widget representation")),
        ),
    );
    assert!(
        nested.has_alternate_names(),
        "/TU on a nested representation counts"
    );

    let mut plain = AcroForm::new();
    plain.add_field(0, FormField::text("name", rect));
    assert!(
        !plain.has_alternate_names(),
        "a form without /TU must not trigger the gate"
    );
}

/// PDF 1.3 门禁谓词（12.7.2 Table 220 标注 `Optional; PDF 1.3`）：
/// 目标版本低于 1.3 且声明了 `/TU` 时返回类型化 `UnsupportedVersion`；
/// 端到端零字节行为见 [`alternate_name_gate_rejects_pdf_1_2_build_without_bytes`]。
#[test]
fn alternate_name_version_gate_requires_pdf_1_3() {
    use crate::core::types::PdfFeature;

    assert_eq!(
        PdfFeature::FormFieldAlternateName.min_version(),
        PdfVersion::V1_3
    );
    assert!(!PdfVersion::V1_2.supports(PdfFeature::FormFieldAlternateName));
    assert!(PdfVersion::V1_3.supports(PdfFeature::FormFieldAlternateName));

    let mut form = AcroForm::new();
    form.add_field(
        0,
        FormField::text("name", [72.0, 700.0, 200.0, 720.0]).with_alternate_name("Full name"),
    );
    let error = form
        .validate_alternate_name_version(PdfVersion::V1_2)
        .expect_err("PDF 1.2 rejects /TU");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("form field alternate names")),
        "unexpected error: {error:?}"
    );
    assert!(
        form.validate_alternate_name_version(PdfVersion::V1_3)
            .is_ok()
    );

    let plain = AcroForm::new();
    assert!(
        plain
            .validate_alternate_name_version(PdfVersion::V1_2)
            .is_ok()
    );
}

/// 端到端门禁：`PdfBuilder::build` 在 PDF 1.2 上写出声明 `/TU` 的字段树时
/// 返回类型化 `UnsupportedVersion` 且不写出任何字节；1.3 起正常写出。
#[test]
fn alternate_name_gate_rejects_pdf_1_2_build_without_bytes() {
    let mut out = Vec::new();
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_2);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 200.0, 720.0]).with_alternate_name("Full name"),
    );
    builder.set_acro_form(form);
    let error = builder
        .build(&mut out)
        .expect_err("PDF 1.2 must reject /TU");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(
        format!("{error}").contains("form field alternate names"),
        "{error}"
    );
    assert_eq!(out.len(), 0, "gate must fire before any byte is written");

    let mut out = Vec::new();
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 200.0, 720.0]).with_alternate_name("Full name"),
    );
    builder.set_acro_form(form);
    builder.build(&mut out).expect("PDF 1.3 accepts /TU");
    assert!(!out.is_empty());
}
