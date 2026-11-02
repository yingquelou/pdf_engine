//! 属性对象（Table 327/328）与 ClassMap 属性合并测试（14.7.5）。

use super::*;

#[test]
fn structure_attributes_expose_owner_and_user_properties() {
    let layout = layout_attribute(12).insert("Placement", name("Block"));
    let properties = arr(vec![
        Value::Dict(
            user_property("role", Value::String("caption".into()))
                .insert("F", Value::String("Caption".into()))
                .insert("H", Value::Bool(true)),
        ),
        Value::Dict(user_property("count", Value::Integer(3))),
        Value::Dict(user_property("empty", Value::Null)),
    ]);
    let user = user_properties(properties);
    let doc = malformed_document(Value::Dict(element("P", ROOT).insert(
        "A",
        arr(vec![Value::Dict(layout.clone()), Value::Dict(user)]),
    )));

    let tree = doc.structure_tree().expect("structure tree").expect("root");
    let element = tree.element(0).expect("first element");
    assert_eq!(element.attributes.len(), 2);
    assert_eq!(element.attributes[0].owner, "Layout");
    assert_eq!(element.attributes[0].value, Value::Dict(layout));
    assert!(element.attributes[0].user_properties.is_none());

    let attribute = &element.attributes[1];
    assert_eq!(attribute.owner, "UserProperties");
    let parsed = attribute.user_properties.as_ref().expect("user properties");
    assert_eq!(parsed.dict.get("O"), Some(&name("UserProperties")));
    assert_eq!(parsed.properties.len(), 3);
    assert_eq!(parsed.properties[0].name, "role");
    assert_eq!(parsed.properties[0].value, Value::String("caption".into()));
    assert_eq!(parsed.properties[0].formatted.as_deref(), Some("Caption"));
    assert!(parsed.properties[0].hidden);
    assert_eq!(
        parsed.properties[0].dict.get("F"),
        Some(&Value::String("Caption".into())),
        "原始属性字典应保留"
    );
    assert_eq!(parsed.properties[1].name, "count");
    assert_eq!(parsed.properties[1].value, Value::Integer(3));
    assert_eq!(parsed.properties[1].formatted, None);
    assert!(!parsed.properties[1].hidden, "/H 缺省 false");
    assert_eq!(parsed.properties[2].value, Value::Null, "/V 允许 null");
}

#[test]
fn class_map_attributes_merge_with_element_attributes() {
    let class_layout = layout_attribute(12).insert("Placement", name("Block"));
    let class_user = user_properties(arr(vec![Value::Dict(user_property(
        "role",
        Value::String("caption".into()),
    ))]));
    let root = struct_root(Dict::new())
        .insert(
            "ClassMap",
            Value::Dict(Dict::new().insert(
                "Body",
                arr(vec![Value::Dict(class_layout), Value::Dict(class_user)]),
            )),
        )
        .insert("K", arr(vec![Value::Ref(5, 0)]));
    let doc = document_with_root(root);
    let overlay = layout_attribute(24).insert("InlineAlign", name("Center"));
    insert(
        &doc,
        5,
        Value::Dict(
            element("P", ROOT)
                .insert("C", arr(vec![name("Body"), Value::Integer(2)]))
                .insert("A", arr(vec![Value::Dict(overlay), Value::Integer(3)])),
        ),
    );

    let tree = doc.structure_tree().expect("structure tree").expect("root");
    assert_eq!(tree.root().class_map["Body"].len(), 2);
    assert_eq!(
        tree.elements()[0].classes[0].attributes.len(),
        2,
        "类属性按类名回填"
    );

    let resolved = tree.resolved_attributes(0);
    assert_eq!(resolved.len(), 2, "同 owner 合并后仍有两个属性对象");
    assert_eq!(resolved[0].owner, "Layout");
    assert_eq!(resolved[0].revision, Some(3), "/A 修订号优先");
    let layout = resolved[0].value.as_dict().expect("layout dict");
    assert_eq!(layout.get("Placement"), Some(&name("Block")), "类键保留");
    assert_eq!(
        layout.get("SpaceBefore"),
        Some(&Value::Integer(24)),
        "/A 覆盖类键"
    );
    assert_eq!(
        layout.get("InlineAlign"),
        Some(&name("Center")),
        "/A 新键并入"
    );

    assert_eq!(resolved[1].owner, "UserProperties");
    assert_eq!(resolved[1].revision, Some(2), "未被 /A 覆盖时沿用类修订号");
    let user = resolved[1]
        .user_properties
        .as_ref()
        .expect("user properties");
    assert_eq!(user.properties[0].value, Value::String("caption".into()));

    assert!(tree.resolved_attributes(99).is_empty(), "越界下标返回空");
}
