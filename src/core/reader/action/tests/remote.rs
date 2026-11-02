//! 跨文档 GoToR、Launch 与 JavaScript 动作测试（12.6.4.3/12.6.4.2/12.6.4.16）。

use crate::core::types::{Dict, StreamData, Value};

use super::*;

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

    insert(&doc, 20, name("chapter"));
    let remote_indirect = Value::Dict(
        action_dict("GoToR")
            .insert("F", Value::String("other.pdf".into()))
            .insert("D", Value::Ref(20, 0)),
    );
    assert_eq!(
        doc.action(&remote_indirect).unwrap(),
        Action::GoToR {
            file: FileSpec {
                file: "other.pdf".into()
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
