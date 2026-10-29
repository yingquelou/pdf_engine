//! E4 读端表单测试。

mod choice;
mod flags;
mod options;

use super::*;
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::graphics_state::BlendMode;
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

fn javascript(script: &str) -> Value {
    Value::Dict(
        Dict::new()
            .insert("S", name("JavaScript"))
            .insert("JS", text(script)),
    )
}

#[test]
fn field_aa_actions_are_typed_and_other_events_preserved() {
    let field = Dict::new()
        .insert("T", text("amount"))
        .insert("FT", name("Tx"))
        .insert(
            "AA",
            Value::Dict(
                Dict::new()
                    .insert("K", javascript("keystroke()"))
                    .insert("F", javascript("format()"))
                    .insert("V", javascript("validate()"))
                    .insert("C", javascript("calculate()"))
                    .insert("X", javascript("custom()")),
            ),
        );
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new().insert("Fields", arr(vec![Value::Dict(field)])),
    ));
    let form = doc.acro_form().expect("parse").expect("present");
    let actions = form.fields[0].actions.as_ref().expect("/AA");
    assert_eq!(
        actions.keystroke,
        Some(Action::JavaScript("keystroke()".into()))
    );
    assert_eq!(actions.format, Some(Action::JavaScript("format()".into())));
    assert_eq!(
        actions.validate,
        Some(Action::JavaScript("validate()".into()))
    );
    assert_eq!(
        actions.calculate,
        Some(Action::JavaScript("calculate()".into()))
    );
    assert_eq!(actions.other.len(), 1);
    assert_eq!(actions.other[0].0, "X");
    assert!(!actions.is_empty());
}

#[test]
fn malformed_field_actions_are_typed_errors() {
    let cases = [
        Value::Integer(1),
        Value::Dict(Dict::new().insert("K", Value::Dict(Dict::new()))),
    ];
    for aa in cases {
        let doc = doc_with_acro_form(Value::Dict(Dict::new().insert(
            "Fields",
            arr(vec![Value::Dict(Dict::new().insert("AA", aa))]),
        )));
        assert!(
            matches!(
                doc.acro_form().expect_err("malformed /AA"),
                FormError::Malformed(_)
            ),
            "malformed /AA must be rejected"
        );
    }
}

#[test]
fn calculation_order_resolves_field_references_to_qualified_names() {
    let doc = doc_with_acro_form(Value::Ref(20, 0));
    insert(
        &doc,
        20,
        Value::Dict(
            Dict::new()
                .insert("Fields", arr(vec![Value::Ref(21, 0)]))
                .insert("CO", arr(vec![Value::Ref(23, 0), Value::Ref(21, 0)])),
        ),
    );
    insert(
        &doc,
        21,
        Value::Dict(
            Dict::new()
                .insert("T", text("group"))
                .insert("Kids", arr(vec![Value::Ref(23, 0)])),
        ),
    );
    insert(
        &doc,
        23,
        Value::Dict(Dict::new().insert("T", text("total"))),
    );

    let form = doc.acro_form().expect("parse").expect("present");
    assert_eq!(form.calculation_order, ["group.total", "group"]);
}

#[test]
fn malformed_calculation_order_is_typed_error() {
    let cases = [
        Value::Integer(1),
        arr(vec![Value::Dict(Dict::new())]),
        arr(vec![Value::Ref(99, 0)]),
    ];
    for co in cases {
        let doc = doc_with_acro_form(Value::Dict(
            Dict::new()
                .insert("Fields", arr(Vec::new()))
                .insert("CO", co),
        ));
        assert!(
            matches!(
                doc.acro_form().expect_err("malformed /CO"),
                FormError::Malformed(_)
            ),
            "malformed /CO must be rejected"
        );
    }
}

#[test]
fn acro_form_default_resources_are_typed() {
    let doc = doc_with_acro_form(Value::Dict(
        Dict::new()
            .insert("Fields", arr(Vec::new()))
            .insert("DR", Value::Ref(30, 0)),
    ));
    insert(
        &doc,
        30,
        Value::Dict(
            Dict::new()
                .insert(
                    "Font",
                    Value::Dict(Dict::new().insert("Helv", Value::Ref(31, 0))),
                )
                .insert(
                    "ExtGState",
                    Value::Dict(
                        Dict::new().insert(
                            "GS1",
                            Value::Dict(
                                Dict::new()
                                    .insert("Type", name("ExtGState"))
                                    .insert("CA", Value::Float(0.5))
                                    .insert("ca", Value::Float(0.25))
                                    .insert("BM", name("Multiply")),
                            ),
                        ),
                    ),
                )
                .insert(
                    "ColorSpace",
                    Value::Dict(Dict::new().insert("CS1", name("DeviceRGB")).insert(
                        "CS2",
                        arr(vec![
                            name("Indexed"),
                            name("DeviceRGB"),
                            Value::Integer(1),
                            Value::HexString("000000FFFFFF".into()),
                        ]),
                    )),
                )
                .insert("ProcSet", arr(vec![name("PDF"), name("Text")]))
                .insert("Pattern", Value::Dict(Dict::new())),
        ),
    );
    insert(
        &doc,
        31,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Font"))
                .insert("Subtype", name("Type1"))
                .insert("BaseFont", name("Helvetica")),
        ),
    );

    let form = doc.acro_form().expect("parse").expect("present");
    let dr = form.default_resources.as_ref().expect("/DR present");
    assert_eq!(dr.fonts.get("Helv"), Some(&Value::Ref(31, 0)));
    let gs = dr.ext_g_states.get("GS1").expect("/DR /ExtGState /GS1");
    assert_eq!(gs.stroke_alpha, Some(0.5));
    assert_eq!(gs.fill_alpha, Some(0.25));
    assert_eq!(gs.blend_mode, Some(BlendMode::Name("Multiply".into())));
    assert_eq!(
        dr.color_spaces.get("CS1"),
        Some(&ColorSpaceModel::DeviceRGB)
    );
    let Some(ColorSpaceModel::Indexed(indexed)) = dr.color_spaces.get("CS2") else {
        panic!("/CS2 must be Indexed");
    };
    assert_eq!(*indexed.base, ColorSpaceModel::DeviceRGB);
    assert_eq!(indexed.hival, 1);
    assert_eq!(indexed.lookup, vec![0, 0, 0, 0xFF, 0xFF, 0xFF]);
    assert_eq!(dr.proc_set, ["PDF", "Text"]);
    // 未建模条目保留在完整字典中。
    assert_eq!(dr.dict.get("Pattern"), Some(&Value::Dict(Dict::new())));
}

#[test]
fn malformed_default_resources_are_typed_errors() {
    let cases = [
        // /DR 不是字典。
        Value::Integer(1),
        // /Font 不是字典。
        Value::Dict(Dict::new().insert("Font", name("Helv"))),
        // /ExtGState 条目取值非法（/CA 越界）。
        Value::Dict(Dict::new().insert(
            "ExtGState",
            Value::Dict(Dict::new().insert(
                "GS1",
                Value::Dict(Dict::new().insert("CA", Value::Float(2.0))),
            )),
        )),
        // /ColorSpace 名字不认识。
        Value::Dict(Dict::new().insert(
            "ColorSpace",
            Value::Dict(Dict::new().insert("CS1", name("Bogus"))),
        )),
        // /ProcSet 条目不是名字。
        Value::Dict(Dict::new().insert("ProcSet", arr(vec![Value::Integer(1)]))),
    ];
    for dr in cases {
        let error = form_error(Value::Dict(
            Dict::new()
                .insert("Fields", arr(Vec::new()))
                .insert("DR", dr),
        ));
        assert!(matches!(error, FormError::Malformed(_)), "{error}");
    }
}
