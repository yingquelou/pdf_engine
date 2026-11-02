//! 表单动作 SubmitForm / ResetForm / ImportData 测试（12.6.4.7/12.7.5）。

use crate::core::types::{Dict, Value};

use super::*;

#[test]
fn parses_submit_form_action_with_fields_and_flags() {
    let doc = document_with_pages(1);
    let field_object = 41;
    insert(
        &doc,
        field_object,
        Value::Dict(Dict::new().insert("T", Value::String("name".into()))),
    );

    let submit = Value::Dict(
        action_dict("SubmitForm")
            .insert(
                "F",
                Value::Dict(
                    Dict::new()
                        .insert("FS", name("URL"))
                        .insert("F", Value::String("https://example.com/submit".into())),
                ),
            )
            .insert(
                "Fields",
                arr(vec![
                    Value::String("name".into()),
                    Value::Ref(field_object, 0),
                ]),
            )
            .insert("Flags", Value::Integer(0x0003))
            .insert("URLType", name("Browser")),
    );
    assert_eq!(
        doc.action(&submit).unwrap(),
        Action::SubmitForm(SubmitFormAction {
            file: FileSpec {
                file: "https://example.com/submit".into()
            },
            url_type: Some("Browser".into()),
            fields: vec![
                ActionField::Name("name".into()),
                ActionField::Reference {
                    number: field_object,
                    generation: 0,
                },
            ],
            flags: 0x0003,
        })
    );

    let minimal = Value::Dict(
        action_dict("SubmitForm").insert("F", Value::String("https://example.com/s".into())),
    );
    assert_eq!(
        doc.action(&minimal).unwrap(),
        Action::SubmitForm(SubmitFormAction {
            file: FileSpec {
                file: "https://example.com/s".into()
            },
            url_type: None,
            fields: Vec::new(),
            flags: 0,
        })
    );
}

#[test]
fn parses_reset_form_and_import_data_actions() {
    let doc = document_with_pages(1);

    let reset = Value::Dict(
        action_dict("ResetForm")
            .insert("Fields", arr(vec![Value::String("group.total".into())]))
            .insert("Flags", Value::Integer(1)),
    );
    assert_eq!(
        doc.action(&reset).unwrap(),
        Action::ResetForm(ResetFormAction {
            fields: vec![ActionField::Name("group.total".into())],
            flags: 1,
        })
    );

    let reset_all = Value::Dict(action_dict("ResetForm"));
    assert_eq!(
        doc.action(&reset_all).unwrap(),
        Action::ResetForm(ResetFormAction {
            fields: Vec::new(),
            flags: 0,
        })
    );

    let import =
        Value::Dict(action_dict("ImportData").insert("F", Value::String("data.fdf".into())));
    assert_eq!(
        doc.action(&import).unwrap(),
        Action::ImportData {
            file: FileSpec {
                file: "data.fdf".into()
            },
        }
    );
}

#[test]
fn malformed_form_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(
        &doc,
        40,
        Value::Dict(Dict::new().insert("T", Value::String("name".into()))),
    );
    insert(&doc, 41, Value::String("not a field".into()));

    let cases = [
        Value::Dict(action_dict("SubmitForm")),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", Value::Integer(1)),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", arr(vec![Value::Integer(3)])),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", arr(vec![Value::Ref(41, 0)])),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", arr(vec![Value::Ref(99, 0)])),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Flags", Value::Integer(-1)),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Flags", Value::Float(1.0)),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("URLType", Value::String("Browser".into())),
        ),
        Value::Dict(action_dict("ResetForm").insert("Fields", arr(vec![Value::Bool(true)]))),
        Value::Dict(action_dict("ResetForm").insert("Flags", Value::Integer(-2))),
        Value::Dict(action_dict("ImportData")),
        Value::Dict(action_dict("ImportData").insert("F", Value::Integer(1))),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}
