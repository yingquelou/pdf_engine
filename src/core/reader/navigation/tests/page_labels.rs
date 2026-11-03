use super::*;

#[test]
fn page_labels_format_styles_prefixes_and_starts() {
    let doc = document_with_pages(10, Dict::new());
    insert(
        &doc,
        50,
        Value::Dict(Dict::new().insert(
            "Nums",
            arr(vec![
                Value::Integer(0),
                Value::Dict(
                    Dict::new()
                        .insert("S", name("D"))
                        .insert("P", Value::String("p-".into()))
                        .insert("St", Value::Integer(3)),
                ),
                Value::Integer(2),
                Value::Dict(
                    Dict::new()
                        .insert("S", name("R"))
                        .insert("St", Value::Integer(4)),
                ),
                Value::Integer(4),
                Value::Dict(Dict::new().insert("S", name("A"))),
                Value::Integer(6),
                Value::Dict(Dict::new().insert("S", name("r"))),
                Value::Integer(8),
                Value::Dict(Dict::new().insert("P", Value::String("Appendix".into()))),
            ]),
        )),
    );
    let mut catalog = Dict::new().insert("PageLabels", Value::Ref(50, 0));
    // 目录对象在构造后被替换，补入 /PageLabels。
    let doc = doc;
    catalog = catalog
        .insert("Type", name("Catalog"))
        .insert("Pages", Value::Ref(PAGES_OBJECT, 0));
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));

    let labels = doc.page_labels().unwrap().expect("page labels");
    assert_eq!(labels.page_count(), 10);
    assert_eq!(labels.label_for(0).as_deref(), Some("p-3"));
    assert_eq!(labels.label_for(1).as_deref(), Some("p-4"));
    assert_eq!(labels.label_for(2).as_deref(), Some("IV"));
    assert_eq!(labels.label_for(3).as_deref(), Some("V"));
    assert_eq!(labels.label_for(4).as_deref(), Some("A"));
    assert_eq!(labels.label_for(5).as_deref(), Some("B"));
    assert_eq!(labels.label_for(6).as_deref(), Some("i"));
    assert_eq!(labels.label_for(7).as_deref(), Some("ii"));
    assert_eq!(labels.label_for(8).as_deref(), Some("Appendix"));
    assert_eq!(labels.label_for(9).as_deref(), Some("Appendix"));
    assert_eq!(labels.label_for(10), None);
    assert_eq!(doc.page_label(2).unwrap().as_deref(), Some("IV"));
    assert_eq!(
        doc.page_label(10).unwrap_err(),
        NavigationError::PageNotFound
    );
}

#[test]
fn invalid_page_label_entries_are_typed_errors() {
    let doc = document_with_pages(2, Dict::new());
    insert(
        &doc,
        51,
        Value::Dict(Dict::new().insert(
            "Nums",
            arr(vec![
                Value::Integer(0),
                Value::Dict(Dict::new().insert("S", name("X"))),
            ]),
        )),
    );
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0))
                .insert("PageLabels", Value::Ref(51, 0)),
        ),
    ));
    let error = doc.page_labels().unwrap_err();
    assert!(
        error.to_string().contains("unknown page label style"),
        "{error}"
    );

    insert(
        &doc,
        51,
        Value::Dict(Dict::new().insert(
            "Nums",
            arr(vec![
                Value::Integer(0),
                Value::Dict(Dict::new().insert("St", Value::Integer(0))),
            ]),
        )),
    );
    let error = doc.page_labels().unwrap_err();
    assert!(
        error.to_string().contains("/St must be at least 1"),
        "{error}"
    );
}
