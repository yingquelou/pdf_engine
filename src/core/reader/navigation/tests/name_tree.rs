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
fn name_tree_accepts_valid_limits_on_leaf_and_intermediate_nodes() {
    let doc = document_with_pages(1, names_catalog("Dests", 40));
    insert(
        &doc,
        40,
        Value::Dict(
            Dict::new()
                .insert("Kids", arr(vec![Value::Ref(41, 0), Value::Ref(42, 0)]))
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("Alpha".into()),
                        Value::String("Gamma".into()),
                    ]),
                ),
        ),
    );
    insert(
        &doc,
        41,
        Value::Dict(
            Dict::new()
                .insert(
                    "Names",
                    arr(vec![
                        Value::String("Alpha".into()),
                        Value::Integer(1),
                        Value::String("Delta".into()),
                        Value::Integer(4),
                    ]),
                )
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("Alpha".into()),
                        Value::String("Delta".into()),
                    ]),
                ),
        ),
    );
    insert(
        &doc,
        42,
        Value::Dict(
            Dict::new()
                .insert(
                    "Names",
                    arr(vec![
                        Value::String("Beta".into()),
                        Value::Integer(2),
                        Value::String("Gamma".into()),
                        Value::Integer(3),
                    ]),
                )
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("Beta".into()),
                        Value::String("Gamma".into()),
                    ]),
                ),
        ),
    );
    let tree = doc.name_tree("Dests").unwrap();
    let keys: Vec<&str> = tree.keys().map(String::as_str).collect();
    assert_eq!(keys, ["Alpha", "Beta", "Delta", "Gamma"]);
}

#[test]
fn name_tree_rejects_malformed_limits_shape() {
    for limits in [
        Value::Integer(7),
        arr(vec![Value::String("A".into())]),
        arr(vec![Value::String("A".into()), Value::Integer(2)]),
        arr(vec![
            Value::String("A".into()),
            Value::String("Z".into()),
            Value::String("extra".into()),
        ]),
    ] {
        let doc = document_with_pages(1, names_catalog("Dests", 43));
        insert(
            &doc,
            43,
            Value::Dict(
                Dict::new()
                    .insert("Names", arr(vec![]))
                    .insert("Limits", limits),
            ),
        );
        let error = doc.name_tree("Dests").unwrap_err();
        assert!(
            matches!(error, NavigationError::Malformed(_)) && error.to_string().contains("/Limits"),
            "{error}"
        );
    }
}

#[test]
fn name_tree_rejects_limits_lower_bound_above_upper() {
    let doc = document_with_pages(1, names_catalog("Dests", 44));
    insert(
        &doc,
        44,
        Value::Dict(Dict::new().insert("Names", arr(vec![])).insert(
            "Limits",
            arr(vec![Value::String("Z".into()), Value::String("A".into())]),
        )),
    );
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("lower bound exceeds upper bound"),
        "{error}"
    );
}

#[test]
fn name_tree_rejects_limits_not_covering_leaf_keys() {
    let doc = document_with_pages(1, names_catalog("Dests", 45));
    insert(
        &doc,
        45,
        Value::Dict(
            Dict::new()
                .insert(
                    "Names",
                    arr(vec![
                        Value::String("Alpha".into()),
                        Value::Integer(1),
                        Value::String("Zulu".into()),
                        Value::Integer(2),
                    ]),
                )
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("Beta".into()),
                        Value::String("Yankee".into()),
                    ]),
                ),
        ),
    );
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(
        error.to_string().contains("does not cover the node keys"),
        "{error}"
    );
}

#[test]
fn name_tree_rejects_intermediate_limits_not_covering_descendants() {
    let doc = document_with_pages(1, names_catalog("Dests", 46));
    insert(
        &doc,
        46,
        Value::Dict(
            Dict::new()
                .insert("Kids", arr(vec![Value::Ref(47, 0)]))
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("Beta".into()),
                        Value::String("Yankee".into()),
                    ]),
                ),
        ),
    );
    insert(
        &doc,
        47,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Alpha".into()),
                Value::Integer(1),
                Value::String("Zulu".into()),
                Value::Integer(2),
            ]),
        )),
    );
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(
        error.to_string().contains("does not cover the node keys"),
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
