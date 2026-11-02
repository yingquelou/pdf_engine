use super::*;

#[test]
fn type3_font_reads_direct_resources() {
    let document = Document::new();
    let resources = sample_resources();
    let font = Value::Dict(type3_dict_with_resources(Value::Dict(resources.clone())));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.resources(&document).unwrap(), Some(resources));
}

#[test]
fn type3_font_resolves_indirect_resources() {
    let document = Document::new();
    // /Resources 为间接引用，指向对象 40 中的字典。
    let resources = Dict::new().insert(
        "ColorSpace",
        Value::Dict(Dict::new().insert("CS0", Value::Name("DeviceRGB".into()))),
    );
    document.set_object(Object::new(40, 0, Value::Dict(resources.clone())));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(40, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.resources(&document).unwrap(), Some(resources));
}

#[test]
fn type3_font_resources_missing_returns_none() {
    let document = Document::new();
    let font = type3_font(differences_encoding());
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(parsed.resources(&document).unwrap(), None);
}

#[test]
fn type3_font_resources_rejects_non_dict_and_unresolved() {
    let document = Document::new();

    // 直接值不是字典。
    let font = Value::Dict(type3_dict_with_resources(Value::Integer(1)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "/Resources in Type 3 font is not a dictionary",
    );

    // 间接引用无法解析。
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(99, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "unresolved object reference 99 0 R",
    );

    // 间接引用的目标不是字典。
    document.set_object(Object::new(41, 0, Value::Name("NotADict".into())));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(41, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "/Resources in Type 3 font is not a dictionary",
    );
}

/// `/Resources` 间接引用链（多级）应解引用到最终字典。
#[test]
fn type3_resources_follow_multi_level_indirect_chain() {
    let document = Document::new();
    let resources = sample_resources();
    document.set_object(Object::new(10, 0, Value::Ref(11, 0)));
    document.set_object(Object::new(11, 0, Value::Ref(12, 0)));
    document.set_object(Object::new(12, 0, Value::Dict(resources.clone())));

    let font = Value::Dict(type3_dict_with_resources(Value::Ref(10, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_eq!(parsed.resources(&document).unwrap(), Some(resources));
}

/// 引用环与超出深度上限均返回类型化错误。
#[test]
fn type3_resources_reject_reference_cycle_and_depth_overflow() {
    // 环：10 → 10。
    let document = Document::new();
    document.set_object(Object::new(10, 0, Value::Ref(10, 0)));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(10, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "reference cycle at 10 0 R",
    );

    // 17 层引用超过 16 层上限。
    let document = Document::new();
    for num in 100..117 {
        document.set_object(Object::new(num, 0, Value::Ref(num + 1, 0)));
    }
    document.set_object(Object::new(117, 0, Value::Dict(sample_resources())));
    let font = Value::Dict(type3_dict_with_resources(Value::Ref(100, 0)));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_contains(
        parsed.resources(&document).unwrap_err(),
        "exceeds 16 indirect references",
    );
}

/// 类别字典可为间接引用链；资源名按 `#xx` 解码后匹配。
#[test]
fn type3_resource_resolves_category_chain_and_escaped_names() {
    let document = Document::new();
    document.set_object(Object::new(
        20,
        0,
        Value::Dict(
            Dict::new()
                .insert("Im#201", Value::Ref(40, 0))
                .insert("Plain", Value::Ref(41, 0)),
        ),
    ));
    let resources = Dict::new().insert("XObject", Value::Ref(20, 0));
    let font = Value::Dict(type3_dict_with_resources(Value::Dict(resources)));
    let parsed = document.type3_font(&font).unwrap().unwrap();

    assert_eq!(
        parsed.resource(&document, "XObject", "Im 1").unwrap(),
        Some(Value::Ref(40, 0))
    );
    assert_eq!(
        parsed.resource(&document, "XObject", "Plain").unwrap(),
        Some(Value::Ref(41, 0))
    );
    assert_eq!(
        parsed.resource(&document, "XObject", "Missing").unwrap(),
        None
    );
    // 类别缺失或类别不是字典时返回 None。
    assert_eq!(parsed.resource(&document, "Pattern", "Im 1").unwrap(), None);
    document.set_object(Object::new(21, 0, Value::Integer(1)));
    let font = Value::Dict(type3_dict_with_resources(Value::Dict(
        Dict::new().insert("XObject", Value::Ref(21, 0)),
    )));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_eq!(parsed.resource(&document, "XObject", "Im 1").unwrap(), None);
}
