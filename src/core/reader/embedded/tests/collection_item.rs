//! 集合项 `/CI` 读端测试（7.11.6 Table 48/49，PDF 1.7）。

use super::*;

/// 目录 `/Names /EmbeddedFiles` 含一个名为 `a.pdf` 的文件规格。
fn doc_with_filespec(entries: Vec<(&str, Value)>) -> Document {
    doc_with_embedded_node(leaf(vec![(
        Value::String("a.pdf".into()),
        filespec_dict(entries),
    )]))
}

#[test]
fn parses_collection_item_values() {
    let doc = doc_with_filespec(vec![("CI", Value::Ref(40, 0))]);
    let subitem_text = Dict::new()
        .insert("Type", name("CollectionSubitem"))
        .insert("D", text("D:20261030120000Z"))
        .insert("P", text("Issued: "));
    let subitem_number = Dict::new()
        .insert("Type", name("CollectionSubitem"))
        .insert("D", Value::Integer(42));
    let ci = Dict::new()
        .insert("Type", name("CollectionItem"))
        .insert("Title", text("Annual Report"))
        .insert("Copies", Value::Integer(2))
        .insert("Score", Value::Float(4.5))
        .insert("Issued", Value::Dict(subitem_text))
        .insert("Pages", Value::Dict(subitem_number));
    doc.set_object(Object::new(40, 0, Value::Dict(ci)));

    let files = doc.embedded_files().expect("embedded files");
    let item = files
        .get("a.pdf")
        .expect("filespec")
        .collection_item
        .as_ref()
        .expect("/CI");
    assert_eq!(
        item.values.get("Title"),
        Some(&CollectionItemValue::Text("Annual Report".into()))
    );
    assert_eq!(
        item.values.get("Copies"),
        Some(&CollectionItemValue::Number(2.0))
    );
    assert_eq!(
        item.values.get("Score"),
        Some(&CollectionItemValue::Number(4.5))
    );
    let issued = match item.values.get("Issued").expect("Issued") {
        CollectionItemValue::Subitem(subitem) => subitem,
        other => panic!("expected subitem, got {other:?}"),
    };
    assert_eq!(
        issued.datum,
        Some(CollectionSubitemValue::Text("D:20261030120000Z".into()))
    );
    assert_eq!(issued.prefix.as_deref(), Some("Issued: "));
    assert_eq!(
        issued.dict.get("Type"),
        Some(&Value::Name("CollectionSubitem".into()))
    );
    let pages = match item.values.get("Pages").expect("Pages") {
        CollectionItemValue::Subitem(subitem) => subitem,
        other => panic!("expected subitem, got {other:?}"),
    };
    assert_eq!(pages.datum, Some(CollectionSubitemValue::Number(42.0)));
    assert!(pages.prefix.is_none());
    assert_eq!(
        item.dict.get("Type"),
        Some(&Value::Name("CollectionItem".into()))
    );

    // 未声明 `/CI` 时为 `None`。
    let plain = doc_with_filespec(vec![("F", Value::String("a.pdf".into()))]);
    let files = plain.embedded_files().expect("embedded files");
    assert!(
        files
            .get("a.pdf")
            .expect("filespec")
            .collection_item
            .is_none()
    );
}

#[test]
fn malformed_collection_items_are_typed_errors() {
    let cases: Vec<(Value, Value, &str)> = vec![
        // 直接字典：Table 44 要求间接引用。
        (
            Value::Dict(Dict::new().insert("Type", name("CollectionItem"))),
            Value::Null,
            "must be an indirect reference",
        ),
        // 引用目标不是字典。
        (Value::Ref(40, 0), Value::Integer(7), "is not a dictionary"),
        // `/Type` 不匹配。
        (
            Value::Ref(40, 0),
            Value::Dict(Dict::new().insert("Type", name("CollectionField"))),
            "/Type must be CollectionItem",
        ),
        // 条目为布尔。
        (
            Value::Ref(40, 0),
            Value::Dict(Dict::new().insert("Flag", Value::Bool(true))),
            "must be a text string, number or collection subitem dictionary",
        ),
        // 条目引用解析后不是字典。
        (
            Value::Ref(40, 0),
            Value::Dict(Dict::new().insert("Other", Value::Ref(41, 0))),
            "is not a collection subitem dictionary",
        ),
        // 子项 `/Type` 不匹配。
        (
            Value::Ref(40, 0),
            Value::Dict(Dict::new().insert(
                "Datum",
                Value::Dict(Dict::new().insert("Type", name("CollectionItem"))),
            )),
            "collection subitem /Type must be CollectionSubitem",
        ),
        // 子项 `/D` 为布尔。
        (
            Value::Ref(40, 0),
            Value::Dict(Dict::new().insert(
                "Datum",
                Value::Dict(Dict::new().insert("D", Value::Bool(false))),
            )),
            "/D must be a text string or number",
        ),
        // 子项 `/P` 为数值。
        (
            Value::Ref(40, 0),
            Value::Dict(Dict::new().insert(
                "Datum",
                Value::Dict(Dict::new().insert("P", Value::Integer(1))),
            )),
            "/P is not a text string",
        ),
    ];
    for (ci, object, expected) in cases {
        let doc = doc_with_filespec(vec![("CI", ci)]);
        doc.set_object(Object::new(40, 0, object));
        doc.set_object(Object::new(41, 0, Value::String("not a dict".into())));
        let error = doc
            .embedded_files()
            .expect_err("malformed /CI must be rejected");
        assert!(
            error.to_string().contains(expected),
            "expected {expected:?} in {error}"
        );
    }
}
