//! 按钮字段 `/Opt`（Table 227）：Widget 开状态位置化、纯文本条目与
//! PDF 1.4 版本门禁。

use super::*;
use crate::core::content::ContentStream;
use crate::core::writer::WriteError;

fn form_handle(builder: &mut PdfBuilder) -> usize {
    builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ))
}

fn build(
    version: PdfVersion,
    make: impl FnOnce(&mut PdfBuilder) -> FormField,
) -> (Result<(), WriteError>, Vec<u8>) {
    let mut builder = PdfBuilder::with_version(version);
    let page = builder.page(PageSize::A4);
    let field = make(&mut builder);
    let mut form = AcroForm::new();
    form.add_field(page, field);
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    (result, out)
}

#[test]
fn radio_group_options_roundtrip_with_positional_states() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let off = form_handle(&mut builder);
    let first = form_handle(&mut builder);
    let second = form_handle(&mut builder);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::radio_group("agree")
            .with_option(FormOption::new("first"))
            .with_option(FormOption::new("second"))
            .with_child(
                FormField::widget([72.0, 560.0, 92.0, 580.0])
                    .with_appearance_state("Off", off)
                    .with_appearance_state("0", first)
                    .with_selected_appearance_state("0"),
            )
            .with_child(
                FormField::widget([100.0, 560.0, 120.0, 580.0])
                    .with_appearance_state("Off", off)
                    .with_appearance_state("1", second)
                    .with_selected_appearance_state("Off"),
            ),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");
    let parsed = doc.acro_form().expect("read").expect("form present");
    let group = &parsed.fields[0];
    let options = group.options.as_ref().expect("/Opt");
    assert_eq!(
        options
            .iter()
            .map(|option| (option.export(), option.display()))
            .collect::<Vec<_>>(),
        vec![("first", None), ("second", None)]
    );
    assert_eq!(
        group.kids[0].options.as_ref(),
        Some(options),
        "widgets inherit the group's /Opt (Table 227)"
    );
    assert_eq!(group.kids[0].appearance_state.as_deref(), Some("0"));
    assert_eq!(
        group.kids[1]
            .appearance_normal_states
            .iter()
            .map(|(state, _)| state.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "Off"]
    );
}

type BuildOutcome = (Result<(), WriteError>, Vec<u8>);

#[test]
fn button_option_contract_is_enforced_at_build_time() {
    let cases: Vec<(&str, BuildOutcome)> = vec![
        (
            "non-positional on state",
            build(PdfVersion::V1_4, |builder| {
                let off = form_handle(builder);
                let on = form_handle(builder);
                FormField::radio_group("agree")
                    .with_option(FormOption::new("first"))
                    .with_child(
                        FormField::widget([72.0, 560.0, 92.0, 580.0])
                            .with_appearance_state("Off", off)
                            .with_appearance_state("Choice1", on),
                    )
            }),
        ),
        (
            "entry count mismatch",
            build(PdfVersion::V1_4, |builder| {
                let off = form_handle(builder);
                let on = form_handle(builder);
                FormField::radio_group("agree")
                    .with_option(FormOption::new("first"))
                    .with_option(FormOption::new("second"))
                    .with_child(
                        FormField::widget([72.0, 560.0, 92.0, 580.0])
                            .with_appearance_state("Off", off)
                            .with_appearance_state("0", on),
                    )
            }),
        ),
        (
            "export/display pair on a button",
            build(PdfVersion::V1_4, |_| {
                FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0])
                    .with_option(FormOption::new("x").with_display("X"))
            }),
        ),
        (
            "option on a pushbutton",
            build(PdfVersion::V1_4, |_| {
                FormField::push_button("go", [72.0, 520.0, 92.0, 540.0])
                    .with_option(FormOption::new("x"))
            }),
        ),
        (
            "option on a text field",
            build(PdfVersion::V1_4, |_| {
                FormField::text("name", [72.0, 500.0, 200.0, 520.0])
                    .with_option(FormOption::new("x"))
            }),
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
fn button_options_are_version_gated_to_1_4() {
    let (result, bytes) = build(PdfVersion::V1_3, |_| {
        FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0]).with_option(FormOption::new("x"))
    });
    match result.expect_err("PDF 1.3 button /Opt should be rejected") {
        WriteError::UnsupportedVersion(message) => {
            assert!(
                message.contains("button form field options"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
    assert!(bytes.is_empty(), "rejected build must not write bytes");
}
