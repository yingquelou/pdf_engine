use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

use super::{MAX_VIEWPORTS, ViewportError};

fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn number_format(unit: &str, conversion: i64) -> Value {
    Value::Dict(dict(vec![
        ("U", Value::String(unit.into())),
        ("C", Value::Integer(conversion)),
    ]))
}

fn measure_value() -> Value {
    let formats = || Value::Array(Array(vec![number_format("in", 1)]));
    Value::Dict(dict(vec![
        ("Type", Value::Name("Measure".into())),
        ("Subtype", Value::Name("RL".into())),
        ("R", Value::String("1 in = 1 ft".into())),
        ("X", formats()),
        ("D", formats()),
        ("A", Value::Array(Array(vec![number_format("sq in", 1)]))),
    ]))
}

fn bbox(x0: i64, y0: i64, x1: i64, y1: i64) -> Value {
    Value::Array(Array(vec![
        Value::Integer(x0),
        Value::Integer(y0),
        Value::Integer(x1),
        Value::Integer(y1),
    ]))
}

fn viewport_dict() -> Dict {
    dict(vec![
        ("Type", Value::Name("Viewport".into())),
        ("BBox", bbox(10, 20, 300, 400)),
        ("Name", Value::String("Plan".into())),
        ("Measure", measure_value()),
        ("Foos", Value::Integer(7)),
    ])
}

fn document_with_vp(page_vp: Option<Value>, extra: Vec<(i64, Value)>) -> Document {
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
    if let Some(vp) = page_vp {
        page = page.insert("VP", vp);
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

fn assert_invalid(value: Value, needle: &str) {
    let document = Document::new();
    match document.viewport(&value) {
        Err(ViewportError::Invalid(message)) => assert!(
            message.contains(needle),
            "error {message:?} should mention {needle}"
        ),
        other => panic!("expected an invalid viewport, got {other:?}"),
    }
}

#[test]
fn page_viewports_parse_bbox_name_and_measure() {
    let document = document_with_vp(
        Some(Value::Array(Array(vec![
            Value::Dict(viewport_dict()),
            Value::Ref(4, 0),
        ]))),
        vec![(4, Value::Dict(dict(vec![("BBox", bbox(0, 0, 50, 60))])))],
    );
    let viewports = document.page_viewports(0).expect("viewports");
    assert_eq!(viewports.len(), 2);

    let plan = &viewports[0];
    assert_eq!(plan.bbox, [10.0, 20.0, 300.0, 400.0]);
    assert_eq!(plan.name.as_deref(), Some("Plan"));
    let measure = plan.measure.as_ref().expect("measure");
    assert_eq!(measure.subtype.as_deref(), Some("RL"));
    assert_eq!(measure.ratio.as_deref(), Some("1 in = 1 ft"));
    let x = measure.x.as_ref().expect("/X");
    assert_eq!(x.len(), 1);
    assert_eq!(x[0].unit.as_deref(), Some("in"));
    assert_eq!(x[0].conversion, Some(1.0));
    assert_eq!(
        measure
            .area
            .as_ref()
            .map(|formats| formats[0].unit.as_deref()),
        Some(Some("sq in"))
    );
    assert_eq!(plan.other.get("Foos"), Some(&Value::Integer(7)));

    let minimal = &viewports[1];
    assert_eq!(minimal.bbox, [0.0, 0.0, 50.0, 60.0]);
    assert!(minimal.name.is_none());
    assert!(minimal.measure.is_none());
    assert!(minimal.other.is_empty());
}

#[test]
fn viewports_without_vp_or_with_plain_bbox_default_to_empty_entries() {
    let document = document_with_vp(None, Vec::new());
    assert!(document.page_viewports(0).expect("page").is_empty());

    let document = document_with_vp(Some(Value::Array(Array(Vec::new()))), Vec::new());
    assert!(document.page_viewports(0).expect("page").is_empty());

    let document = document_with_vp(
        Some(Value::Array(Array(vec![Value::Dict(dict(vec![(
            "BBox",
            bbox(0, 0, 1, 1),
        )]))]))),
        Vec::new(),
    );
    let viewports = document.page_viewports(0).expect("page");
    assert_eq!(viewports[0].bbox, [0.0, 0.0, 1.0, 1.0]);
    assert!(viewports[0].name.is_none());
    assert!(viewports[0].measure.is_none());
}

#[test]
fn viewport_page_errors_are_typed() {
    let document = document_with_vp(None, Vec::new());
    assert!(matches!(
        document.page_viewports(5),
        Err(ViewportError::PageContent(_))
    ));

    let document = document_with_vp(Some(Value::Integer(1)), Vec::new());
    match document.page_viewports(0) {
        Err(ViewportError::Invalid(message)) => {
            assert!(message.contains("/VP is not an array"), "{message}");
        }
        other => panic!("expected invalid /VP, got {other:?}"),
    }

    let too_many: Vec<Value> = (0..=MAX_VIEWPORTS)
        .map(|_| Value::Dict(dict(vec![("BBox", bbox(0, 0, 1, 1))])))
        .collect();
    let document = document_with_vp(Some(Value::Array(Array(too_many))), Vec::new());
    match document.page_viewports(0) {
        Err(ViewportError::Invalid(message)) => {
            assert!(message.contains("more than"), "{message}");
        }
        other => panic!("expected viewport limit, got {other:?}"),
    }
}

#[test]
fn malformed_viewports_are_typed_errors() {
    assert_invalid(Value::Integer(1), "must be a dictionary");
    assert_invalid(
        Value::Dict(dict(vec![("Type", Value::Name("Widget".into()))])),
        "/Type must be /Viewport",
    );
    assert_invalid(
        Value::Dict(dict(vec![("Type", Value::Integer(1))])),
        "/Type must be a name",
    );
    assert_invalid(
        Value::Dict(dict(vec![("Name", Value::String("x".into()))])),
        "/BBox is required",
    );
    assert_invalid(
        Value::Dict(dict(vec![("BBox", Value::Integer(1))])),
        "/BBox is not an array",
    );
    // 四个条目照常，但类型错或顺序未规范化。
    assert_invalid(
        Value::Dict(dict(vec![(
            "BBox",
            Value::Array(Array(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(1),
                Value::Name("top".into()),
            ])),
        )])),
        "four numbers",
    );
    assert_invalid(
        Value::Dict(dict(vec![
            ("BBox", bbox(10, 20, 300, 400)),
            ("Name", Value::Integer(1)),
        ])),
        "/Name must be a text string",
    );
    let bad_measure = Value::Dict(dict(vec![
        ("BBox", bbox(10, 20, 300, 400)),
        ("Measure", Value::Integer(1)),
    ]));
    let document = Document::new();
    match document.viewport(&bad_measure) {
        Err(ViewportError::Measure(error)) => {
            let message = error.to_string();
            assert!(message.contains("measure dictionary"), "{message}");
        }
        other => panic!("expected measure error, got {other:?}"),
    }
    let reversed = Value::Dict(dict(vec![("BBox", bbox(300, 400, 10, 20))]));
    let document = Document::new();
    match document.viewport(&reversed) {
        Err(ViewportError::Invalid(message)) => {
            assert!(message.contains("normalized"), "{message}");
        }
        other => panic!("expected normalized error, got {other:?}"),
    }
}
