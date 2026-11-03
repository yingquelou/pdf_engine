//! 字段动作测试：`/AA` 脚本动作与 `/CO` 计算顺序（Table 225/221）。

use super::*;

#[test]
fn field_aa_actions_are_typed_and_other_events_preserved() {
    let field = Dict::new()
        .insert("T", text("amount"))
        .insert("FT", name("Tx"))
        .insert(
            "AA",
            Value::Dict(
                Dict::new()
                    .insert("K", javascript("keystroke()"))
                    .insert("F", javascript("format()"))
                    .insert("V", javascript("validate()"))
                    .insert("C", javascript("calculate()"))
                    .insert("X", javascript("custom()")),
            ),
        );
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(field)])),
    ));
    let form = doc.acro_form().expect("parse").expect("present");
    let actions = form.fields[0].actions.as_ref().expect("/AA");
    assert_eq!(
        actions.keystroke,
        Some(Action::JavaScript("keystroke()".into()))
    );
    assert_eq!(actions.format, Some(Action::JavaScript("format()".into())));
    assert_eq!(
        actions.validate,
        Some(Action::JavaScript("validate()".into()))
    );
    assert_eq!(
        actions.calculate,
        Some(Action::JavaScript("calculate()".into()))
    );
    assert_eq!(actions.other.len(), 1);
    assert_eq!(actions.other[0].0, "X");
    assert!(!actions.is_empty());
}

#[test]
fn malformed_field_actions_are_typed_errors() {
    let cases = [
        Value::Integer(1),
        Value::Dict(Dict::new().insert("K", Value::Dict(Dict::new()))),
    ];
    for aa in cases {
        let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(Dict::new().insert("AA", aa))]),
        )));
        assert!(
            matches!(
                doc.acro_form().expect_err("malformed /AA"),
                FormError::Malformed(_)
            ),
            "malformed /AA must be rejected"
        );
    }
}

#[test]
fn calculation_order_resolves_field_references_to_qualified_names() {
    let doc = doc_with_acro_form(Value::Ref(20, 0));
    insert(
        &doc,
        20,
        Value::Dict(
            Dict::new()
                .insert("Fields", arr(vec![Value::Ref(21, 0)]))
                .insert("CO", arr(vec![Value::Ref(23, 0), Value::Ref(21, 0)])),
        ),
    );
    insert(
        &doc,
        21,
        Value::Dict(
            Dict::new()
                .insert("T", text("group"))
                .insert("Kids", arr(vec![Value::Ref(23, 0)])),
        ),
    );
    insert(
        &doc,
        23,
        Value::Dict(Dict::new().insert("T", text("total"))),
    );

    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(form.calculation_order, ["group.total", "group"]);
}

#[test]
fn malformed_calculation_order_is_typed_error() {
    let cases = [
        Value::Integer(1),
        arr(vec![Value::Dict(Dict::new())]),
        arr(vec![Value::Ref(99, 0)]),
    ];
    for co in cases {
        let doc = doc_with_acro_form(Value::Dict(
            Dict::new()
                .insert("Fields", arr(Vec::new()))
                .insert("CO", co),
        ));
        assert!(
            matches!(
                doc.acro_form().expect_err("malformed /CO"),
                FormError::Malformed(_)
            ),
            "malformed /CO must be rejected"
        );
    }
}
