//! E4 读端表单测试。

use super::*;
use crate::core::types::{Array, Object, PdfTextString, Trailer};

const PAGES_OBJECT: i64 = 2;

fn name(value: &str) -> Value {
    Value::Name(value.into())
}

fn text(value: &str) -> Value {
    Value::HexString(
        PdfTextString::from(value)
            .encode()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect(),
    )
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn doc_with_catalog(catalog: Dict) -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        1,
        Value::Dict(
            catalog
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0)),
        ),
    );
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(Vec::new()))
                .insert("Count", Value::Integer(0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn doc_with_acro_form(acro: Value) -> Document {
    doc_with_catalog(Dict::new().insert("AcroForm", acro))
}

fn form_error(acro: Value) -> FormError {
    let doc = doc_with_acro_form(acro);
    doc.acro_form().expect_err("form should be rejected")
}

fn widget(rect: Value) -> Dict {
    Dict::new()
        .insert("Type", name("Annot"))
        .insert("Subtype", name("Widget"))
        .insert("Rect", rect)
}

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
    assert_eq!(form.default_resources, Some(Value::Dict(Dict::new())));
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
fn widget_node_keeps_appearance_state_and_page_reference() {
    let widget_dict = widget(arr(vec![
        Value::Integer(0),
        Value::Integer(0),
        Value::Integer(20),
        Value::Integer(20),
    ]))
    .insert("T", text("ack"))
    .insert("FT", name("Btn"))
    .insert("Ff", Value::Integer(1 << 16))
    .insert(
        "AP",
        Value::Dict(Dict::new().insert("N", Value::Ref(30, 0))),
    )
    .insert("AS", name("On"))
    .insert("P", Value::Ref(3, 0));
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(widget_dict)])),
    ));
    let form = doc.acro_form().expect("parse").expect("present");
    let field = &form.fields[0];
    assert!(field.is_widget);
    assert_eq!(field.field_type, Some(FieldType::Button));
    assert_eq!(field.appearance_state.as_deref(), Some("On"));
    assert_eq!(field.page, Some((3, 0)));
    assert_eq!(field.flags.bits(), 1 << 16);
    assert!(field.flags.pushbutton());
    assert_eq!(
        field
            .appearance
            .as_ref()
            .and_then(Value::as_dict)
            .and_then(|dict| dict.get("N")),
        Some(&Value::Ref(30, 0))
    );
}

#[test]
fn choice_button_and_signature_types() {
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
        "Fields",
        arr(vec![
            Value::Dict(
                Dict::new()
                    .insert("FT", name("Btn"))
                    .insert("Ff", Value::Integer((1 << 15) | (1 << 16))),
            ),
            Value::Dict(
                Dict::new()
                    .insert("FT", name("Ch"))
                    .insert("Opt", arr(vec![text("A"), text("B")]))
                    .insert("V", name("B"))
                    .insert("Ff", Value::Integer((1 << 17) | (1 << 21))),
            ),
            Value::Dict(Dict::new().insert("FT", name("Sig"))),
        ]),
    )));
    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(form.fields[0].field_type, Some(FieldType::Button));
    assert!(form.fields[0].flags.radio());
    assert!(form.fields[0].flags.pushbutton());
    assert_eq!(form.fields[1].field_type, Some(FieldType::Choice));
    assert_eq!(
        form.fields[1].options.as_ref().map(|options| options.len()),
        Some(2)
    );
    assert_eq!(form.fields[1].value, Some(name("B")));
    assert!(form.fields[1].flags.combo());
    assert!(form.fields[1].flags.multi_select());
    assert_eq!(form.fields[2].field_type, Some(FieldType::Signature));
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

#[test]
fn missing_acro_form_and_defaults() {
    let doc = doc_with_catalog(Dict::new());
    assert!(doc.acro_form().expect("parse").is_none());

    let doc = doc_with_acro_form(Value::Dict(Dict::new()));
    let form = doc.acro_form().expect("parse").expect("present");
    assert!(form.fields.is_empty());
    assert!(!form.need_appearances);
    assert!(form.default_appearance.is_none());
    assert!(form.default_resources.is_none());
    assert!(form.xfa.is_none());
}

#[test]
fn malformed_form_entries_are_typed_errors() {
    assert!(matches!(
        form_error(Value::Integer(7)),
        FormError::Malformed(_)
    ));
    let cases: Vec<Value> = vec![
        Value::Dict(Dict::new().insert("Fields", Value::Integer(1))),
        Value::Dict(Dict::new().insert("NeedAppearances", Value::Integer(1))),
        Value::Dict(Dict::new().insert("Fields", arr(vec![Value::Integer(5)]))),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("FT", Value::Integer(1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("T", Value::Integer(1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Ff", Value::Integer(-1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("MaxLen", Value::Integer(-2)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Q", Value::Float(1.5)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Opt", Value::Integer(1)),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Subtype", name("Widget")),
            )]),
        )),
        Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(
                Dict::new().insert("Kids", Value::Integer(1)),
            )]),
        )),
    ];
    for acro in cases {
        assert!(matches!(form_error(acro), FormError::Malformed(_)));
    }
}

#[test]
fn field_tree_cycles_and_depth_are_typed_errors() {
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Ref(21, 0)])),
    ));
    insert(
        &doc,
        21,
        Value::Dict(
            Dict::new()
                .insert("T", text("loop"))
                .insert("Kids", arr(vec![Value::Ref(21, 0)])),
        ),
    );
    assert!(matches!(
        doc.acro_form().expect_err("cycle"),
        FormError::Malformed(_)
    ));

    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Ref(100, 0)])),
    ));
    for index in 0..70i64 {
        let kids = if index < 69 {
            arr(vec![Value::Ref(101 + index, 0)])
        } else {
            arr(Vec::new())
        };
        insert(
            &doc,
            100 + index,
            Value::Dict(Dict::new().insert("T", text("n")).insert("Kids", kids)),
        );
    }
    assert_eq!(
        doc.acro_form().expect_err("depth"),
        FormError::LimitExceeded("form field depth")
    );
}

#[test]
fn form_node_limit_is_enforced() {
    let fields: Vec<Value> = (0..=MAX_FORM_NODES)
        .map(|_| Value::Dict(Dict::new()))
        .collect();
    let doc = doc_with_acro_form(Value::Dict(Dict::new().insert("Fields", arr(fields))));
    assert_eq!(
        doc.acro_form().expect_err("node limit"),
        FormError::LimitExceeded("form field count")
    );
}
