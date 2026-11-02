//! 字段与 Widget 写出：目录 `/AcroForm` 条目、字段树 `/Kids`/`/Parent` 与
//! `/T` 全限定名、Widget 合并字典的 `/FT`/`/V`/`/MaxLen`/`/Q`/`/F`/`/P` 条目
//! 与页面 `/Annots` 注册、选择/按钮字段的 `/Opt` 与旗标、`/P` 覆盖页绑定，
//! 以及写后读端 `doc.acro_form()` 的类型化回读。

use super::super::*;
use super::support::{assert_text_entry, catalog, object_dict};
use crate::core::reader::form::FieldType;

fn page_annots(doc: &Document, page_num: i64) -> Vec<(i64, i64)> {
    object_dict(doc, page_num)
        .get("Annots")
        .and_then(|value| value.as_array())
        .map(|array| {
            array
                .0
                .iter()
                .map(|value| value.as_ref().expect("/Annots entries are indirect"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn text_field_is_written_as_widget_in_acro_form_and_page_annots() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new()
        .with_need_appearances(true)
        .with_default_appearance("/Helv 0 Tf 0 g");
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0])
            .with_value("Alice")
            .with_max_len(40)
            .with_quadding(1)
            .with_tooltip("Full name")
            .with_annotation_flags(4),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let acro = catalog(&doc)
        .get("AcroForm")
        .and_then(|value| value.as_dict())
        .expect("/AcroForm")
        .clone();
    assert_eq!(acro.get("NeedAppearances"), Some(&Value::Bool(true)));
    assert_text_entry(&acro, "DA");
    let fields = acro
        .get("Fields")
        .and_then(|value| value.as_array())
        .expect("/Fields");
    assert_eq!(fields.0.len(), 1);
    let (field_num, _) = fields.0[0].as_ref().expect("field reference");
    let field = object_dict(&doc, field_num);
    assert_eq!(field.get("Type"), Some(&Value::Name("Annot".into())));
    assert_eq!(field.get("Subtype"), Some(&Value::Name("Widget".into())));
    assert_text_entry(&field, "T");
    assert_eq!(field.get("FT"), Some(&Value::Name("Tx".into())));
    assert_text_entry(&field, "V");
    assert_eq!(field.get("MaxLen"), Some(&Value::Integer(40)));
    assert_eq!(field.get("Q"), Some(&Value::Integer(1)));
    assert_text_entry(&field, "TU");
    assert_eq!(field.get("F"), Some(&Value::Integer(4)));
    let (page_ref, _) = field.get("P").and_then(|value| value.as_ref()).expect("/P");
    assert_eq!(page_ref, 3, "/P must reference the first Page object");
    assert_eq!(page_annots(&doc, 3), vec![(field_num, 0)]);

    let parsed = doc.acro_form().expect("read").expect("form present");
    assert!(parsed.need_appearances);
    assert_eq!(parsed.fields.len(), 1);
    let parsed_field = &parsed.fields[0];
    assert_eq!(parsed_field.fully_qualified_name, "name");
    assert_eq!(parsed_field.field_type, Some(FieldType::Text));
    assert!(parsed_field.is_widget);
    assert_eq!(parsed_field.rect, Some([72.0, 700.0, 300.0, 720.0]));
    assert_eq!(parsed_field.max_len, Some(40));
    assert_eq!(parsed_field.quadding, Some(1));
    assert!(matches!(parsed_field.value, Some(Value::HexString(_))));
}

#[test]
fn field_tree_writes_parent_kids_and_fully_qualified_names() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let person = FormField::group("person")
        .with_child(FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"))
        .with_child(FormField::text("email", [72.0, 670.0, 300.0, 690.0]));
    let mut form = AcroForm::new();
    form.add_field(page, person);
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let parsed = doc.acro_form().expect("read").expect("form present");
    assert_eq!(parsed.fields.len(), 1);
    let person = &parsed.fields[0];
    assert_eq!(person.fully_qualified_name, "person");
    assert!(!person.is_widget);
    assert_eq!(person.kids.len(), 2);
    assert_eq!(person.kids[0].fully_qualified_name, "person.name");
    assert_eq!(person.kids[1].fully_qualified_name, "person.email");
    assert!(person.kids[0].is_widget);

    let (parent_num, _) = person.reference.expect("parent reference");
    let parent = object_dict(&doc, parent_num);
    assert_eq!(
        parent
            .get("Kids")
            .and_then(|value| value.as_array())
            .unwrap()
            .0
            .len(),
        2
    );
    let (child_num, _) = person.kids[0].reference.expect("child reference");
    let child = object_dict(&doc, child_num);
    assert_eq!(child.get("Parent"), Some(&Value::Ref(parent_num, 0)));
    assert_eq!(page_annots(&doc, 3).len(), 2);
}

#[test]
fn choice_and_button_fields_write_options_and_flags() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::choice("color", [72.0, 620.0, 200.0, 640.0])
            .with_flags(FormField::COMBO)
            .with_value("R")
            .with_option(FormOption::new("R").with_display("Red"))
            .with_option(FormOption::new("G")),
    );
    form.add_field(
        page,
        FormField::radio("agree", [72.0, 590.0, 92.0, 610.0]).with_selected_appearance_state("Off"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let parsed = doc.acro_form().expect("read").expect("form present");
    assert_eq!(parsed.fields.len(), 2);
    let choice = &parsed.fields[0];
    assert_eq!(choice.field_type, Some(FieldType::Choice));
    assert!(choice.flags.combo());
    let options = choice.options.as_ref().expect("/Opt");
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].export(), "R");
    assert_eq!(options[0].display(), Some("Red"));
    assert_eq!(options[1].export(), "G");
    assert_eq!(options[1].display(), None);
    let radio = &parsed.fields[1];
    assert_eq!(radio.field_type, Some(FieldType::Button));
    assert!(radio.flags.radio());
    assert_eq!(radio.appearance_state.as_deref(), Some("Off"));
}

#[test]
fn widget_page_override_binds_and_registers_on_that_page() {
    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(0, FormField::text("first", [72.0, 700.0, 200.0, 720.0]));
    form.add_field(
        0,
        FormField::text("second", [72.0, 700.0, 200.0, 720.0]).with_page(1),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let parsed = doc.acro_form().expect("read").expect("form present");
    assert_eq!(parsed.fields[0].page, Some((3, 0)));
    assert_eq!(parsed.fields[1].page, Some((5, 0)));
    assert_eq!(page_annots(&doc, 3).len(), 1);
    assert_eq!(page_annots(&doc, 5).len(), 1);
}
