//! 大纲条目 `/A` 类型化读端测试（12.3.3 / 12.6）。

use crate::core::types::{Dict, Document, Value};

use super::*;

fn outline_item(action: Option<Value>) -> OutlineItem {
    OutlineItem {
        title: "Chapter".into(),
        destination: None,
        action,
        color: None,
        flags: 0,
        count: 0,
        children: Vec::new(),
    }
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

#[test]
fn outline_action_parses_typed_actions() {
    let doc = Document::new();

    let uri = Value::Dict(
        Dict::new()
            .insert("S", name("URI"))
            .insert("URI", Value::String("https://example.com".into())),
    );
    assert_eq!(
        doc.outline_action(&outline_item(Some(uri))).unwrap(),
        Some(Action::Uri("https://example.com".into()))
    );

    let named = Value::Dict(
        Dict::new()
            .insert("S", name("Named"))
            .insert("N", name("NextPage")),
    );
    assert_eq!(
        doc.outline_action(&outline_item(Some(named))).unwrap(),
        Some(Action::Named(NamedAction::NextPage))
    );
}

#[test]
fn outline_without_action_returns_none() {
    let doc = Document::new();

    assert_eq!(doc.outline_action(&outline_item(None)).unwrap(), None);

    // 只有 `/Dest` 的条目同样不物化动作。
    let mut destination_only = outline_item(None);
    destination_only.destination = Some(OutlineDestination::Named("chapter".into()));
    assert_eq!(doc.outline_action(&destination_only).unwrap(), None);
}

#[test]
fn malformed_outline_actions_are_typed_errors() {
    let doc = Document::new();

    let not_a_dict = outline_item(Some(Value::Integer(7)));
    assert!(matches!(
        doc.outline_action(&not_a_dict),
        Err(ActionError::Malformed(_))
    ));

    let missing_s = Value::Dict(Dict::new().insert("URI", Value::String("unused".into())));
    assert!(matches!(
        doc.outline_action(&outline_item(Some(missing_s))),
        Err(ActionError::Malformed(_))
    ));

    let unresolved = outline_item(Some(Value::Ref(99, 0)));
    assert!(matches!(
        doc.outline_action(&unresolved),
        Err(ActionError::Malformed(_))
    ));
}
