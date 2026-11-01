//! 同全限定名字段多重表示（12.7.3.2）：`LogicalField` 归并与 `/FT`、`/V`、`/DV` 一致性校验。

use super::*;

fn rect(low: i64) -> Value {
    arr(vec![
        Value::Integer(low),
        Value::Integer(0),
        Value::Integer(low + 20),
        Value::Integer(20),
    ])
}

/// 单选组：具名父节点（`agree`）加若干无名 Widget 表示。
fn radio_group(kids: Vec<Value>) -> Dict {
    Dict::new()
        .insert("FT", name("Btn"))
        .insert("Ff", Value::Integer(1 << 15))
        .insert("T", text("agree"))
        .insert("V", name("1"))
        .insert("DV", name("Off"))
        .insert("Opt", arr(vec![text("first"), text("second")]))
        .insert("Kids", arr(kids))
}

fn form_of(acro: Value) -> AcroForm {
    let doc = doc_with_acro_form(acro);
    doc.acro_form().expect("parse").expect("form present")
}

/// 以单个 `agree` 单选组为根的表单，`kids` 为其 `/Kids` 条目。
fn form_with_group(kids: Vec<Value>) -> AcroForm {
    let group = Value::Dict(radio_group(kids));
    form_of(Value::Dict(Dict::new().insert("Fields", arr(vec![group]))))
}

/// `agree` 组加一个无名 Widget 表示，返回解析错误。
fn group_error(widget: Dict) -> FormError {
    form_error(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(radio_group(vec![Value::Dict(widget)]))]),
    )))
}

#[test]
fn unnamed_widgets_merge_into_the_named_field_group() {
    let form = form_with_group(vec![
        Value::Dict(widget(rect(0))),
        Value::Dict(widget(rect(30))),
    ]);
    let groups = form.logical_fields();
    assert_eq!(groups.len(), 1, "one logical field, three representations");
    let group = &groups[0];
    assert_eq!(group.fully_qualified_name, "agree");
    assert_eq!(
        group
            .representations
            .iter()
            .map(|node| node.partial_name.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("agree"), None, None],
        "representations stay in preorder"
    );
    assert_eq!(group.widgets.len(), 2, "only /Rect nodes count as widgets");
    assert_eq!(group.field_type, Some(FieldType::Button));
    let on = name("1");
    let off = name("Off");
    assert_eq!(group.value, Some(&on));
    assert_eq!(group.default_value, Some(&off));
    // 外观类条目允许各表示不同，不影响归并。
    assert_eq!(
        group
            .widgets
            .iter()
            .map(|node| node.rect.expect("widget /Rect"))
            .collect::<Vec<_>>(),
        vec![[0.0, 0.0, 20.0, 20.0], [30.0, 0.0, 50.0, 20.0]]
    );
    assert_eq!(
        form.logical_field("agree").as_ref(),
        Some(group),
        "lookup by fully qualified name"
    );
    assert!(form.logical_field("other").is_none());
}

/// Table 220 `/TU`：合并视图按前序首个表示暴露替代名，各表示自身的
/// `/TU` 经 `representations` 逐节点访问（12.7.3.2 仅要求 `/FT` `/V`
/// `/DV` 一致，替代名不在强约束之列）。
#[test]
fn logical_field_exposes_first_representation_alternate_name() {
    let parent = radio_group(vec![
        Value::Dict(widget(rect(0)).insert("TU", text("Visa"))),
        Value::Dict(widget(rect(30)).insert("TU", text("Mastercard"))),
    ])
    .insert("TU", text("I agree"));
    let form = form_of(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(parent)])),
    ));
    let group = form.logical_field("agree").expect("group present");
    assert_eq!(
        group.alternate_name,
        Some("I agree"),
        "the first representation's /TU is the merged alternate name"
    );
    assert_eq!(
        group
            .representations
            .iter()
            .map(|node| node.alternate_name.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("I agree"), Some("Visa"), Some("Mastercard")],
        "each representation keeps its own /TU"
    );
}

#[test]
fn named_children_are_their_own_logical_fields() {
    let form = form_of(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("payment")).insert(
                "Kids",
                arr(vec![
                        Value::Dict(
                            Dict::new()
                                .insert("T", text("card"))
                                .insert("FT", name("Tx"))
                                .insert("Kids", arr(vec![Value::Dict(widget(rect(0)))])),
                        ),
                        Value::Dict(widget(rect(40))),
                    ]),
            ),
        )]),
    )));
    assert_eq!(
        form.logical_fields()
            .iter()
            .map(|group| (
                group.fully_qualified_name.as_str(),
                group.representations.len(),
                group.widgets.len()
            ))
            .collect::<Vec<_>>(),
        vec![("payment", 2, 1), ("payment.card", 2, 1)],
        "a named child starts a new field; its unnamed widget folds in"
    );
}

#[test]
fn representations_without_a_name_are_not_grouped() {
    let unnamed = radio_group(Vec::new()).insert("T", text(""));
    let form = form_of(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(unnamed)])),
    ));
    assert_eq!(
        form.fields[0].fully_qualified_name, "",
        "an empty /T leaves the node unnamed"
    );
    assert!(
        form.logical_fields().is_empty(),
        "unnamed representations have no fully qualified name to group by"
    );
}

#[test]
fn conflicting_representations_are_typed_errors() {
    let cases = vec![
        (
            "/V",
            widget(rect(0)).insert("V", name("0")),
            "field agree has representations with different /V",
        ),
        (
            "/DV",
            widget(rect(0)).insert("DV", name("1")),
            "field agree has representations with different /DV",
        ),
        (
            "/FT",
            widget(rect(0)).insert("FT", name("Tx")),
            "field agree has representations with different /FT",
        ),
    ];
    for (label, kid, expected) in cases {
        let err = group_error(kid);
        match err {
            FormError::Malformed(message) => assert!(
                message.contains(expected),
                "{label} conflict should report {expected:?}, got {message:?}"
            ),
            other => panic!("{label} conflict should be Malformed, got {other:?}"),
        }
    }
}

#[test]
fn indirect_representations_are_compared_after_dereference() {
    let doc = doc_with_catalog(Dict::new().insert("AcroForm", Value::Ref(9, 0)));
    let fields = arr(vec![Value::Dict(radio_group(vec![
        Value::Ref(10, 0),
        Value::Ref(11, 0),
    ]))]);
    insert(&doc, 9, Value::Dict(Dict::new().insert("Fields", fields)));
    insert(&doc, 10, Value::Dict(widget(rect(0))));
    insert(
        &doc,
        11,
        Value::Dict(widget(rect(30)).insert("V", Value::Ref(12, 0))),
    );
    insert(&doc, 12, name("1"));
    let form = doc.acro_form().expect("parse").expect("present");
    let group = form.logical_field("agree").expect("group");
    assert_eq!(group.representations.len(), 3);
    assert_eq!(
        group.widgets[1].value,
        Some(Value::Ref(12, 0)),
        "the widget keeps its own /V object"
    );

    // 同一引用解引用后不同即冲突。
    insert(&doc, 12, name("0"));
    match doc
        .acro_form()
        .expect_err("dereferenced /V must still be compared")
    {
        FormError::Malformed(message) => assert!(
            message.contains("different /V"),
            "unexpected message: {message:?}"
        ),
        other => panic!("expected Malformed, got {other:?}"),
    }
}
