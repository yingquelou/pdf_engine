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

#[test]
fn parses_submit_form_action_with_fields_and_flags() {
    let doc = document_with_pages(1);
    let field_object = 41;
    insert(
        &doc,
        field_object,
        Value::Dict(Dict::new().insert("T", Value::String("name".into()))),
    );

    let submit = Value::Dict(
        action_dict("SubmitForm")
            .insert(
                "F",
                Value::Dict(
                    Dict::new()
                        .insert("FS", name("URL"))
                        .insert("F", Value::String("https://example.com/submit".into())),
                ),
            )
            .insert(
                "Fields",
                arr(vec![
                    Value::String("name".into()),
                    Value::Ref(field_object, 0),
                ]),
            )
            .insert("Flags", Value::Integer(0x0003))
            .insert("URLType", name("Browser")),
    );
    assert_eq!(
        doc.action(&submit).unwrap(),
        Action::SubmitForm(SubmitFormAction {
            file: FileSpec {
                file: "https://example.com/submit".into()
            },
            url_type: Some("Browser".into()),
            fields: vec![
                ActionField::Name("name".into()),
                ActionField::Reference {
                    number: field_object,
                    generation: 0,
                },
            ],
            flags: 0x0003,
        })
    );

    let minimal = Value::Dict(
        action_dict("SubmitForm").insert("F", Value::String("https://example.com/s".into())),
    );
    assert_eq!(
        doc.action(&minimal).unwrap(),
        Action::SubmitForm(SubmitFormAction {
            file: FileSpec {
                file: "https://example.com/s".into()
            },
            url_type: None,
            fields: Vec::new(),
            flags: 0,
        })
    );
}

#[test]
fn parses_reset_form_and_import_data_actions() {
    let doc = document_with_pages(1);

    let reset = Value::Dict(
        action_dict("ResetForm")
            .insert("Fields", arr(vec![Value::String("group.total".into())]))
            .insert("Flags", Value::Integer(1)),
    );
    assert_eq!(
        doc.action(&reset).unwrap(),
        Action::ResetForm(ResetFormAction {
            fields: vec![ActionField::Name("group.total".into())],
            flags: 1,
        })
    );

    let reset_all = Value::Dict(action_dict("ResetForm"));
    assert_eq!(
        doc.action(&reset_all).unwrap(),
        Action::ResetForm(ResetFormAction {
            fields: Vec::new(),
            flags: 0,
        })
    );

    let import =
        Value::Dict(action_dict("ImportData").insert("F", Value::String("data.fdf".into())));
    assert_eq!(
        doc.action(&import).unwrap(),
        Action::ImportData {
            file: FileSpec {
                file: "data.fdf".into()
            },
        }
    );
}

#[test]
fn malformed_form_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(
        &doc,
        40,
        Value::Dict(Dict::new().insert("T", Value::String("name".into()))),
    );
    insert(&doc, 41, Value::String("not a field".into()));

    let cases = [
        Value::Dict(action_dict("SubmitForm")),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", Value::Integer(1)),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", arr(vec![Value::Integer(3)])),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", arr(vec![Value::Ref(41, 0)])),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Fields", arr(vec![Value::Ref(99, 0)])),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Flags", Value::Integer(-1)),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("Flags", Value::Float(1.0)),
        ),
        Value::Dict(
            action_dict("SubmitForm")
                .insert("F", Value::String("https://example.com".into()))
                .insert("URLType", Value::String("Browser".into())),
        ),
        Value::Dict(action_dict("ResetForm").insert("Fields", arr(vec![Value::Bool(true)]))),
        Value::Dict(action_dict("ResetForm").insert("Flags", Value::Integer(-2))),
        Value::Dict(action_dict("ImportData")),
        Value::Dict(action_dict("ImportData").insert("F", Value::Integer(1))),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}

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

#[test]
fn parses_hide_action_targets_and_flag() {
    let doc = document_with_pages(1);
    let annot_object = 60;
    insert(
        &doc,
        annot_object,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Annot"))
                .insert("Subtype", name("Text")),
        ),
    );

    let single = Value::Dict(action_dict("Hide").insert("T", Value::String("group.note".into())));
    assert_eq!(
        doc.action(&single).unwrap(),
        Action::Hide(HideAction {
            targets: vec![HideTarget::FieldName("group.note".into())],
            hide: None,
        })
    );

    let array = Value::Dict(
        action_dict("Hide")
            .insert(
                "T",
                arr(vec![
                    Value::Ref(annot_object, 0),
                    Value::String("group.note".into()),
                ]),
            )
            .insert("H", Value::Bool(false)),
    );
    assert_eq!(
        doc.action(&array).unwrap(),
        Action::Hide(HideAction {
            targets: vec![
                HideTarget::Annotation {
                    number: annot_object,
                    generation: 0,
                },
                HideTarget::FieldName("group.note".into()),
            ],
            hide: Some(false),
        })
    );
}

#[test]
fn malformed_hide_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(&doc, 61, name("not-an-annotation"));

    let cases = [
        Value::Dict(action_dict("Hide")),
        Value::Dict(action_dict("Hide").insert("T", arr(Vec::new()))),
        Value::Dict(action_dict("Hide").insert("T", Value::Integer(7))),
        Value::Dict(action_dict("Hide").insert("T", Value::Ref(61, 0))),
        Value::Dict(
            action_dict("Hide")
                .insert("T", Value::String("field".into()))
                .insert("H", Value::Integer(1)),
        ),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}

#[test]
fn parses_set_ocg_state_action() {
    let doc = document_with_pages(1);
    for num in [41, 42, 43] {
        insert(
            &doc,
            num,
            Value::Dict(Dict::new().insert("Type", name("OCG"))),
        );
    }

    let value = Value::Dict(
        action_dict("SetOCGState")
            .insert(
                "State",
                arr(vec![
                    name("OFF"),
                    Value::Ref(41, 0),
                    Value::Ref(42, 0),
                    name("Toggle"),
                    Value::Ref(43, 0),
                    name("Restore"),
                    Value::Ref(41, 0),
                ]),
            )
            .insert("PreserveRB", Value::Bool(false)),
    );
    assert_eq!(
        doc.action(&value).unwrap(),
        Action::SetOCGState(SetOcgStateAction {
            state: vec![
                OcgStateSequence {
                    operation: OcgStateOperation::Off,
                    groups: vec![(41, 0), (42, 0)],
                },
                OcgStateSequence {
                    operation: OcgStateOperation::Toggle,
                    groups: vec![(43, 0)],
                },
                OcgStateSequence {
                    operation: OcgStateOperation::Other("Restore".into()),
                    groups: vec![(41, 0)],
                },
            ],
            preserve_radio_buttons: Some(false),
        })
    );

    let minimal = Value::Dict(
        action_dict("SetOCGState").insert("State", arr(vec![name("ON"), Value::Ref(43, 0)])),
    );
    assert_eq!(
        doc.action(&minimal).unwrap(),
        Action::SetOCGState(SetOcgStateAction {
            state: vec![OcgStateSequence {
                operation: OcgStateOperation::On,
                groups: vec![(43, 0)],
            }],
            preserve_radio_buttons: None,
        })
    );
}

#[test]
fn malformed_set_ocg_state_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert(
        &doc,
        41,
        Value::Dict(Dict::new().insert("Type", name("OCG"))),
    );
    insert(&doc, 61, name("not-an-ocg"));

    let cases = [
        Value::Dict(action_dict("SetOCGState")),
        Value::Dict(action_dict("SetOCGState").insert("State", Value::Integer(1))),
        Value::Dict(action_dict("SetOCGState").insert("State", arr(Vec::new()))),
        Value::Dict(action_dict("SetOCGState").insert("State", arr(vec![Value::Ref(41, 0)]))),
        Value::Dict(action_dict("SetOCGState").insert("State", arr(vec![name("ON")]))),
        Value::Dict(action_dict("SetOCGState").insert(
            "State",
            arr(vec![name("ON"), name("OFF"), Value::Ref(41, 0)]),
        )),
        Value::Dict(
            action_dict("SetOCGState").insert("State", arr(vec![name("ON"), Value::Integer(3)])),
        ),
        Value::Dict(
            action_dict("SetOCGState").insert("State", arr(vec![name("ON"), Value::Ref(61, 0)])),
        ),
        Value::Dict(
            action_dict("SetOCGState")
                .insert("State", arr(vec![name("ON"), Value::Ref(41, 0)]))
                .insert("PreserveRB", Value::Integer(1)),
        ),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}

#[test]
fn parses_transition_action() {
    use crate::core::reader::transition::{
        Transition, TransitionDimension, TransitionDirection, TransitionMotion, TransitionStyle,
    };

    let doc = document_with_pages(1);

    let split = Value::Dict(
        action_dict("Trans").insert(
            "Trans",
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Trans"))
                    .insert("S", name("Split"))
                    .insert("D", Value::Float(2.5))
                    .insert("Dm", name("V"))
                    .insert("M", name("O")),
            ),
        ),
    );
    assert_eq!(
        doc.action(&split).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Split,
            duration: Some(2.5),
            dimension: Some(TransitionDimension::Vertical),
            motion: Some(TransitionMotion::Outward),
            direction: None,
            scale: None,
            border: None,
        })
    );

    let fly = Value::Dict(
        action_dict("Trans").insert(
            "Trans",
            Value::Dict(
                Dict::new()
                    .insert("S", name("Fly"))
                    .insert("D", Value::Integer(3))
                    .insert("Di", name("None"))
                    .insert("SS", Value::Float(0.25))
                    .insert("B", Value::Bool(true)),
            ),
        ),
    );
    assert_eq!(
        doc.action(&fly).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Fly,
            duration: Some(3.0),
            dimension: None,
            motion: None,
            direction: Some(TransitionDirection::None),
            scale: Some(0.25),
            border: Some(true),
        })
    );

    let wipe = Value::Dict(
        action_dict("Trans").insert(
            "Trans",
            Value::Dict(
                Dict::new()
                    .insert("S", name("Wipe"))
                    .insert("Di", Value::Integer(90)),
            ),
        ),
    );
    assert_eq!(
        doc.action(&wipe).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Wipe,
            duration: None,
            dimension: None,
            motion: None,
            direction: Some(TransitionDirection::Degrees(90)),
            scale: None,
            border: None,
        })
    );

    let defaulted = Value::Dict(action_dict("Trans").insert(
        "Trans",
        Value::Dict(Dict::new().insert("D", Value::Integer(0))),
    ));
    assert_eq!(
        doc.action(&defaulted).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::R,
            duration: Some(0.0),
            dimension: None,
            motion: None,
            direction: None,
            scale: None,
            border: None,
        })
    );

    let unknown = Value::Dict(action_dict("Trans").insert(
        "Trans",
        Value::Dict(Dict::new().insert("S", name("Spiral"))),
    ));
    assert_eq!(
        doc.action(&unknown).unwrap(),
        Action::Transition(Transition {
            style: TransitionStyle::Other("Spiral".into()),
            duration: None,
            dimension: None,
            motion: None,
            direction: None,
            scale: None,
            border: None,
        })
    );
}

#[test]
fn malformed_transition_actions_are_typed_errors() {
    let doc = document_with_pages(1);

    let cases = [
        Value::Dict(action_dict("Trans")),
        Value::Dict(action_dict("Trans").insert("Trans", Value::Integer(1))),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("Type", name("Foo"))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("S", Value::Integer(1))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("D", Value::String("2".into()))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("D", Value::Float(-1.0))),
        )),
        Value::Dict(
            action_dict("Trans").insert("Trans", Value::Dict(Dict::new().insert("Dm", name("X")))),
        ),
        Value::Dict(
            action_dict("Trans").insert("Trans", Value::Dict(Dict::new().insert("M", name("X")))),
        ),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("Di", Value::Integer(45))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("Di", name("Horizontal"))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("SS", Value::Bool(true))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("SS", Value::Float(0.0))),
        )),
        Value::Dict(action_dict("Trans").insert(
            "Trans",
            Value::Dict(Dict::new().insert("B", Value::Integer(1))),
        )),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}

mod gotoe;
mod sound;
mod thread;
