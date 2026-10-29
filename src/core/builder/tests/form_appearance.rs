//! Widget 常态外观状态子字典测试（12.5.5 Table 168 / 12.7.4 Table 189）：
//! `/AP /N << /state 句柄 >>` 的写出、`/AS` 登记校验与读端回读。

use super::support::annot_object;
use super::*;
use crate::core::content::ContentStream;
use crate::core::writer::WriteError;

fn form_handle(builder: &mut PdfBuilder) -> usize {
    builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ))
}

fn build_field(field: FormField) -> Result<Vec<u8>, WriteError> {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, field);
    builder.set_acro_form(form);
    let mut out = Vec::new();
    builder.build(&mut out)?;
    Ok(out)
}

#[test]
fn radio_widget_writes_normal_appearance_states() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let off = form_handle(&mut builder);
    let on = form_handle(&mut builder);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::radio("agree", [72.0, 590.0, 92.0, 610.0])
            .with_appearance_state("Off", off)
            .with_appearance_state("Yes", on)
            .with_selected_appearance_state("Yes"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("AS"), Some(&Value::Name("Yes".into())));
    let normal = dict
        .get("AP")
        .and_then(|value| value.as_dict())
        .and_then(|ap| ap.get("N"))
        .and_then(|value| value.as_dict())
        .expect("/AP /N state dictionary");
    assert_eq!(normal.0.len(), 2);
    let written_off = normal.get("Off").expect("Off state entry");
    let written_on = normal.get("Yes").expect("Yes state entry");
    assert_ne!(written_off, written_on, "states reference distinct forms");
    for (state, value) in [("Off", written_off), ("Yes", written_on)] {
        let (num, _) = value
            .as_ref()
            .unwrap_or_else(|| panic!("{state} state entry is a reference"));
        let object = doc.get_object(num).expect("lookup").expect("exists");
        assert!(matches!(object.value, Value::Stream(ref dict, _)
            if dict.get("Subtype") == Some(&Value::Name("Form".into()))));
    }

    let parsed = doc.acro_form().expect("read").expect("form present");
    let field = &parsed.fields[0];
    assert_eq!(field.appearance_state.as_deref(), Some("Yes"));
    let states: Vec<(&str, &Value)> = field
        .appearance_normal_states
        .iter()
        .map(|(state, value)| (state.as_str(), value))
        .collect();
    assert_eq!(
        states,
        vec![("Off", written_off), ("Yes", written_on)],
        "state names are sorted and references preserved"
    );
}

#[test]
fn radio_group_widgets_write_per_widget_state_dictionaries() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let off = form_handle(&mut builder);
    let first = form_handle(&mut builder);
    let second = form_handle(&mut builder);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::radio_group("agree")
            .with_child(
                FormField::widget([72.0, 560.0, 92.0, 580.0])
                    .with_appearance_state("Off", off)
                    .with_appearance_state("Choice1", first)
                    .with_selected_appearance_state("Choice1"),
            )
            .with_child(
                FormField::widget([100.0, 560.0, 120.0, 580.0])
                    .with_appearance_state("Off", off)
                    .with_appearance_state("Choice2", second)
                    .with_selected_appearance_state("Off"),
            ),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let parsed = doc.acro_form().expect("read").expect("form present");
    let group = &parsed.fields[0];
    assert_eq!(group.kids.len(), 2);
    assert_eq!(group.appearance_normal_states, Vec::new());
    assert_eq!(
        group.kids[0]
            .appearance_normal_states
            .iter()
            .map(|(state, _)| state.as_str())
            .collect::<Vec<_>>(),
        vec!["Choice1", "Off"]
    );
    assert_eq!(group.kids[0].appearance_state.as_deref(), Some("Choice1"));
    assert_eq!(group.kids[1].appearance_state.as_deref(), Some("Off"));
}

#[test]
fn invalid_appearance_states_are_rejected() {
    let cases: Vec<FormField> = vec![
        FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0]).with_appearance_state("", 0),
        FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0]).with_appearance_state("Bad Name", 0),
        FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0])
            .with_appearance_state("On", 0)
            .with_appearance_state("On", 0),
        FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0])
            .with_appearance(0)
            .with_appearance_state("On", 0),
        FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0])
            .with_appearance_state("On", 0)
            .with_selected_appearance_state("Off"),
        FormField::checkbox("box", [72.0, 540.0, 92.0, 560.0])
            .with_appearance_state("On", 99)
            .with_selected_appearance_state("On"),
        FormField::group("group")
            .with_appearance_state("On", 0)
            .with_child(FormField::widget([72.0, 540.0, 92.0, 560.0])),
    ];
    for field in cases {
        let error = build_field(field).expect_err("invalid appearance states must fail");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "unexpected error: {error:?}"
        );
    }
}
