use super::*;

#[test]
fn name_tree_reads_flat_and_kid_nodes() {
    let doc = document_with_pages(1, names_catalog("Dests", 30));
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(31, 0), Value::Ref(32, 0)]))),
    );
    insert(
        &doc,
        31,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Alpha".into()),
                Value::Integer(1),
                Value::String("Delta".into()),
                Value::Integer(4),
            ]),
        )),
    );
    insert(
        &doc,
        32,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Beta".into()),
                Value::Integer(2),
                Value::String("Gamma".into()),
                Value::Integer(3),
            ]),
        )),
    );

    let tree = doc.name_tree("Dests").unwrap();
    let keys: Vec<&str> = tree.keys().map(String::as_str).collect();
    assert_eq!(keys, ["Alpha", "Beta", "Delta", "Gamma"]);
    assert_eq!(tree["Gamma"].as_integer(), Some(3));
    assert!(doc.name_tree("EmbeddedFiles").unwrap().is_empty());
}

#[test]
fn name_tree_rejects_cycles_and_depth_overflow() {
    let doc = document_with_pages(1, names_catalog("Dests", 35));
    insert(
        &doc,
        35,
        Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(35, 0)]))),
    );
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(error.to_string().contains("cycle"), "{error}");

    let deep = document_with_pages(1, names_catalog("Dests", 100));
    for num in 100..170 {
        insert(
            &deep,
            num,
            Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(num + 1, 0)]))),
        );
    }
    insert(
        &deep,
        170,
        Value::Dict(Dict::new().insert("Names", arr(vec![]))),
    );
    let error = deep.name_tree("Dests").unwrap_err();
    assert!(
        matches!(error, NavigationError::LimitExceeded("name tree depth")),
        "{error}"
    );
}

#[test]
fn name_tree_rejects_malformed_nodes() {
    let doc = document_with_pages(1, names_catalog("Dests", 36));
    insert(&doc, 36, Value::Dict(Dict::new()));
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(
        error.to_string().contains("neither /Kids nor /Names"),
        "{error}"
    );

    insert(
        &doc,
        36,
        Value::Dict(Dict::new().insert("Names", arr(vec![Value::String("Odd".into())]))),
    );
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(error.to_string().contains("not even"), "{error}");
}
