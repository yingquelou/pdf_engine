//! `/Opt` 类型化选项测试（Table 227 按钮 / Table 231 选择）。

use super::*;

#[test]
fn choice_options_parse_export_and_display_pairs() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("FT", name("Ch")).insert(
                "Opt",
                arr(vec![text("Plain"), arr(vec![text("R"), text("Red")])]),
            ),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    let options = form.fields[0].options.as_ref().expect("/Opt");
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].export(), "Plain");
    assert_eq!(options[0].display(), None);
    assert_eq!(options[1].export(), "R");
    assert_eq!(options[1].display(), Some("Red"));
}

#[test]
fn button_options_are_plain_on_states_and_inherit_to_widgets() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("FT", name("Btn"))
                .insert("Ff", Value::Integer((1 << 14) | (1 << 15)))
                .insert("T", text("agree"))
                .insert("Opt", arr(vec![text("first"), text("second")]))
                .insert(
                    "Kids",
                    arr(vec![
                        Value::Dict(widget(arr(vec![
                            Value::Integer(0),
                            Value::Integer(0),
                            Value::Integer(20),
                            Value::Integer(20),
                        ]))),
                        Value::Dict(widget(arr(vec![
                            Value::Integer(30),
                            Value::Integer(0),
                            Value::Integer(50),
                            Value::Integer(20),
                        ]))),
                    ]),
                ),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    let group = &form.fields[0];
    assert!(group.flags.radio());
    let options = group.options.as_ref().expect("/Opt");
    assert_eq!(
        options
            .iter()
            .map(|option| (option.export(), option.display()))
            .collect::<Vec<_>>(),
        vec![("first", None), ("second", None)]
    );
    for kid in &group.kids {
        assert_eq!(kid.options.as_ref(), Some(options), "widgets inherit /Opt");
    }
}

#[test]
fn choice_options_are_not_inherited() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("FT", name("Ch"))
                .insert("T", text("color"))
                .insert("Opt", arr(vec![text("Red"), text("Blue")]))
                .insert(
                    "Kids",
                    arr(vec![
                        Value::Dict(
                            Dict::new()
                                .insert("T", text("inner"))
                                .insert("Opt", arr(vec![text("Green")])),
                        ),
                        Value::Dict(Dict::new().insert("T", text("other"))),
                    ]),
                ),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    let parent = &form.fields[0];
    assert_eq!(parent.options.as_ref().map(Vec::len), Some(2));
    let child = &parent.kids[0];
    assert_eq!(child.field_type, Some(FieldType::Choice));
    assert_eq!(
        child.options.as_ref().map(|options| options[0].export()),
        Some("Green"),
        "child declares its own /Opt"
    );
    assert_eq!(
        parent.kids[1].options, None,
        "choice /Opt is not inherited (Table 231)"
    );
}

#[test]
fn malformed_option_entries_are_typed_errors() {
    let cases: Vec<(&str, Value, Value)> = vec![
        ("integer element", name("Ch"), Value::Integer(1)),
        (
            "pair with three elements",
            name("Ch"),
            arr(vec![arr(vec![text("A"), text("B"), text("C")])]),
        ),
        (
            "pair with non-text display",
            name("Ch"),
            arr(vec![arr(vec![text("A"), Value::Integer(2)])]),
        ),
        (
            "nested array on button",
            name("Btn"),
            arr(vec![arr(vec![text("A"), text("B")])]),
        ),
        (
            "integer element on button",
            name("Btn"),
            arr(vec![Value::Integer(1)]),
        ),
        ("non-array /Opt", name("Ch"), Value::Integer(3)),
    ];
    for (label, field_type, opt) in cases {
        let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("FT", field_type).insert("Opt", opt),
            )]),
        )));
        let error = doc.acro_form().expect_err(label);
        assert!(
            matches!(error, FormError::Malformed(_)),
            "{label} should be malformed: {error}"
        );
    }
}
