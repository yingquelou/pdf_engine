//! 未知子类型保留与非法动作字典的 typed error 测试（12.6.2）。

use crate::core::types::{Dict, Value};

use super::*;

#[test]
fn unknown_action_subtype_is_preserved() {
    let doc = document_with_pages(1);
    let action = Value::Dict(action_dict("Hologram"));
    assert_eq!(
        doc.action(&action).unwrap(),
        Action::Unknown {
            subtype: "Hologram".into()
        }
    );
}
#[test]
fn malformed_actions_are_typed_errors() {
    let doc = document_with_pages(1);

    let cases = [
        Value::Dict(Dict::new()),
        Value::Dict(Dict::new().insert("S", Value::Integer(1))),
        Value::Dict(action_dict("GoTo")),
        Value::Dict(action_dict("GoToR").insert("F", Value::String("a.pdf".into()))),
        Value::Dict(
            action_dict("Launch")
                .insert("F", Value::String("a.exe".into()))
                .insert("NewWindow", Value::Integer(1)),
        ),
        Value::Dict(action_dict("URI").insert("URI", Value::Integer(1))),
        Value::Dict(action_dict("JavaScript")),
        Value::Dict(
            action_dict("GoToR")
                .insert("F", Value::Integer(1))
                .insert("D", Value::Integer(0)),
        ),
        Value::Dict(
            action_dict("GoToR")
                .insert("F", Value::String("a.pdf".into()))
                .insert("D", Value::Bool(true)),
        ),
        Value::Integer(3),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }

    let missing_page =
        Value::Dict(action_dict("GoTo").insert("D", arr(vec![Value::Ref(99, 0), name("Fit")])));
    assert_eq!(
        doc.action(&missing_page),
        Err(ActionError::PageNotFound),
        "未知页面应返回 PageNotFound"
    );
}
