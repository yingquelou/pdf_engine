//! 畸形结构条目诊断与子项数量上限测试（14.7.4）。

use crate::core::reader::structure::MAX_STRUCTURE_CHILDREN;

use super::*;

#[test]
fn structure_tree_reports_malformed_entries() {
    // /S 缺失（无 /Type 的字典按结构元素处理）。
    assert_malformed(
        &malformed_document(Value::Dict(Dict::new().insert("P", Value::Ref(ROOT, 0)))),
        "/S is required",
    );

    // /P 不是间接引用。
    assert_malformed(
        &malformed_document(Value::Dict(
            element("P", ROOT).insert("P", Value::Integer(ROOT)),
        )),
        "/P must be an indirect reference",
    );

    // MCR 缺 /MCID。
    let doc = malformed_document(Value::Dict(
        element("P", ROOT).insert("Pg", Value::Ref(PAGE, 0)).insert(
            "K",
            arr(vec![Value::Dict(
                Dict::new()
                    .insert("Type", name("MCR"))
                    .insert("Pg", Value::Ref(PAGE, 0)),
            )]),
        ),
    ));
    insert(&doc, PAGE, Value::Dict(Dict::new()));
    assert_malformed(&doc, "/MCID is required");

    // MCR 无 /Pg 且父元素无 /Pg。
    assert_malformed(
        &malformed_document(Value::Dict(element("P", ROOT).insert(
            "K",
            arr(vec![Value::Dict(
                Dict::new()
                    .insert("Type", name("MCR"))
                    .insert("MCID", Value::Integer(1)),
            )]),
        ))),
        "parent structure element has no /Pg",
    );

    // 未知 /K 字典类型。
    assert_malformed(
        &malformed_document(Value::Dict(element("P", ROOT).insert(
            "K",
            arr(vec![Value::Dict(Dict::new().insert("Type", name("Bogus")))]),
        ))),
        "unknown /Type",
    );

    // /P 指向了错误的父元素。
    let doc = malformed_document(Value::Dict(
        element("P", ROOT).insert("K", arr(vec![Value::Ref(6, 0)])),
    ));
    insert(&doc, 6, Value::Dict(element("Span", 99)));
    insert(&doc, 99, Value::Dict(Dict::new()));
    assert_malformed(&doc, "must reference its parent object 5");

    // /K 内嵌数组。
    assert_malformed(
        &malformed_document(Value::Dict(
            element("P", ROOT).insert("K", arr(vec![arr(vec![Value::Integer(0)])])),
        )),
        "nested /K arrays",
    );
}

#[test]
fn structure_tree_limits_are_enforced() {
    let doc = document_with_root(struct_root(
        Dict::new().insert("K", arr(vec![Value::Ref(100, 0)])),
    ));
    for index in 0..65 {
        let number = 100 + index;
        let parent = if index == 0 { ROOT } else { number - 1 };
        let mut entry = element("P", parent);
        if index + 1 < 65 {
            entry = entry.insert("K", arr(vec![Value::Ref(number + 1, 0)]));
        }
        insert(&doc, number, Value::Dict(entry));
    }

    match doc.structure_tree() {
        Err(StructureError::LimitExceeded(what)) => {
            assert_eq!(what, "structure tree depth");
        }
        other => panic!("expected depth limit error, got {other:?}"),
    }

    // 元素 `/A` 条目数上限。
    let doc = malformed_document(Value::Dict(element("P", ROOT).insert(
        "A",
        arr(vec![Value::Integer(0); MAX_STRUCTURE_CHILDREN + 1]),
    )));
    match doc.structure_tree() {
        Err(StructureError::LimitExceeded(what)) => {
            assert_eq!(what, "structure attributes");
        }
        other => panic!("expected attribute limit error, got {other:?}"),
    }
}
