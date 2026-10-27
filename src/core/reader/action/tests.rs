//! E3 读端动作字典测试（12.6）。

use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

use super::*;

const PAGES_OBJECT: i64 = 2;
const PAGE_BASE: i64 = 10;

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn page_ref(index: usize) -> Value {
    Value::Ref(PAGE_BASE + index as i64, 0)
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn document_with_pages(page_count: usize) -> Document {
    let mut doc = Document::new();
    let mut kids = Array::new();
    for index in 0..page_count {
        let num = PAGE_BASE + index as i64;
        kids = kids.push(Value::Ref(num, 0));
        insert(
            &doc,
            num,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Page"))
                    .insert("Parent", Value::Ref(PAGES_OBJECT, 0)),
            ),
        );
    }
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(kids))
                .insert("Count", Value::Integer(page_count as i64)),
        ),
    );
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0)),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn action_dict(subtype: &str) -> Dict {
    Dict::new().insert("S", name(subtype))
}

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
fn parses_remote_launch_and_javascript_actions() {
    let doc = document_with_pages(1);

    let remote_page = Value::Dict(
        action_dict("GoToR")
            .insert("F", Value::String("other.pdf".into()))
            .insert("D", Value::Integer(5)),
    );
    assert_eq!(
        doc.action(&remote_page).unwrap(),
        Action::GoToR {
            file: FileSpec {
                file: "other.pdf".into()
            },
            destination: RemoteDestination::Page(5),
        }
    );

    let remote_named = Value::Dict(
        action_dict("GoToR")
            .insert(
                "F",
                Value::Dict(Dict::new().insert("UF", Value::String("remote.pdf".into()))),
            )
            .insert("D", arr(vec![name("chapter"), name("Fit")])),
    );
    assert_eq!(
        doc.action(&remote_named).unwrap(),
        Action::GoToR {
            file: FileSpec {
                file: "remote.pdf".into()
            },
            destination: RemoteDestination::Named("chapter".into()),
        }
    );

    let launch = Value::Dict(
        action_dict("Launch")
            .insert("F", Value::String("viewer.exe".into()))
            .insert("NewWindow", Value::Bool(true)),
    );
    assert_eq!(
        doc.action(&launch).unwrap(),
        Action::Launch {
            file: FileSpec {
                file: "viewer.exe".into()
            },
            new_window: Some(true),
        }
    );

    let script =
        Value::Dict(action_dict("JavaScript").insert("JS", Value::String("app.alert(1);".into())));
    assert_eq!(
        doc.action(&script).unwrap(),
        Action::JavaScript("app.alert(1);".into())
    );

    let js_stream_ref = 50;
    insert(
        &doc,
        js_stream_ref,
        Value::Stream(Dict::new(), StreamData::Owned(b"this.print();".to_vec())),
    );
    let stream_action =
        Value::Dict(action_dict("JavaScript").insert("JS", Value::Ref(js_stream_ref, 0)));
    assert_eq!(
        doc.action(&stream_action).unwrap(),
        Action::JavaScript("this.print();".into())
    );
}

#[test]
fn unknown_action_subtype_is_preserved() {
    let doc = document_with_pages(1);
    let action = Value::Dict(action_dict("GoToE"));
    assert_eq!(
        doc.action(&action).unwrap(),
        Action::Unknown {
            subtype: "GoToE".into()
        }
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
