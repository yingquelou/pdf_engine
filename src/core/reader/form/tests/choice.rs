//! 选择字段 `/TI` 与 `/I` 的类型化读取（Table 231）。

use super::*;

fn options_entry() -> Value {
    arr(vec![
        text("R"),
        arr(vec![text("G"), text("Green")]),
        text("B"),
    ])
}

fn rect_entry() -> Value {
    arr(vec![
        Value::Integer(0),
        Value::Integer(0),
        Value::Integer(20),
        Value::Integer(20),
    ])
}

fn choice_doc(extra: Vec<(&str, Value)>) -> Document {
    let mut dict = Dict::new()
        .insert("FT", name("Ch"))
        .insert("Opt", options_entry());
    for (key, value) in extra {
        dict = dict.insert(key, value);
    }
    doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(dict)])),
    ))
}

#[test]
fn choice_top_index_and_alternate_indices_are_typed() {
    let doc = choice_doc(vec![
        ("TI", Value::Integer(1)),
        ("I", arr(vec![Value::Integer(0), Value::Integer(2)])),
    ]);
    let form = doc.acro_form().expect("parse").expect("present");
    let field = &form.fields[0];
    assert_eq!(field.options.as_ref().expect("/Opt").len(), 3);
    assert_eq!(field.top_index, Some(1));
    assert_eq!(field.alternate_indices, Some(vec![0, 2]));
}

#[test]
fn choice_indices_absent_when_not_declared() {
    let doc = choice_doc(Vec::new());
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(form.fields[0].top_index, None);
    assert_eq!(form.fields[0].alternate_indices, None);
}

#[test]
fn choice_indices_are_not_inherited_by_widgets() {
    let widget = Dict::new()
        .insert("Type", name("Annot"))
        .insert("Subtype", name("Widget"))
        .insert("Opt", options_entry())
        .insert("Rect", rect_entry());
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("FT", name("Ch"))
                .insert("Opt", options_entry())
                .insert("TI", Value::Integer(2))
                .insert("Kids", arr(vec![Value::Dict(widget)])),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(form.fields[0].top_index, Some(2));
    assert_eq!(form.fields[0].kids[0].top_index, None);
    assert_eq!(form.fields[0].kids[0].alternate_indices, None);
}

#[test]
fn malformed_choice_indices_are_typed_errors() {
    let cases: Vec<(&str, Vec<(&str, Value)>)> = vec![
        ("/TI is not an integer", vec![("TI", name("1"))]),
        ("/TI is negative", vec![("TI", Value::Integer(-1))]),
        ("/TI is outside /Opt", vec![("TI", Value::Integer(3))]),
        ("/I is not an array", vec![("I", Value::Integer(1))]),
        (
            "/I entry is not an integer",
            vec![("I", arr(vec![name("0")]))],
        ),
        (
            "/I is not ascending",
            vec![("I", arr(vec![Value::Integer(2), Value::Integer(0)]))],
        ),
        (
            "/I repeats an index",
            vec![("I", arr(vec![Value::Integer(1), Value::Integer(1)]))],
        ),
        (
            "/I is outside /Opt",
            vec![("I", arr(vec![Value::Integer(3)]))],
        ),
    ];
    for (label, extra) in cases {
        let error = choice_doc(extra).acro_form().expect_err(label);
        match error {
            FormError::Malformed(_) => {}
            other => panic!("{label} should be Malformed, got {other:?}"),
        }
    }
}

#[test]
fn choice_indices_require_an_opt_array() {
    for (label, key, value) in [
        ("TI without /Opt", "TI", Value::Integer(0)),
        ("I without /Opt", "I", arr(vec![Value::Integer(0)])),
    ] {
        let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("FT", name("Ch")).insert(key, value),
            )]),
        )));
        let error = doc.acro_form().expect_err(label);
        match error {
            FormError::Malformed(_) => {}
            other => panic!("{label} should be Malformed, got {other:?}"),
        }
    }
}
