//! 属性所有者校验、用户属性（Table 328/329）与 ClassMap 数组测试（14.7.5）。

use crate::core::reader::structure::MAX_STRUCTURE_CHILDREN;

use super::*;

#[test]
fn structure_attribute_owner_is_required() {
    assert_malformed(
        &attribute_document(Dict::new().insert("Placement", name("Block"))),
        "/O is required",
    );
    assert_malformed(
        &attribute_document(Dict::new().insert("O", Value::Integer(1))),
        "/O must be a name",
    );
    assert_malformed(
        &attribute_document(Dict::new().insert("O", name(""))),
        "/O must not be empty",
    );

    let doc = document_with_root(struct_root(Dict::new().insert(
        "ClassMap",
        Value::Dict(Dict::new().insert("Body", Value::Dict(Dict::new()))),
    )));
    assert_malformed(&doc, "/O is required");
}

#[test]
fn user_properties_require_p_array_and_typed_entries() {
    assert_malformed(
        &attribute_document(Dict::new().insert("O", name("UserProperties"))),
        "/P is required",
    );
    assert_malformed(
        &attribute_document(user_properties(Value::Integer(1))),
        "/P must be an array",
    );
    assert_malformed(
        &attribute_document(user_properties(arr(vec![Value::Integer(1)]))),
        "must be a dictionary",
    );
    assert_malformed(
        &attribute_document(user_properties(arr(vec![Value::Dict(
            Dict::new().insert("V", Value::Integer(1)),
        )]))),
        "/N is required",
    );
    assert_malformed(
        &attribute_document(user_properties(arr(vec![Value::Dict(
            Dict::new()
                .insert("N", Value::Integer(1))
                .insert("V", Value::Integer(1)),
        )]))),
        "/N must be a text string",
    );
    assert_malformed(
        &attribute_document(user_properties(arr(vec![Value::Dict(
            Dict::new().insert("N", Value::String("role".into())),
        )]))),
        "/V is required",
    );
    assert_malformed(
        &attribute_document(user_properties(arr(vec![Value::Dict(
            user_property("role", Value::Integer(1)).insert("F", Value::Integer(2)),
        )]))),
        "/F must be a text string",
    );
    assert_malformed(
        &attribute_document(user_properties(arr(vec![Value::Dict(
            user_property("role", Value::Integer(1)).insert("H", Value::Integer(2)),
        )]))),
        "/H must be a boolean",
    );
}

#[test]
fn class_map_accepts_attribute_object_arrays() {
    let root = struct_root(Dict::new()).insert(
        "ClassMap",
        Value::Dict(
            Dict::new()
                .insert(
                    "Body",
                    arr(vec![
                        Value::Dict(layout_attribute(12)),
                        Value::Dict(user_properties(arr(vec![Value::Dict(user_property(
                            "role",
                            Value::String("caption".into()),
                        ))]))),
                    ]),
                )
                .insert("Empty", arr(Vec::new())),
        ),
    );
    let doc = document_with_root(root);
    let tree = doc.structure_tree().expect("structure tree").expect("root");
    let body = tree.root().class_map.get("Body").expect("Body");
    assert_eq!(body.len(), 2);
    assert_eq!(body[0].owner, "Layout");
    assert_eq!(body[1].owner, "UserProperties");
    assert!(
        body.iter().all(|attribute| attribute.revision.is_none()),
        "类属性不带修订号"
    );
    assert!(
        tree.root()
            .class_map
            .get("Empty")
            .expect("Empty")
            .is_empty(),
        "空数组等价于没有类属性"
    );

    let doc = document_with_root(struct_root(Dict::new().insert(
        "ClassMap",
        Value::Dict(Dict::new().insert(
            "Big",
            arr(vec![
                Value::Dict(layout_attribute(0));
                MAX_STRUCTURE_CHILDREN + 1
            ]),
        )),
    )));
    match doc.structure_tree() {
        Err(StructureError::LimitExceeded(what)) => {
            assert_eq!(what, "structure class map attributes");
        }
        other => panic!("expected class map attribute limit, got {other:?}"),
    }
}
