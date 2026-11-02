//! Catalog /MarkInfo 布尔条目读取测试（14.7.2）。

use super::*;

#[test]
fn mark_info_reads_boolean_entries() {
    let mut doc = Document::new();
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new().insert("Type", name("Catalog")).insert(
                "MarkInfo",
                Value::Dict(
                    Dict::new()
                        .insert("Marked", Value::Bool(true))
                        .insert("Suspects", Value::Bool(true))
                        .insert("Custom", Value::Integer(1)),
                ),
            ),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };

    let info = doc.mark_info().expect("mark info").expect("declared");
    assert!(info.marked);
    assert!(!info.user_properties, "缺省 false");
    assert!(info.suspects);
    assert_eq!(info.dict.get("Custom"), Some(&Value::Integer(1)));

    // 空 /MarkInfo：三个布尔条目均为缺省 false。
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("MarkInfo", Value::Dict(Dict::new())),
        ),
    );
    let info = doc.mark_info().expect("mark info").expect("declared");
    assert!(!info.marked && !info.user_properties && !info.suspects);

    // 目录没有 /MarkInfo。
    insert(
        &doc,
        CATALOG,
        Value::Dict(Dict::new().insert("Type", name("Catalog"))),
    );
    assert_eq!(doc.mark_info(), Ok(None));

    // 非布尔条目是类型化错误。
    insert(
        &doc,
        CATALOG,
        Value::Dict(Dict::new().insert("Type", name("Catalog")).insert(
            "MarkInfo",
            Value::Dict(Dict::new().insert("Marked", Value::Integer(1))),
        )),
    );
    match doc.mark_info() {
        Err(StructureError::Malformed(message)) => {
            assert!(message.contains("must be a boolean"), "{message}");
        }
        other => panic!("expected malformed mark info, got {other:?}"),
    }
}
