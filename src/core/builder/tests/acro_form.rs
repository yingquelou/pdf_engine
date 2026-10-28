//! AcroForm 写端测试（ISO 32000-1 12.7.2–12.7.4）：目录 `/AcroForm`、
//! 字段树 `/Kids`/`/Parent`、Widget 合并字典与 `/Annots`、`/Opt` 与
//! `/AP` 外观流引用。

use super::*;
use crate::core::content::ContentStream;
use crate::core::reader::form::FieldType;
use crate::core::types::PdfVersion;
use crate::core::writer::WriteError;

fn assert_text_entry(dict: &Dict, key: &str) {
    match dict.get(key) {
        Some(Value::HexString(_)) | Some(Value::String(_)) => {}
        other => panic!("expected text string at {key}, got {other:?}"),
    }
}

fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object lookup")
        .expect("object exists")
        .value
        .as_dict()
        .expect("object is a dictionary")
        .clone()
}

fn catalog(doc: &Document) -> Dict {
    object_dict(doc, 1)
}

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

/// 构造单页文档并注册一个字段树；返回组装后的文档。
fn build_error(field: FormField, page: usize) -> WriteError {
    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, field);
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("invalid form must be rejected");
    assert!(out.is_empty(), "failed build must not write bytes");
    error
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
        FormField::radio("agree", [72.0, 590.0, 92.0, 610.0]).with_appearance_state("Off"),
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
    let pair = options[0].as_array().expect("export/display pair");
    assert_eq!(pair.0.len(), 2);
    assert!(matches!(options[1], Value::HexString(_) | Value::String(_)));
    let radio = &parsed.fields[1];
    assert_eq!(radio.field_type, Some(FieldType::Button));
    assert!(radio.flags.radio());
    assert_eq!(radio.appearance_state.as_deref(), Some("Off"));
}

#[test]
fn widget_appearance_references_pool_form_streams() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let handle = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::checkbox("subscribe", [72.0, 560.0, 92.0, 580.0])
            .with_appearance(handle)
            .with_appearance_down(handle)
            .with_appearance_state("Yes"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let parsed = doc.acro_form().expect("read").expect("form present");
    let field = &parsed.fields[0];
    let appearance = field
        .appearance
        .as_ref()
        .and_then(|value| value.as_dict())
        .expect("/AP dictionary");
    let (normal_num, _) = appearance
        .get("N")
        .and_then(|value| value.as_ref())
        .expect("/N");
    let (down_num, _) = appearance
        .get("D")
        .and_then(|value| value.as_ref())
        .expect("/D");
    assert_eq!(normal_num, down_num, "same form handle is shared");
    assert_eq!(field.appearance_state.as_deref(), Some("Yes"));
    let object = doc.get_object(normal_num).expect("lookup").expect("exists");
    match object.value {
        Value::Stream(ref dict, _) => {
            assert_eq!(dict.get("Type"), Some(&Value::Name("XObject".into())));
            assert_eq!(dict.get("Subtype"), Some(&Value::Name("Form".into())));
        }
        other => panic!("expected Form XObject stream, got {other:?}"),
    }
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

#[test]
fn acro_form_requires_pdf_1_2() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_1);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, FormField::text("name", [72.0, 700.0, 200.0, 720.0]));
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder.build(&mut out).expect_err("1.1 rejects forms");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error: {error}"
    );
    assert!(out.is_empty());
}

#[test]
fn invalid_form_trees_are_rejected_without_output() {
    let rect = [72.0, 700.0, 200.0, 720.0];
    let cases = [
        (
            "widget with children",
            FormField::text("x", rect).with_child(FormField::text("y", [0.0, 0.0, 10.0, 10.0])),
            0,
        ),
        ("leaf without /FT", FormField::widget(rect), 0),
        (
            "quadding out of range",
            FormField::text("x", rect).with_quadding(3),
            0,
        ),
        (
            "max length on choice",
            FormField::choice("x", rect).with_max_len(3),
            0,
        ),
        (
            "options on text",
            FormField::text("x", rect).with_option(FormOption::new("A")),
            0,
        ),
        ("unknown page", FormField::text("x", rect), 7),
    ];
    for (label, field, page) in cases {
        let error = build_error(field, page);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{label}: unexpected error {error}"
        );
    }
}

#[test]
fn widget_appearance_handle_out_of_range_is_rejected() {
    let error = build_error(
        FormField::checkbox("x", [0.0, 0.0, 10.0, 10.0]).with_appearance(9),
        0,
    );
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error: {error}"
    );
}

/// 表单级校验失败的辅助：仅声明 AcroForm 选项，字段树保持合法。
fn form_build_error(form: AcroForm) -> WriteError {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = form;
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("invalid form options must be rejected");
    assert!(out.is_empty(), "failed build must not write bytes");
    error
}

fn acro_dict(doc: &Document) -> Dict {
    catalog(doc)
        .get("AcroForm")
        .and_then(|value| value.as_dict())
        .expect("/AcroForm")
        .clone()
}

#[test]
fn acro_form_default_resources_write_font_references() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new()
        .with_default_appearance("/Helv 12 Tf 0 g")
        .with_default_font("Helv", Font::helvetica(12.0));
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let dr = acro_dict(&doc)
        .get("DR")
        .and_then(|value| value.as_dict())
        .expect("/DR")
        .clone();
    let fonts = dr
        .get("Font")
        .and_then(|value| value.as_dict())
        .expect("/DR /Font")
        .clone();
    assert_eq!(fonts.0.len(), 1, "仅声明一个默认字体资源");
    let (font_num, _) = fonts
        .get("Helv")
        .and_then(|value| value.as_ref())
        .expect("/Helv 是间接引用");
    let font_dict = object_dict(&doc, font_num);
    assert_eq!(font_dict.get("Type"), Some(&Value::Name("Font".into())));
    assert_eq!(font_dict.get("Subtype"), Some(&Value::Name("Type1".into())));
    assert_eq!(
        font_dict.get("BaseFont"),
        Some(&Value::Name("Helvetica".into()))
    );
}

#[test]
fn acro_form_default_font_shares_object_with_page_font() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let font = Font::helvetica(24.0);
    builder.text(page, &font, 72.0, 700.0, "Hello");
    let mut form = AcroForm::new().with_default_font("Helv", Font::helvetica(24.0));
    form.add_field(
        page,
        FormField::text("name", [72.0, 600.0, 300.0, 620.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let dr_fonts = acro_dict(&doc)
        .get("DR")
        .and_then(|value| value.as_dict())
        .and_then(|dr| dr.get("Font"))
        .and_then(|value| value.as_dict())
        .expect("/DR /Font")
        .clone();
    let (dr_num, _) = dr_fonts
        .get("Helv")
        .and_then(|value| value.as_ref())
        .expect("DR font reference");
    let page_fonts = object_dict(&doc, 3)
        .get("Resources")
        .and_then(|value| value.as_dict())
        .and_then(|res| res.get("Font"))
        .and_then(|value| value.as_dict())
        .expect("page /Resources /Font")
        .clone();
    let page_nums: Vec<i64> = page_fonts
        .0
        .values()
        .map(|value| value.as_ref().expect("page font reference").0)
        .collect();
    assert!(
        page_nums.contains(&dr_num),
        "/DR 字体应与页面字体共享对象号：page={page_nums:?}, dr={dr_num}"
    );
}

#[test]
fn acro_form_rejects_duplicate_default_font_names() {
    let form = AcroForm::new()
        .with_default_font("Helv", Font::helvetica(12.0))
        .with_default_font("Helv", Font::helvetica(14.0));
    let error = form_build_error(form);
    assert!(
        matches!(&error, WriteError::InvalidObject(message) if message.contains("duplicate")),
        "unexpected error: {error:?}"
    );
}

#[test]
fn acro_form_rejects_invalid_default_font_names() {
    for name in ["", "He/lv", "He lv", "He[lv]"] {
        let form = AcroForm::new().with_default_font(name, Font::helvetica(12.0));
        let error = form_build_error(form);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "name {name:?} must be rejected, got {error:?}"
        );
    }
}
