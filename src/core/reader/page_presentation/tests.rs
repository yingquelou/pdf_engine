use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

use super::PagePresentationError;

fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn document_with_page(entries: Vec<(&str, Value)>, extra: Vec<(i64, Value)>) -> Document {
    let mut document = Document::new();
    document.set_object(Object::new(
        1,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Catalog".into())),
            ("Pages", Value::Ref(2, 0)),
        ])),
    ));
    document.set_object(Object::new(
        2,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Pages".into())),
            ("Kids", Value::Array(Array(vec![Value::Ref(3, 0)]))),
            ("Count", Value::Integer(1)),
        ])),
    ));
    let mut page = dict(vec![
        ("Type", Value::Name("Page".into())),
        ("Parent", Value::Ref(2, 0)),
    ]);
    for (key, value) in entries {
        page = page.insert(key, value);
    }
    document.set_object(Object::new(3, 0, Value::Dict(page)));
    for (num, value) in extra {
        document.set_object(Object::new(num, 0, value));
    }
    document.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(10)),
    };
    document
}

fn split_transition() -> Value {
    Value::Dict(dict(vec![
        ("Type", Value::Name("Trans".into())),
        ("S", Value::Name("Split".into())),
        ("D", Value::Float(3.5)),
        ("Dm", Value::Name("V".into())),
        ("M", Value::Name("O".into())),
    ]))
}

#[test]
fn page_presentation_reads_duration_and_transition() {
    let document = document_with_page(
        vec![("Dur", Value::Integer(5)), ("Trans", split_transition())],
        Vec::new(),
    );
    let duration = document.page_duration(0).expect("duration");
    assert_eq!(duration, Some(5.0));
    let transition = document
        .page_transition(0)
        .expect("transition")
        .expect("present");
    assert_eq!(transition.duration, Some(3.5));
    assert!(
        matches!(
            transition.style,
            crate::core::reader::transition::TransitionStyle::Split
        ),
        "style {:?}",
        transition.style
    );
    assert!(
        matches!(
            transition.dimension,
            Some(crate::core::reader::transition::TransitionDimension::Vertical)
        ),
        "dimension {:?}",
        transition.dimension
    );
    assert!(
        matches!(
            transition.motion,
            Some(crate::core::reader::transition::TransitionMotion::Outward)
        ),
        "motion {:?}",
        transition.motion
    );
    assert!(transition.direction.is_none());
    assert!(transition.scale.is_none());
    assert!(transition.border.is_none());
}

#[test]
fn missing_page_presentation_entries_read_as_none() {
    let document = document_with_page(Vec::new(), Vec::new());
    assert_eq!(document.page_duration(0).expect("duration"), None);
    assert!(document.page_transition(0).expect("transition").is_none());
}

#[test]
fn indirect_transition_dictionary_is_resolved_and_defaults_apply() {
    let document = document_with_page(
        vec![("Trans", Value::Ref(4, 0))],
        vec![(4, Value::Dict(dict(vec![("S", Value::Name("R".into()))])))],
    );
    let transition = document
        .page_transition(0)
        .expect("transition")
        .expect("present");
    assert!(
        matches!(
            transition.style,
            crate::core::reader::transition::TransitionStyle::R
        ),
        "style {:?}",
        transition.style
    );
    assert!(transition.duration.is_none());
    assert!(transition.dimension.is_none());
    assert!(transition.motion.is_none());
    assert!(transition.direction.is_none());
}

#[test]
fn malformed_page_presentation_entries_are_typed_errors() {
    let document = document_with_page(vec![("Dur", Value::String("5".into()))], Vec::new());
    match document.page_duration(0) {
        Err(PagePresentationError::Invalid(message)) => {
            assert!(message.contains("/Dur is not a number"), "{message}");
        }
        other => panic!("expected invalid /Dur, got {other:?}"),
    }

    let document = document_with_page(vec![("Dur", Value::Integer(-1))], Vec::new());
    match document.page_duration(0) {
        Err(PagePresentationError::Invalid(message)) => {
            assert!(message.contains("/Dur is negative"), "{message}");
        }
        other => panic!("expected negative /Dur, got {other:?}"),
    }

    let document = document_with_page(vec![("Dur", Value::Float(f64::NAN))], Vec::new());
    match document.page_duration(0) {
        Err(PagePresentationError::Invalid(message)) => {
            assert!(message.contains("not a finite number"), "{message}");
        }
        other => panic!("expected non-finite /Dur, got {other:?}"),
    }

    let document = document_with_page(vec![("Trans", Value::Integer(1))], Vec::new());
    match document.page_transition(0) {
        Err(PagePresentationError::Transition(error)) => {
            assert!(error.to_string().contains("not a dictionary"), "{error}");
        }
        other => panic!("expected malformed /Trans, got {other:?}"),
    }

    let document = document_with_page(
        vec![(
            "Trans",
            Value::Dict(dict(vec![("Type", Value::Name("Widget".into()))])),
        )],
        Vec::new(),
    );
    match document.page_transition(0) {
        Err(PagePresentationError::Transition(error)) => {
            assert!(error.to_string().contains("/Type is not Trans"), "{error}");
        }
        other => panic!("expected malformed /Trans, got {other:?}"),
    }

    let document = document_with_page(vec![("Trans", Value::Ref(9, 0))], Vec::new());
    match document.page_transition(0) {
        Err(PagePresentationError::Invalid(message)) => {
            assert!(message.contains("not found"), "{message}");
        }
        other => panic!("expected unresolved /Trans, got {other:?}"),
    }

    let document = document_with_page(Vec::new(), Vec::new());
    assert!(matches!(
        document.page_duration(5),
        Err(PagePresentationError::Page(_))
    ));
    assert!(matches!(
        document.page_transition(5),
        Err(PagePresentationError::Page(_))
    ));
}
