//! 便携集合读端测试：`/Collection` 模式、初始文档、视图与排序。

use crate::core::types::{Array, Dict, Object, Trailer, Value};

use super::*;

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 只含目录与空页面树的文档；`catalog` 条目并入目录。
fn document(catalog: Dict) -> Document {
    let mut doc = Document::new();
    let catalog = catalog
        .insert("Type", Value::Name("Catalog".into()))
        .insert("Pages", Value::Ref(2, 0));
    insert(
        &doc,
        2,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new()))
                .insert("Count", Value::Integer(0)),
        ),
    );
    insert(&doc, 1, Value::Dict(catalog));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn text(value: &str) -> Value {
    Value::String(value.into())
}

fn field(subtype: &str, name: &str) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name(subtype.into()))
            .insert("N", text(name)),
    )
}

fn collection_with(entries: Dict) -> Document {
    document(Dict::new().insert("Collection", Value::Dict(entries)))
}

#[test]
fn collection_reads_schema_view_and_sort() {
    let schema = Dict::new()
        .insert("Type", Value::Name("CollectionSchema".into()))
        .insert(
            "from",
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("CollectionField".into()))
                    .insert("Subtype", Value::Name("S".into()))
                    .insert("N", text("From"))
                    .insert("O", Value::Integer(1))
                    .insert("V", Value::Bool(true))
                    .insert("E", Value::Bool(false)),
            ),
        )
        .insert("date", field("D", "Date received"))
        .insert("subject", field("S", "Subject"))
        .insert("size", field("Size", "Size"));
    let sort = Dict::new()
        .insert("Type", Value::Name("CollectionSort".into()))
        .insert(
            "S",
            Value::Array(Array(vec![
                Value::Name("date".into()),
                Value::Name("subject".into()),
            ])),
        )
        .insert(
            "A",
            Value::Array(Array(vec![Value::Bool(false), Value::Bool(true)])),
        );
    let doc = collection_with(
        Dict::new()
            .insert("Type", Value::Name("Collection".into()))
            .insert("Schema", Value::Dict(schema))
            .insert("D", Value::String("Doc1".into()))
            .insert("View", Value::Name("T".into()))
            .insert("Sort", Value::Dict(sort)),
    );

    let collection = doc.collection().expect("collection").expect("declared");
    assert_eq!(collection.view, CollectionView::Tiles);
    assert_eq!(
        collection.initial_document.as_deref(),
        Some(b"Doc1".as_slice())
    );

    let schema = collection.schema.expect("schema");
    assert_eq!(schema.len(), 4);
    let from = &schema["from"];
    assert_eq!(from.key, "from");
    assert_eq!(from.field_type, CollectionFieldType::Text);
    assert_eq!(from.name, "From");
    assert_eq!(from.order, Some(1));
    assert_eq!(from.visible, Some(true));
    assert_eq!(from.editable, Some(false));
    assert_eq!(schema["date"].field_type, CollectionFieldType::Date);
    assert_eq!(schema["size"].field_type, CollectionFieldType::Size);
    assert_eq!(schema["subject"].order, None);
    assert_eq!(schema["subject"].visible, None);

    let sort = collection.sort.expect("sort");
    assert_eq!(sort.fields, vec!["date".to_string(), "subject".to_string()]);
    assert_eq!(sort.ascending, vec![false, true]);
}

#[test]
fn collection_defaults_and_absent_entries() {
    assert_eq!(
        document(Dict::new())
            .collection()
            .expect("no catalog entry"),
        None
    );

    let collection = collection_with(Dict::new())
        .collection()
        .expect("collection")
        .expect("declared");
    assert_eq!(collection.schema, None);
    assert_eq!(collection.initial_document, None);
    assert_eq!(collection.view, CollectionView::Details);
    assert_eq!(collection.sort, None);

    let view = collection_with(Dict::new().insert("View", Value::Name("H".into())))
        .collection()
        .expect("collection")
        .expect("declared");
    assert_eq!(view.view, CollectionView::Hidden);
}

#[test]
fn collection_sort_booleans_and_arrays_are_normalized() {
    let schema = || {
        Dict::new()
            .insert("a", field("S", "A"))
            .insert("b", field("S", "B"))
            .insert("date", field("D", "Date"))
    };

    let scalar = collection_with(
        Dict::new().insert("Schema", Value::Dict(schema())).insert(
            "Sort",
            Value::Dict(
                Dict::new()
                    .insert("S", Value::Name("date".into()))
                    .insert("A", Value::Bool(false)),
            ),
        ),
    )
    .collection()
    .expect("collection")
    .expect("declared")
    .sort
    .expect("sort");
    assert_eq!(scalar.fields, vec!["date".to_string()]);
    assert_eq!(scalar.ascending, vec![false]);

    let short = collection_with(
        Dict::new().insert("Schema", Value::Dict(schema())).insert(
            "Sort",
            Value::Dict(
                Dict::new()
                    .insert(
                        "S",
                        Value::Array(Array(vec![
                            Value::Name("a".into()),
                            Value::Name("b".into()),
                        ])),
                    )
                    .insert("A", Value::Array(Array(vec![Value::Bool(false)]))),
            ),
        ),
    )
    .collection()
    .expect("collection")
    .expect("declared")
    .sort
    .expect("sort");
    assert_eq!(short.ascending, vec![false, true]);

    let long = collection_with(Dict::new().insert("Schema", Value::Dict(schema())).insert(
        "Sort",
        Value::Dict(Dict::new().insert("S", Value::Name("a".into())).insert(
            "A",
            Value::Array(Array(vec![Value::Bool(false), Value::Bool(true)])),
        )),
    ))
    .collection()
    .expect("collection")
    .expect("declared")
    .sort
    .expect("sort");
    assert_eq!(long.ascending, vec![false]);

    let defaulted = collection_with(Dict::new().insert("Schema", Value::Dict(schema())).insert(
        "Sort",
        Value::Dict(Dict::new().insert("S", Value::Name("a".into()))),
    ))
    .collection()
    .expect("collection")
    .expect("declared")
    .sort
    .expect("sort");
    assert_eq!(defaulted.ascending, vec![true]);
}

#[test]
fn malformed_collection_entries_are_typed_errors() {
    let cases = [
        Dict::new().insert("Type", Value::Name("CollectionX".into())),
        Dict::new().insert("Schema", Value::Integer(1)),
        Dict::new().insert(
            "Schema",
            Value::Dict(Dict::new().insert("Type", Value::Name("Bad".into()))),
        ),
        Dict::new().insert(
            "Schema",
            Value::Dict(Dict::new().insert("a", Value::Integer(1))),
        ),
        Dict::new().insert(
            "Schema",
            Value::Dict(Dict::new().insert("a", Value::Dict(Dict::new().insert("N", text("A"))))),
        ),
        Dict::new().insert(
            "Schema",
            Value::Dict(Dict::new().insert(
                "a",
                Value::Dict(Dict::new().insert("Subtype", Value::Name("S".into()))),
            )),
        ),
        Dict::new().insert(
            "Schema",
            Value::Dict(
                Dict::new().insert(
                    "a",
                    Value::Dict(
                        Dict::new()
                            .insert("Subtype", Value::Name("S".into()))
                            .insert("N", text("A"))
                            .insert("V", Value::Integer(1)),
                    ),
                ),
            ),
        ),
        Dict::new().insert("D", Value::Integer(1)),
        Dict::new().insert("View", Value::Name("X".into())),
        Dict::new().insert("View", Value::Integer(1)),
        Dict::new().insert("Sort", Value::Dict(Dict::new())),
        Dict::new().insert(
            "Sort",
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Bad".into()))
                    .insert("S", Value::Name("a".into())),
            ),
        ),
        Dict::new().insert(
            "Sort",
            Value::Dict(Dict::new().insert("S", Value::Array(Array(vec![Value::Integer(1)])))),
        ),
        Dict::new().insert(
            "Sort",
            Value::Dict(
                Dict::new()
                    .insert("S", Value::Name("a".into()))
                    .insert("A", Value::Array(Array(vec![Value::Integer(1)]))),
            ),
        ),
    ];
    for case in cases {
        let error = collection_with(case).collection().expect_err("malformed");
        assert!(matches!(error, NavigationError::Malformed(_)), "{error:?}");
    }

    let unknown_sort = collection_with(
        Dict::new()
            .insert(
                "Schema",
                Value::Dict(Dict::new().insert("a", field("S", "A"))),
            )
            .insert(
                "Sort",
                Value::Dict(Dict::new().insert("S", Value::Name("b".into()))),
            ),
    )
    .collection()
    .expect_err("unknown sort field");
    assert!(
        format!("{unknown_sort}").contains("schema"),
        "{unknown_sort}"
    );

    let missing_schema_sort = collection_with(Dict::new().insert(
        "Sort",
        Value::Dict(Dict::new().insert("S", Value::Name("b".into()))),
    ))
    .collection()
    .expect("schema absent leaves sort references unchecked");
    assert!(missing_schema_sort.is_some());
}

#[test]
fn collection_field_subtypes_and_escaped_keys_are_preserved() {
    let schema = Dict::new()
        .insert("S#20text", field("S", "Text"))
        .insert("date", field("D", "Date"))
        .insert("number", field("N", "Number"))
        .insert("file", field("F", "File"))
        .insert("desc", field("Desc", "Description"))
        .insert("mod", field("ModDate", "Modified"))
        .insert("created", field("CreationDate", "Created"))
        .insert("size", field("Size", "Size"))
        .insert("weird", field("Weird", "Weird"));
    let collection = collection_with(Dict::new().insert("Schema", Value::Dict(schema)))
        .collection()
        .expect("collection")
        .expect("declared");
    let schema = collection.schema.expect("schema");

    assert_eq!(
        schema["S text"].field_type,
        CollectionFieldType::Text,
        "schema keys decode #xx escapes"
    );
    assert_eq!(schema["date"].field_type, CollectionFieldType::Date);
    assert_eq!(schema["number"].field_type, CollectionFieldType::Number);
    assert_eq!(schema["file"].field_type, CollectionFieldType::FileName);
    assert_eq!(schema["desc"].field_type, CollectionFieldType::Description);
    assert_eq!(
        schema["mod"].field_type,
        CollectionFieldType::ModificationDate
    );
    assert_eq!(
        schema["created"].field_type,
        CollectionFieldType::CreationDate
    );
    assert_eq!(schema["size"].field_type, CollectionFieldType::Size);
    assert_eq!(
        schema["weird"].field_type,
        CollectionFieldType::Other("Weird".into())
    );
}
