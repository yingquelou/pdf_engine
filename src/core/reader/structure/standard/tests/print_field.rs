//! 标准结构 PrintField owner 属性读端测试（ISO 32000-1 14.8.5.5）。

use super::*;

#[test]
fn print_field_attributes_parse_role_checked_and_description() {
    let doc = document_with_elements(vec![
        (
            5,
            element("Form").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("PrintField"))
                        .insert("Role", name("rb"))
                        .insert("checked", name("on"))
                        .insert("Desc", Value::String("Radio button".into())),
                ),
            ),
        ),
        (
            6,
            element("Form").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("PrintField"))
                        .insert("Role", name("tv"))
                        .insert("checked", name("neutral")),
                ),
            ),
        ),
        (
            7,
            element("Form").insert(
                "A",
                Value::Dict(Dict::new().insert("O", name("PrintField"))),
            ),
        ),
        (
            8,
            element("Form").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("PrintField"))
                        .insert("Role", name("xx"))
                        .insert("checked", name("maybe"))
                        .insert("Desc", Value::HexString("48656C6C6F".into())),
                ),
            ),
        ),
    ]);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let radio = tree.standard_attributes(0).expect("print field");
    let field = radio.print_field.expect("PrintField owner");
    assert_eq!(field.role, Some(PrintFieldRole::RadioButton));
    assert_eq!(field.checked, PrintFieldChecked::On);
    assert_eq!(field.description.as_deref(), Some("Radio button"));

    let neutral = tree.standard_attributes(1).expect("print field");
    let field = neutral.print_field.expect("PrintField owner");
    assert_eq!(field.role, Some(PrintFieldRole::TextValue));
    assert_eq!(field.checked, PrintFieldChecked::Neutral);
    assert_eq!(field.description, None);

    let defaults = tree.standard_attributes(2).expect("print field");
    let field = defaults.print_field.expect("PrintField owner");
    assert_eq!(field.role, None);
    assert_eq!(field.checked, PrintFieldChecked::Off, "/checked 缺省为 off");
    assert_eq!(field.description, None);

    let unknown = tree.standard_attributes(3).expect("print field");
    let field = unknown.print_field.expect("PrintField owner");
    assert_eq!(field.role, Some(PrintFieldRole::Other("xx".into())));
    assert_eq!(field.checked, PrintFieldChecked::Other("maybe".into()));
    assert_eq!(field.description.as_deref(), Some("Hello"));
}
