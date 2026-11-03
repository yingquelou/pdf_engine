use super::*;

#[test]
fn explicit_destinations_parse_all_types() {
    let doc = document_with_pages(3, Dict::new());
    let index = page_tree_index(&doc).unwrap();
    let parse = |value: Value| destination::parse_destination(&doc, &value, &index);

    assert_eq!(
        parse(arr(vec![
            page_ref(0),
            name("XYZ"),
            Value::Null,
            Value::Integer(700),
            Value::Float(1.5),
        ]))
        .unwrap(),
        Destination::Xyz {
            page: 0,
            left: None,
            top: Some(700.0),
            zoom: Some(1.5),
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(1), name("Fit")])).unwrap(),
        Destination::Fit { page: 1 }
    );
    assert_eq!(
        parse(arr(vec![page_ref(2), name("FitH"), Value::Integer(720)])).unwrap(),
        Destination::FitH {
            page: 2,
            top: Some(720.0),
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(0), name("FitV"), Value::Null])).unwrap(),
        Destination::FitV {
            page: 0,
            left: None,
        }
    );
    assert_eq!(
        parse(arr(vec![
            page_ref(1),
            name("FitR"),
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(200),
            Value::Float(300.5),
        ]))
        .unwrap(),
        Destination::FitR {
            page: 1,
            rect: [0.0, 0.0, 200.0, 300.5],
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(2), name("FitB")])).unwrap(),
        Destination::FitB { page: 2 }
    );
    assert_eq!(
        parse(arr(vec![page_ref(0), name("FitBH"), Value::Integer(10)])).unwrap(),
        Destination::FitBH {
            page: 0,
            top: Some(10.0),
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(1), name("FitBV"), Value::Null])).unwrap(),
        Destination::FitBV {
            page: 1,
            left: None,
        }
    );
}

#[test]
fn malformed_destinations_are_typed_errors() {
    let doc = document_with_pages(2, Dict::new());
    let index = page_tree_index(&doc).unwrap();
    let parse = |value: Value| destination::parse_destination(&doc, &value, &index);

    let error = parse(arr(vec![page_ref(0), name("Nope")])).unwrap_err();
    assert!(
        error.to_string().contains("unknown destination type"),
        "{error}"
    );

    let error = parse(arr(vec![Value::Integer(10), name("Fit")])).unwrap_err();
    assert!(
        error.to_string().contains("indirect page reference"),
        "{error}"
    );

    let error = parse(arr(vec![Value::Ref(99, 0), name("Fit")])).unwrap_err();
    assert_eq!(error, NavigationError::PageNotFound);

    let error = parse(arr(vec![
        page_ref(0),
        name("FitR"),
        Value::Integer(0),
        Value::Integer(0),
        Value::Integer(1),
    ]))
    .unwrap_err();
    assert!(
        error.to_string().contains("operand 5 is missing"),
        "{error}"
    );

    let error = parse(arr(vec![page_ref(0), name("Fit"), Value::Integer(1)])).unwrap_err();
    assert!(error.to_string().contains("unexpected operand"), "{error}");
}

#[test]
fn named_destinations_resolve_from_name_tree_and_legacy_dict() {
    let mut catalog = names_catalog("Dests", 40);
    catalog = catalog.insert(
        "Dests",
        Value::Dict(
            Dict::new()
                .insert("Legacy", arr(vec![page_ref(0), name("Fit")]))
                .insert("Old#20Name", arr(vec![page_ref(1), name("Fit")])),
        ),
    );
    let doc = document_with_pages(3, catalog);
    insert(
        &doc,
        40,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Chapter1".into()),
                arr(vec![page_ref(1), name("Fit")]),
                Value::String("Chapter2".into()),
                Value::Dict(Dict::new().insert(
                    "D",
                    arr(vec![
                        page_ref(0),
                        name("XYZ"),
                        Value::Integer(0),
                        Value::Integer(800),
                        Value::Null,
                    ]),
                )),
                Value::HexString(hex_of("Hex")),
                arr(vec![page_ref(2), name("Fit")]),
            ]),
        )),
    );

    assert_eq!(
        doc.named_destination("Chapter1").unwrap(),
        Some(Destination::Fit { page: 1 })
    );
    assert_eq!(
        doc.named_destination("Chapter2").unwrap(),
        Some(Destination::Xyz {
            page: 0,
            left: Some(0.0),
            top: Some(800.0),
            zoom: None,
        })
    );
    assert_eq!(
        doc.named_destination("Hex").unwrap(),
        Some(Destination::Fit { page: 2 })
    );
    assert_eq!(
        doc.named_destination("Legacy").unwrap(),
        Some(Destination::Fit { page: 0 })
    );
    assert_eq!(
        doc.named_destination("Old Name").unwrap(),
        Some(Destination::Fit { page: 1 })
    );
    assert_eq!(doc.named_destination("Missing").unwrap(), None);
}

#[test]
fn malformed_named_destination_is_a_typed_error() {
    let doc = document_with_pages(1, names_catalog("Dests", 41));
    insert(
        &doc,
        41,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Broken".into()),
                Value::Dict(Dict::new()),
            ]),
        )),
    );
    let error = doc.named_destination("Broken").unwrap_err();
    assert!(error.to_string().contains("missing /D"), "{error}");
}

#[test]
fn named_pages_resolve_page_references() {
    let doc = document_with_pages(3, names_catalog("Pages", 45));
    insert(
        &doc,
        45,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Cover".into()),
                Value::Ref(12, 0),
                Value::String("Broken".into()),
                Value::Dict(Dict::new().insert("Type", name("Page"))),
            ]),
        )),
    );
    assert_eq!(doc.named_page("Cover").unwrap(), Some(2));
    assert_eq!(doc.named_page("Missing").unwrap(), None);
    let error = doc.named_page("Broken").unwrap_err();
    assert!(
        error.to_string().contains("indirect page reference"),
        "{error}"
    );
}
