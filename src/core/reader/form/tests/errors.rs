//! AcroForm 错误形态测试：缺省默认值、类型化 `Malformed` 与资源上限（12.7.2）。

use super::*;

#[test]
fn missing_acro_form_and_defaults() {
    let doc = doc_with_catalog(Dict::new());
    assert!(doc.acro_form().expect("parse").is_none());

    let doc = doc_with_acro_form(Value::Dict(Dict::new()));
    let form = doc.acro_form().expect("parse").expect("present");
    assert!(form.fields.is_empty());
    assert!(!form.need_appearances);
    assert!(form.default_appearance.is_none());
    assert!(form.default_resources.is_none());
    assert!(form.xfa.is_none());
}

#[test]
fn malformed_form_entries_are_typed_errors() {
    assert!(matches!(
        form_error(Value::Integer(7)),
        FormError::Malformed(_)
    ));
    let cases: Vec<Value> = vec![
        Value::Dict(Dict::new().insert("Fields", Value::Integer(1))),
        Value::Dict(Dict::new().insert("NeedAppearances", Value::Integer(1))),
        Value::Dict(Dict::new().insert("Fields", arr(vec![Value::Integer(5)]))),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("FT", Value::Integer(1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("T", Value::Integer(1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Ff", Value::Integer(-1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("MaxLen", Value::Integer(-2)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Q", Value::Float(1.5)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Opt", Value::Integer(1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Subtype", name("Widget")),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Kids", Value::Integer(1)),
            )]),
        )),
    ];
    for acro in cases {
        assert!(matches!(form_error(acro), FormError::Malformed(_)));
    }
}

#[test]
fn field_tree_cycles_and_depth_are_typed_errors() {
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Ref(21, 0)])),
    ));
    insert(
        &doc,
        21,
        Value::Dict(
            Dict::new()
                .insert("T", text("loop"))
                .insert("Kids", arr(vec![Value::Ref(21, 0)])),
        ),
    );
    assert!(matches!(
        doc.acro_form().expect_err("cycle"),
        FormError::Malformed(_)
    ));

    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Ref(100, 0)])),
    ));
    for index in 0..70i64 {
        let kids = if index < 69 {
            arr(vec![Value::Ref(101 + index, 0)])
        } else {
            arr(Vec::new())
        };
        insert(
            &doc,
            100 + index,
            Value::Dict(Dict::new().insert("T", text("n")).insert("Kids", kids)),
        );
    }
    assert_eq!(
        doc.acro_form().expect_err("depth"),
        FormError::LimitExceeded("form field depth")
    );
}

#[test]
fn form_node_limit_is_enforced() {
    let fields: Vec<Value> = (0..=MAX_FORM_NODES)
        .map(|_| Value::Dict(Dict::new()))
        .collect();
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert("Fields", arr(fields))));
    assert_eq!(
        doc.acro_form().expect_err("node limit"),
        FormError::LimitExceeded("form field count")
    );
}
