//! 选择字段 `/TI` 与 `/I`（Table 231）：下标契约校验与 PDF 1.4 版本门禁。

use super::*;
use crate::core::writer::WriteError;

type BuildOutcome = (Result<(), WriteError>, Vec<u8>);

fn build(version: PdfVersion, field: FormField) -> BuildOutcome {
    let mut builder = PdfBuilder::with_version(version);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, field);
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    (result, out)
}

fn colors() -> FormField {
    FormField::choice("size", [72.0, 500.0, 200.0, 520.0])
        .with_option(FormOption::new("S"))
        .with_option(FormOption::new("M"))
        .with_option(FormOption::new("L"))
}

fn multiselect() -> FormField {
    colors().with_flags(FormField::MULTI_SELECT)
}

#[test]
fn choice_indices_roundtrip_through_field_reader() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        multiselect()
            .with_top_index(1)
            .with_alternate_indices([0, 2]),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");
    let form = doc.acro_form().expect("read").expect("form present");
    let field = &form.fields[0];
    assert_eq!(field.top_index, Some(1));
    assert_eq!(field.alternate_indices, Some(vec![0, 2]));
}

#[test]
fn choice_index_contract_is_enforced_at_build_time() {
    let cases: Vec<(&str, BuildOutcome)> = vec![
        (
            "/TI outside /Opt",
            build(PdfVersion::V1_4, colors().with_top_index(3)),
        ),
        (
            "/TI on a text field",
            build(
                PdfVersion::V1_4,
                FormField::text("name", [72.0, 500.0, 200.0, 520.0]).with_top_index(0),
            ),
        ),
        (
            "/TI without /Opt",
            build(
                PdfVersion::V1_4,
                FormField::choice("size", [72.0, 500.0, 200.0, 520.0]).with_top_index(0),
            ),
        ),
        (
            "/I without MultiSelect",
            build(PdfVersion::V1_4, colors().with_alternate_indices([0, 1])),
        ),
        (
            "/I is not ascending",
            build(
                PdfVersion::V1_4,
                multiselect().with_alternate_indices([2, 0]),
            ),
        ),
        (
            "/I repeats an index",
            build(
                PdfVersion::V1_4,
                multiselect().with_alternate_indices([1, 1]),
            ),
        ),
        (
            "/I outside /Opt",
            build(PdfVersion::V1_4, multiselect().with_alternate_indices([3])),
        ),
        (
            "/I on a button field",
            build(
                PdfVersion::V1_4,
                FormField::checkbox("box", [72.0, 500.0, 92.0, 520.0])
                    .with_flags(FormField::MULTI_SELECT)
                    .with_option(FormOption::new("x"))
                    .with_alternate_indices([0]),
            ),
        ),
    ];
    for (label, (result, bytes)) in cases {
        match result.expect_err(label) {
            WriteError::InvalidObject(_) => {}
            other => panic!("{label} should be InvalidObject, got {other:?}"),
        }
        assert!(bytes.is_empty(), "{label} must not write bytes");
    }
}

#[test]
fn choice_alternate_indices_are_version_gated_to_1_4() {
    let (result, bytes) = build(
        PdfVersion::V1_3,
        multiselect().with_alternate_indices([0, 1]),
    );
    match result.expect_err("PDF 1.3 /I should be rejected") {
        WriteError::UnsupportedVersion(message) => {
            assert!(
                message.contains("choice field alternate indices"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
    assert!(bytes.is_empty(), "rejected build must not write bytes");
}

#[test]
fn choice_top_index_is_not_version_gated() {
    let (result, _) = build(PdfVersion::V1_2, colors().with_top_index(2));
    result.expect("PDF 1.2 /TI is a plain choice field entry");
}
