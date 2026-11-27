//! Thread 动作读端测试（12.6.4.6 Table 205）。

use super::*;

fn thread_dict() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("Thread"))
            .insert("F", Value::Ref(30, 0)),
    )
}

fn bead_dict() -> Value {
    Value::Dict(Dict::new().insert("Type", name("Bead")))
}

#[test]
fn parses_thread_action_target_forms() {
    let doc = document_with_pages(1);
    insert(&doc, 20, thread_dict());
    insert(&doc, 21, Value::Integer(7));
    insert(&doc, 30, bead_dict());

    let reference = Value::Dict(
        action_dict("Thread")
            .insert("D", Value::Ref(20, 0))
            .insert("B", Value::Ref(30, 0)),
    );
    assert_eq!(
        doc.action(&reference).unwrap(),
        Action::Thread(ThreadAction {
            file: None,
            thread: ThreadTarget::Reference {
                number: 20,
                generation: 0,
            },
            bead: Some(BeadTarget::Reference {
                number: 30,
                generation: 0,
            }),
        })
    );

    let indices = Value::Dict(
        action_dict("Thread")
            .insert("D", Value::Integer(2))
            .insert("B", Value::Integer(1)),
    );
    assert_eq!(
        doc.action(&indices).unwrap(),
        Action::Thread(ThreadAction {
            file: None,
            thread: ThreadTarget::Index(2),
            bead: Some(BeadTarget::Index(1)),
        })
    );

    let titled = Value::Dict(
        action_dict("Thread")
            .insert("F", Value::String("other.pdf".into()))
            .insert("D", Value::String("chapter one".into())),
    );
    assert_eq!(
        doc.action(&titled).unwrap(),
        Action::Thread(ThreadAction {
            file: Some(FileSpec {
                file: "other.pdf".into(),
            }),
            thread: ThreadTarget::Title("chapter one".into()),
            bead: None,
        })
    );
}

#[test]
fn thread_action_d_index_is_bounded_by_catalog_threads() {
    let doc = document_with_pages(1);
    insert(
        &doc,
        1,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(2, 0))
                .insert("Threads", arr(vec![Value::Ref(20, 0), Value::Ref(21, 0)])),
        ),
    );
    insert(&doc, 20, thread_dict());
    insert(&doc, 21, thread_dict());

    for index in [0, 1] {
        let action = Value::Dict(action_dict("Thread").insert("D", Value::Integer(index)));
        assert_eq!(
            doc.action(&action).unwrap(),
            Action::Thread(ThreadAction {
                file: None,
                thread: ThreadTarget::Index(index),
                bead: None,
            })
        );
    }
    for index in [2, 9] {
        let action = Value::Dict(action_dict("Thread").insert("D", Value::Integer(index)));
        let error = doc.action(&action).unwrap_err();
        assert!(
            error.to_string().contains("out of range for /Threads"),
            "{error}"
        );
    }
}

#[test]
fn thread_action_d_index_without_catalog_threads_stays_lenient() {
    let doc = document_with_pages(1);
    let action = Value::Dict(action_dict("Thread").insert("D", Value::Integer(5)));
    assert_eq!(
        doc.action(&action).unwrap(),
        Action::Thread(ThreadAction {
            file: None,
            thread: ThreadTarget::Index(5),
            bead: None,
        })
    );
}

#[test]
fn malformed_thread_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(&doc, 20, thread_dict());
    insert(&doc, 21, Value::Integer(7));
    insert(&doc, 30, bead_dict());

    let cases = [
        Value::Dict(action_dict("Thread")),
        Value::Dict(action_dict("Thread").insert("D", Value::Integer(-1))),
        Value::Dict(action_dict("Thread").insert("D", name("Chapter"))),
        Value::Dict(action_dict("Thread").insert("D", Value::Ref(21, 0))),
        Value::Dict(action_dict("Thread").insert("D", Value::Ref(99, 0))),
        Value::Dict(action_dict("Thread").insert("D", arr(vec![Value::Integer(0)]))),
        Value::Dict(
            action_dict("Thread")
                .insert("D", Value::Integer(0))
                .insert("B", Value::Integer(-2)),
        ),
        Value::Dict(
            action_dict("Thread")
                .insert("D", Value::Integer(0))
                .insert("B", name("FirstBead")),
        ),
        Value::Dict(
            action_dict("Thread")
                .insert("D", Value::Integer(0))
                .insert("B", Value::Ref(21, 0)),
        ),
        Value::Dict(
            action_dict("Thread")
                .insert("D", Value::Integer(0))
                .insert("B", Value::Ref(99, 0)),
        ),
        Value::Dict(
            action_dict("Thread")
                .insert("F", Value::Bool(true))
                .insert("D", Value::Integer(0)),
        ),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}
