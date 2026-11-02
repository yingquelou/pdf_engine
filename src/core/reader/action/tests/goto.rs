//! 本地 GoTo / Named 目标解析与间接引用测试（12.6.4.1/12.6.4.5）。

use crate::core::reader::navigation::Destination;

use super::*;

#[test]
fn parses_goto_uri_and_named_actions() {
    let doc = document_with_pages(3);

    let explicit = Value::Dict(action_dict("GoTo").insert(
        "D",
        arr(vec![page_ref(1), name("FitH"), Value::Integer(700)]),
    ));
    assert_eq!(
        doc.action(&explicit).unwrap(),
        Action::GoTo(OutlineDestination::Explicit(Destination::FitH {
            page: 1,
            top: Some(700.0),
        }))
    );

    let named = Value::Dict(action_dict("GoTo").insert("D", name("chapter")));
    assert_eq!(
        doc.action(&named).unwrap(),
        Action::GoTo(OutlineDestination::Named("chapter".into()))
    );

    let uri = Value::Dict(
        action_dict("URI").insert("URI", Value::String("https://example.com/a".into())),
    );
    assert_eq!(
        doc.action(&uri).unwrap(),
        Action::Uri("https://example.com/a".into())
    );

    let next = Value::Dict(action_dict("Named").insert("N", name("NextPage")));
    assert_eq!(
        doc.action(&next).unwrap(),
        Action::Named(NamedAction::NextPage)
    );

    let print = Value::Dict(action_dict("Named").insert("N", name("Print")));
    assert_eq!(
        doc.action(&print).unwrap(),
        Action::Named(NamedAction::Other("Print".into()))
    );
}
#[test]
fn action_values_resolve_indirect_references() {
    let doc = document_with_pages(2);
    insert(
        &doc,
        40,
        Value::Dict(action_dict("GoTo").insert("D", Value::Ref(41, 0))),
    );
    insert(
        &doc,
        41,
        arr(vec![
            page_ref(1),
            name("XYZ"),
            Value::Integer(10),
            Value::Null,
            Value::Integer(2),
        ]),
    );

    assert_eq!(
        doc.action(&Value::Ref(40, 0)).unwrap(),
        Action::GoTo(OutlineDestination::Explicit(Destination::Xyz {
            page: 1,
            left: Some(10.0),
            top: None,
            zoom: Some(2.0),
        }))
    );
}
