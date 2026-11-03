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

/// 构造仅带旧式目录 `/Dests` 字典（无名称树）的文档；`dests` 是字典条目。
fn legacy_dests_document(page_count: usize, dests: Dict) -> Document {
    let catalog = Dict::new().insert("Dests", Value::Dict(dests));
    document_with_pages(page_count, catalog)
}

/// `<< /A << /S /GoTo /D <target> >> >>` 字典条目。
fn go_to_action_entry(target: Value) -> Value {
    Value::Dict(Dict::new().insert(
        "A",
        Value::Dict(Dict::new().insert("S", name("GoTo")).insert("D", target)),
    ))
}

#[test]
fn named_destination_dictionary_resolves_go_to_action_array_target() {
    // NOTE 2：go-to 动作可作为命名目标的载体；其 `/D` 是 Table 151 数组。
    let doc = legacy_dests_document(
        3,
        Dict::new().insert(
            "Go",
            go_to_action_entry(arr(vec![page_ref(1), name("Fit")])),
        ),
    );
    assert_eq!(
        doc.named_destination("Go").unwrap(),
        Some(Destination::Fit { page: 1 })
    );
}

#[test]
fn named_destination_dictionary_resolves_go_to_action_name_target() {
    // `/A` 的 `/D` 为名字对象：递归解析被指向的命名目标（旧式字典内）。
    let doc = legacy_dests_document(
        3,
        Dict::new()
            .insert("From", go_to_action_entry(name("To")))
            .insert("To", arr(vec![page_ref(2), name("Fit")])),
    );
    assert_eq!(
        doc.named_destination("From").unwrap(),
        Some(Destination::Fit { page: 2 })
    );
}

#[test]
fn named_destination_dictionary_resolves_two_hop_go_to_action_name_target() {
    // 两跳间接引用（From → Via → To）验证递归链能解析多跳。
    let doc = legacy_dests_document(
        3,
        Dict::new()
            .insert("From", go_to_action_entry(name("Via")))
            .insert("Via", go_to_action_entry(name("To")))
            .insert("To", arr(vec![page_ref(0), name("FitB")])),
    );
    assert_eq!(
        doc.named_destination("From").unwrap(),
        Some(Destination::FitB { page: 0 })
    );
}

#[test]
fn named_destination_dictionary_prefers_d_over_a_when_both_present() {
    // `/D` 与 `/A` 并存：规范无 shall 排序，本库优先取显式 `/D`。
    let doc = legacy_dests_document(
        3,
        Dict::new().insert(
            "Both",
            Value::Dict(
                Dict::new()
                    .insert("D", arr(vec![page_ref(0), name("Fit")]))
                    .insert("A", go_to_action_entry(arr(vec![page_ref(1), name("Fit")]))),
            ),
        ),
    );
    assert_eq!(
        doc.named_destination("Both").unwrap(),
        Some(Destination::Fit { page: 0 })
    );
}

#[test]
fn named_destination_dictionary_keeps_unknown_attributes_and_resolves_d() {
    // NOTE 2 前半句的 additional attributes 不在本笔范围：未知额外键不解析、
    // 不报错，只要 `/D` 存在即照常取值。
    let doc = legacy_dests_document(
        3,
        Dict::new().insert(
            "Attr",
            Value::Dict(
                Dict::new()
                    .insert("D", arr(vec![page_ref(2), name("Fit")]))
                    .insert("ModDate", Value::String("D:20200101".into())),
            ),
        ),
    );
    assert_eq!(
        doc.named_destination("Attr").unwrap(),
        Some(Destination::Fit { page: 2 })
    );
}

#[test]
fn named_destination_dictionary_go_to_action_cycle_is_rejected() {
    // 自环 From → From：环检测命中 Malformed，不无限递归。
    let doc = legacy_dests_document(
        3,
        Dict::new().insert("From", go_to_action_entry(name("From"))),
    );
    let error = doc.named_destination("From").unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");

    // 互环 A → B → A。
    let doc = legacy_dests_document(
        3,
        Dict::new()
            .insert("A", go_to_action_entry(name("B")))
            .insert("B", go_to_action_entry(name("A"))),
    );
    let error = doc.named_destination("A").unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");
}

#[test]
fn named_destination_dictionary_dangling_go_to_action_name_is_malformed() {
    // `/A` 的 `/D` 指向不存在的命名目标：dangling 引用作 Malformed。
    let doc = legacy_dests_document(
        3,
        Dict::new().insert("From", go_to_action_entry(name("Absent"))),
    );
    let error = doc.named_destination("From").unwrap_err();
    assert!(error.to_string().contains("not defined"), "{error}");
}

#[test]
fn named_destination_dictionary_without_d_or_a_is_malformed() {
    // 既无 `/D` 也无 `/A`（仅有未知的附加属性）：保持 Malformed，文案同提两者。
    let doc = legacy_dests_document(
        2,
        Dict::new().insert(
            "NoTarget",
            Value::Dict(Dict::new().insert("ModDate", Value::String("x".into()))),
        ),
    );
    let error = doc.named_destination("NoTarget").unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("/D") && message.contains("/A"),
        "{message}"
    );

    // 完全空字典同样命中该分支。
    let doc = legacy_dests_document(2, Dict::new().insert("Empty", Value::Dict(Dict::new())));
    let error = doc.named_destination("Empty").unwrap_err();
    assert!(error.to_string().contains("/D"), "{error}");
}

#[test]
fn named_destination_dictionary_non_goto_action_is_rejected() {
    // NOTE 2 只授权 go-to 动作；其它 `/S` subtype 返回 Malformed（库策略）。
    let doc = legacy_dests_document(
        3,
        Dict::new().insert(
            "Uri",
            Value::Dict(
                Dict::new().insert(
                    "A",
                    Value::Dict(
                        Dict::new()
                            .insert("S", name("URI"))
                            .insert("D", arr(vec![page_ref(0), name("Fit")])),
                    ),
                ),
            ),
        ),
    );
    let error = doc.named_destination("Uri").unwrap_err();
    assert!(error.to_string().contains("/GoTo"), "{error}");
}

#[test]
fn named_destination_dictionary_go_to_action_missing_d_or_bad_action_is_malformed() {
    // `/A` 非字典。
    let doc = legacy_dests_document(
        2,
        Dict::new().insert("Bad", Value::Dict(Dict::new().insert("A", name("GoTo")))),
    );
    let error = doc.named_destination("Bad").unwrap_err();
    assert!(error.to_string().contains("action"), "{error}");

    // `/A` 缺 `/D`。
    let doc = legacy_dests_document(
        2,
        Dict::new().insert(
            "NoD",
            Value::Dict(
                Dict::new().insert("A", Value::Dict(Dict::new().insert("S", name("GoTo")))),
            ),
        ),
    );
    let error = doc.named_destination("NoD").unwrap_err();
    assert!(error.to_string().contains("missing /D"), "{error}");
}

#[test]
fn named_destination_go_to_action_resolves_from_name_tree_channel_too() {
    // 名称树条目的字典形态也支持 `/A` go-to（1.2+ 通道同一句取值）。
    let doc = document_with_pages(3, names_catalog("Dests", 60));
    insert(
        &doc,
        60,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Go".into()),
                go_to_action_entry(arr(vec![page_ref(1), name("Fit")])),
            ]),
        )),
    );
    assert_eq!(
        doc.named_destination("Go").unwrap(),
        Some(Destination::Fit { page: 1 })
    );
}
