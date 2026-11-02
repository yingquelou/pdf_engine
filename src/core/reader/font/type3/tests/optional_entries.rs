use super::*;

/// Table 112 可选条目：`/Name`、`/FontDescriptor` 与 `/ToUnicode`。
#[test]
fn type3_font_parses_name_descriptor_and_to_unicode() {
    let document = Document::new();
    let descriptor = Dict::new()
        .insert("Type", Value::Name("FontDescriptor".into()))
        .insert("FontName", Value::Name("SampleType3".into()))
        .insert("Flags", Value::Integer(4))
        .insert(
            "FontBBox",
            Value::Array(
                Array::new()
                    .push(Value::Integer(0))
                    .push(Value::Integer(-200))
                    .push(Value::Integer(1000))
                    .push(Value::Integer(900)),
            ),
        )
        .insert("ItalicAngle", Value::Integer(0))
        .insert("Ascent", Value::Integer(800))
        .insert("Descent", Value::Integer(-200))
        .insert("CapHeight", Value::Integer(700))
        .insert("StemV", Value::Integer(80));
    let font = Value::Dict(
        type3_dict(differences_encoding())
            .insert("Name", Value::Name("SampleType3".into()))
            .insert("FontDescriptor", Value::Dict(descriptor))
            .insert("ToUnicode", char_proc_with(b"begincmap endcmap")),
    );

    let parsed = document.type3_font(&font).unwrap().unwrap();
    assert_eq!(parsed.name.as_deref(), Some("SampleType3"));
    let descriptor = parsed.descriptor.expect("应解析 /FontDescriptor");
    assert_eq!(descriptor.font_name.as_deref(), Some("SampleType3"));
    assert_eq!(descriptor.font_bbox, Some([0.0, -200.0, 1000.0, 900.0]));
    assert_eq!(descriptor.ascent, Some(800.0));
    assert!(parsed.has_to_unicode);
}

/// 可选条目存在但类型非法时整份字体解析失败。
#[test]
fn type3_font_rejects_invalid_name_descriptor_and_to_unicode() {
    let document = Document::new();

    let font = Value::Dict(type3_dict(differences_encoding()).insert("Name", Value::Integer(1)));
    assert_contains(document.type3_font(&font).unwrap_err(), "/Name");

    let font =
        Value::Dict(type3_dict(differences_encoding()).insert("FontDescriptor", Value::Integer(1)));
    assert_contains(
        document.type3_font(&font).unwrap_err(),
        "/FontDescriptor is not a dictionary",
    );

    // /FontDescriptor 缺少必需 /Type。
    let font = Value::Dict(
        type3_dict(differences_encoding()).insert("FontDescriptor", Value::Dict(Dict::new())),
    );
    assert_contains(document.type3_font(&font).unwrap_err(), "/Type");

    let font = Value::Dict(
        type3_dict(differences_encoding()).insert("ToUnicode", Value::Name("X".into())),
    );
    assert_contains(
        document.type3_font(&font).unwrap_err(),
        "/ToUnicode in Type 3 font is not a stream",
    );
}

/// 未提供可选条目时字段为 `None` / `false`。
#[test]
fn type3_font_omits_optional_entries_when_absent() {
    let document = Document::new();
    let parsed = document
        .type3_font(&type3_font(differences_encoding()))
        .unwrap()
        .unwrap();

    assert_eq!(parsed.name, None);
    assert_eq!(parsed.descriptor, None);
    assert!(!parsed.has_to_unicode);
}
