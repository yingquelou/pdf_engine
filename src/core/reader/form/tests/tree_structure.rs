//! 字段树结构读取测试：继承、全限定名与 Widget 节点状态（12.7.2）。

use super::*;

#[test]
fn acro_form_reads_field_tree_with_inheritance() {
    let doc = doc_with_acro_form(Value::Ref(20, 0));
    insert(
        &doc,
        20,
        Value::Dict(
            Dict::new()
                .insert("Fields", arr(vec![Value::Ref(21, 0)]))
                .insert("NeedAppearances", Value::Bool(true))
                .insert("DA", text("/Helv 12 Tf 0 g"))
                .insert("DR", Value::Dict(Dict::new())),
        ),
    );
    insert(
        &doc,
        21,
        Value::Dict(
            Dict::new()
                .insert("T", text("person"))
                .insert("FT", name("Tx"))
                .insert("DA", text("/Courier 10 Tf 0 g"))
                .insert("Q", Value::Integer(2))
                .insert("Kids", arr(vec![Value::Ref(22, 0)])),
        ),
    );
    insert(
        &doc,
        22,
        Value::Dict(
            Dict::new()
                .insert("T", text("name"))
                .insert("V", text("Alice"))
                .insert("MaxLen", Value::Integer(40))
                .insert("Ff", Value::Integer(1))
                .insert(
                    "Kids",
                    arr(vec![Value::Dict(widget(arr(vec![
                        Value::Integer(10),
                        Value::Integer(20),
                        Value::Integer(110),
                        Value::Integer(40),
                    ])))]),
                ),
        ),
    );

    let form = doc.acro_form().expect("parse").expect("present");
    assert!(form.need_appearances);
    assert_eq!(form.default_appearance, Some(text("/Helv 12 Tf 0 g")));
    let dr = form.default_resources.as_ref().expect("/DR present");
    assert!(dr.fonts.is_empty());
    assert!(dr.ext_g_states.is_empty());
    assert!(dr.color_spaces.is_empty());
    assert!(dr.proc_set.is_empty());
    assert_eq!(dr.dict, Dict::new());
    assert!(form.xfa.is_none());
    assert_eq!(form.fields.len(), 1);

    let person = &form.fields[0];
    assert_eq!(person.partial_name.as_deref(), Some("person"));
    assert_eq!(person.fully_qualified_name, "person");
    assert_eq!(person.field_type, Some(FieldType::Text));
    assert_eq!(person.kids.len(), 1);

    let name_field = &person.kids[0];
    assert_eq!(name_field.fully_qualified_name, "person.name");
    assert_eq!(name_field.field_type, Some(FieldType::Text));
    assert_eq!(
        name_field.default_appearance,
        Some(text("/Courier 10 Tf 0 g"))
    );
    assert_eq!(name_field.quadding, Some(2));
    assert_eq!(name_field.value, Some(text("Alice")));
    assert_eq!(name_field.max_len, Some(40));
    assert!(name_field.flags.read_only());
    assert!(!name_field.flags.required());
    assert_eq!(name_field.kids.len(), 1);

    let widget = &name_field.kids[0];
    assert!(widget.is_widget);
    assert_eq!(widget.fully_qualified_name, "person.name");
    assert_eq!(widget.rect, Some([10.0, 20.0, 110.0, 40.0]));
    assert!(widget.appearance.is_none());
}

#[test]
fn widget_normal_appearance_states_are_extracted() {
    let states = Dict::new()
        .insert("Off", Value::Ref(30, 0))
        .insert("On", Value::Ref(31, 0));
    let widget_dict = widget(arr(vec![
        Value::Integer(0),
        Value::Integer(0),
        Value::Integer(20),
        Value::Integer(20),
    ]))
    .insert("FT", name("Btn"))
    .insert("AP", Value::Ref(40, 0))
    .insert("AS", name("On"));
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(widget_dict)])),
    ));
    insert(
        &doc,
        40,
        Value::Dict(Dict::new().insert("N", Value::Dict(states))),
    );
    let form = doc.acro_form().expect("parse").expect("present");
    let field = &form.fields[0];
    assert_eq!(field.appearance_state.as_deref(), Some("On"));
    assert_eq!(
        field.appearance_normal_states,
        vec![
            ("Off".to_string(), Value::Ref(30, 0)),
            ("On".to_string(), Value::Ref(31, 0)),
        ],
        "indirect /AP resolves and state names stay sorted"
    );

    let single = widget(arr(vec![
        Value::Integer(0),
        Value::Integer(0),
        Value::Integer(20),
        Value::Integer(20),
    ]))
    .insert("FT", name("Btn"))
    .insert(
        "AP",
        Value::Dict(Dict::new().insert("N", Value::Ref(30, 0))),
    );
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(single)])),
    ));
    let form = doc.acro_form().expect("parse").expect("present");
    assert!(form.fields[0].appearance_normal_states.is_empty());
}

#[test]
fn empty_partial_name_does_not_extend_qualified_name() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![Value::Dict(
            Dict::new().insert("T", text("group")).insert(
                "Kids",
                arr(vec![Value::Dict(Dict::new().insert("T", text("")))]),
            ),
        )]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(form.fields[0].kids[0].fully_qualified_name, "group");
}
