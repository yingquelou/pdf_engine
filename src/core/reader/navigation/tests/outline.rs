use super::*;

#[test]
fn outline_reads_titles_destinations_and_children() {
    let doc = document_with_pages(2, Dict::new().insert("Outlines", Value::Ref(60, 0)));
    insert(
        &doc,
        60,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Outlines"))
                .insert("First", Value::Ref(61, 0))
                .insert("Last", Value::Ref(62, 0))
                .insert("Count", Value::Integer(2)),
        ),
    );
    insert(
        &doc,
        61,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::HexString(hex_of("封面 Ω")))
                .insert("Parent", Value::Ref(60, 0))
                .insert("Next", Value::Ref(62, 0))
                .insert("Dest", arr(vec![page_ref(0), name("Fit")]))
                .insert("First", Value::Ref(63, 0))
                .insert("Last", Value::Ref(63, 0))
                .insert("Count", Value::Integer(1))
                .insert(
                    "C",
                    arr(vec![
                        Value::Integer(1),
                        Value::Float(0.5),
                        Value::Integer(0),
                    ]),
                )
                .insert("F", Value::Integer(3)),
        ),
    );
    insert(
        &doc,
        62,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::String("Chapter 1".into()))
                .insert("Parent", Value::Ref(60, 0))
                .insert("Prev", Value::Ref(61, 0))
                .insert("Dest", name("Chapter1")),
        ),
    );
    insert(
        &doc,
        63,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::String("Section".into()))
                .insert("Parent", Value::Ref(61, 0)),
        ),
    );

    let outline = doc.outline().unwrap().expect("outline");
    assert_eq!(outline.items.len(), 2);
    let first = &outline.items[0];
    assert_eq!(first.title, "封面 Ω");
    assert_eq!(
        first.destination,
        Some(OutlineDestination::Explicit(Destination::Fit { page: 0 }))
    );
    assert_eq!(first.color, Some([1.0, 0.5, 0.0]));
    assert_eq!(first.flags, 3);
    assert_eq!(first.count, 1);
    assert_eq!(first.children.len(), 1);
    assert_eq!(first.children[0].title, "Section");
    assert!(first.action.is_none());

    let second = &outline.items[1];
    assert_eq!(second.title, "Chapter 1");
    assert_eq!(
        second.destination,
        Some(OutlineDestination::Named("Chapter1".into()))
    );
    assert_eq!(second.flags, 0);
    assert!(second.children.is_empty());
}

#[test]
fn outline_rejects_cycles_and_missing_titles() {
    let doc = document_with_pages(1, Dict::new().insert("Outlines", Value::Ref(70, 0)));
    insert(
        &doc,
        70,
        Value::Dict(Dict::new().insert("First", Value::Ref(71, 0))),
    );
    insert(
        &doc,
        71,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::String("Loop".into()))
                .insert("Next", Value::Ref(71, 0)),
        ),
    );
    let error = doc.outline().unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");

    let doc = document_with_pages(1, Dict::new().insert("Outlines", Value::Ref(72, 0)));
    insert(
        &doc,
        72,
        Value::Dict(Dict::new().insert("First", Value::Ref(73, 0))),
    );
    insert(&doc, 73, Value::Dict(Dict::new()));
    let error = doc.outline().unwrap_err();
    assert!(error.to_string().contains("missing /Title"), "{error}");
}

#[test]
fn outline_is_absent_without_catalog_entry() {
    let doc = document_with_pages(1, Dict::new());
    assert_eq!(doc.outline().unwrap(), None);
    assert_eq!(doc.page_labels().unwrap(), None);
}
