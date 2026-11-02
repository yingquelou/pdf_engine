//! Hide 与 Set-OCG-State 动作测试（12.6.4.10/12.6.4.12）。

use crate::core::types::{Dict, Value};

use super::*;

#[test]
fn parses_hide_action_targets_and_flag() {
    let doc = document_with_pages(1);
    let annot_object = 60;
    insert(
        &doc,
        annot_object,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Annot"))
                .insert("Subtype", name("Text")),
        ),
    );

    let single = Value::Dict(action_dict("Hide").insert("T", Value::String("group.note".into())));
    assert_eq!(
        doc.action(&single).unwrap(),
        Action::Hide(HideAction {
            targets: vec![HideTarget::FieldName("group.note".into())],
            hide: None,
        })
    );

    let array = Value::Dict(
        action_dict("Hide")
            .insert(
                "T",
                arr(vec![
                    Value::Ref(annot_object, 0),
                    Value::String("group.note".into()),
                ]),
            )
            .insert("H", Value::Bool(false)),
    );
    assert_eq!(
        doc.action(&array).unwrap(),
        Action::Hide(HideAction {
            targets: vec![
                HideTarget::Annotation {
                    number: annot_object,
                    generation: 0,
                },
                HideTarget::FieldName("group.note".into()),
            ],
            hide: Some(false),
        })
    );
}

#[test]
fn malformed_hide_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(&doc, 61, name("not-an-annotation"));

    let cases = [
        Value::Dict(action_dict("Hide")),
        Value::Dict(action_dict("Hide").insert("T", arr(Vec::new()))),
        Value::Dict(action_dict("Hide").insert("T", Value::Integer(7))),
        Value::Dict(action_dict("Hide").insert("T", Value::Ref(61, 0))),
        Value::Dict(
            action_dict("Hide")
                .insert("T", Value::String("field".into()))
                .insert("H", Value::Integer(1)),
        ),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}

#[test]
fn parses_set_ocg_state_action() {
    let doc = document_with_pages(1);
    for num in [41, 42, 43] {
        insert(
            &doc,
            num,
            Value::Dict(Dict::new().insert("Type", name("OCG"))),
        );
    }

    let value = Value::Dict(
        action_dict("SetOCGState")
            .insert(
                "State",
                arr(vec![
                    name("OFF"),
                    Value::Ref(41, 0),
                    Value::Ref(42, 0),
                    name("Toggle"),
                    Value::Ref(43, 0),
                    name("Restore"),
                    Value::Ref(41, 0),
                ]),
            )
            .insert("PreserveRB", Value::Bool(false)),
    );
    assert_eq!(
        doc.action(&value).unwrap(),
        Action::SetOCGState(SetOcgStateAction {
            state: vec![
                OcgStateSequence {
                    operation: OcgStateOperation::Off,
                    groups: vec![(41, 0), (42, 0)],
                },
                OcgStateSequence {
                    operation: OcgStateOperation::Toggle,
                    groups: vec![(43, 0)],
                },
                OcgStateSequence {
                    operation: OcgStateOperation::Other("Restore".into()),
                    groups: vec![(41, 0)],
                },
            ],
            preserve_radio_buttons: Some(false),
        })
    );

    let minimal = Value::Dict(
        action_dict("SetOCGState").insert("State", arr(vec![name("ON"), Value::Ref(43, 0)])),
    );
    assert_eq!(
        doc.action(&minimal).unwrap(),
        Action::SetOCGState(SetOcgStateAction {
            state: vec![OcgStateSequence {
                operation: OcgStateOperation::On,
                groups: vec![(43, 0)],
            }],
            preserve_radio_buttons: None,
        })
    );
}

#[test]
fn malformed_set_ocg_state_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(
        &doc,
        41,
        Value::Dict(Dict::new().insert("Type", name("OCG"))),
    );
    insert(&doc, 61, name("not-an-ocg"));

    let cases = [
        Value::Dict(action_dict("SetOCGState")),
        Value::Dict(action_dict("SetOCGState").insert("State", Value::Integer(1))),
        Value::Dict(action_dict("SetOCGState").insert("State", arr(Vec::new()))),
        Value::Dict(action_dict("SetOCGState").insert("State", arr(vec![Value::Ref(41, 0)]))),
        Value::Dict(action_dict("SetOCGState").insert("State", arr(vec![name("ON")]))),
        Value::Dict(action_dict("SetOCGState").insert(
            "State",
            arr(vec![name("ON"), name("OFF"), Value::Ref(41, 0)]),
        )),
        Value::Dict(
            action_dict("SetOCGState").insert("State", arr(vec![name("ON"), Value::Integer(3)])),
        ),
        Value::Dict(
            action_dict("SetOCGState").insert("State", arr(vec![name("ON"), Value::Ref(61, 0)])),
        ),
        Value::Dict(
            action_dict("SetOCGState")
                .insert("State", arr(vec![name("ON"), Value::Ref(41, 0)]))
                .insert("PreserveRB", Value::Integer(1)),
        ),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}
