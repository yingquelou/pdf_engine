//! E4 AcroForm：字段树、Widget、`/DR` 默认资源与外观流的写出→解析往返。

use super::*;

/// 写入 → 重新解析：AcroForm 字段树、Widget `/Annots`、`/Opt` 与 `/AP`
/// 外观流应经 `Document::acro_form` 完整回读（12.7.2–12.7.4）。
#[test]
fn builder_acro_form_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AcroForm, FormField, FormOption, FormXObject, PageSize, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::reader::form::FieldType;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let mut appearance_content = ContentStream::new();
    appearance_content
        .set_fill_gray(0.0)
        .rect(0.0, 0.0, 10.0, 10.0)
        .fill();
    let appearance = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 10.0, 10.0],
        appearance_content,
    ));

    let mut form = AcroForm::new()
        .with_need_appearances(true)
        .with_default_appearance("/Helv 0 Tf 0 g");
    let person = FormField::group("person")
        .with_child(FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"))
        .with_child(FormField::text("email", [72.0, 670.0, 300.0, 690.0]).with_value("a@b.c"));
    form.add_field(p0, person);
    form.add_field(
        p0,
        FormField::choice("color", [72.0, 620.0, 200.0, 640.0])
            .with_flags(FormField::COMBO)
            .with_value("R")
            .with_option(FormOption::new("R").with_display("Red"))
            .with_option(FormOption::new("G")),
    );
    form.add_field(
        p0,
        FormField::checkbox("subscribe", [72.0, 590.0, 92.0, 610.0])
            .with_appearance(appearance)
            .with_appearance_state("Off"),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let form = doc.acro_form().expect("parse form").expect("form present");
    assert!(form.need_appearances);
    assert!(form.default_appearance.is_some(), "/DA should be present");
    assert_eq!(form.fields.len(), 3);

    let person = &form.fields[0];
    assert_eq!(person.fully_qualified_name, "person");
    assert_eq!(person.kids.len(), 2);
    assert_eq!(person.kids[0].fully_qualified_name, "person.name");
    assert_eq!(person.kids[1].fully_qualified_name, "person.email");
    assert!(person.kids.iter().all(|kid| kid.is_widget));

    let choice = &form.fields[1];
    assert_eq!(choice.field_type, Some(FieldType::Choice));
    assert!(choice.flags.combo());
    assert_eq!(choice.options.as_ref().map(Vec::len), Some(2));

    let checkbox = &form.fields[2];
    assert_eq!(checkbox.field_type, Some(FieldType::Button));
    assert_eq!(checkbox.appearance_state.as_deref(), Some("Off"));
    let appearance = checkbox
        .appearance
        .as_ref()
        .and_then(|value| value.as_dict())
        .expect("/AP dictionary");
    let (form_num, _) = appearance
        .get("N")
        .and_then(|value| value.as_ref())
        .expect("/AP /N");
    let object = doc
        .get_object(form_num)
        .expect("form object")
        .expect("exists");
    match object.value {
        Value::Stream(ref dict, _) => {
            assert_eq!(dict.get("Subtype"), Some(&Value::Name("Form".into())));
        }
        other => panic!("expected Form XObject, got {other:?}"),
    }

    let widgets = doc.page_annotations(0).expect("page annotations");
    assert_eq!(
        widgets.len(),
        4,
        "two child widgets, the choice field and the checkbox"
    );
    assert!(
        widgets
            .iter()
            .all(|annot| annot.subtype == AnnotationSubtype::Widget)
    );
}

/// 写入 → 重新解析：`/AcroForm /DR /Font` 默认资源应回读为字体间接引用，
/// 且与页面 `/Resources` 中的同名字体共用同一对象（12.7.2 Table 218）。
#[test]
fn builder_acro_form_default_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AcroForm, Font, FormField, PageSize, PdfBuilder};
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let font = Font::helvetica(12.0);
    builder.text(page, &font, 72.0, 750.0, "Form");
    let mut form = AcroForm::new()
        .with_default_appearance("/Helv 12 Tf 0 g")
        .with_default_font("Helv", font.clone());
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let parsed = doc.acro_form().expect("parse form").expect("form present");
    assert_eq!(parsed.fields.len(), 1, "one top-level field");

    let dr = parsed
        .default_resources
        .as_ref()
        .and_then(|value| value.as_dict())
        .expect("/DR dictionary")
        .clone();
    let fonts = dr
        .get("Font")
        .and_then(|value| value.as_dict())
        .expect("/DR /Font dictionary")
        .clone();
    assert_eq!(fonts.0.len(), 1, "one default font resource");
    let (font_num, _) = fonts
        .get("Helv")
        .and_then(|value| value.as_ref())
        .expect("/Helv is an indirect reference");
    let font_dict = doc
        .get_object(font_num)
        .expect("font object")
        .expect("font exists")
        .value
        .as_dict()
        .expect("font dictionary")
        .clone();
    assert_eq!(
        font_dict.get("BaseFont"),
        Some(&Value::Name("Helvetica".into())),
        "default resource resolves to a real font"
    );

    let mut helvetica_objects = Vec::new();
    for num in 1..30 {
        if let Some(object) = doc.get_object(num).expect("probe object")
            && let Some(dict) = object.value.as_dict()
            && dict.get("Type").and_then(|value| value.as_name()) == Some("Font")
            && dict.get("BaseFont").and_then(|value| value.as_name()) == Some("Helvetica")
        {
            helvetica_objects.push(num);
        }
    }
    assert_eq!(
        helvetica_objects,
        vec![font_num],
        "page resources and /DR share one font object"
    );
    assert_eq!(
        doc.extract_page_text(0).expect("page text").trim(),
        "Form",
        "page text still extracts after adding default resources"
    );
}

/// 写入 → 重新解析：字段 `/AA` 触发动作（Table 196）与 `/CO` 计算顺序
/// （Table 218）应完整回读，显式名录保持声明顺序。
#[test]
fn builder_form_actions_and_calculation_order_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AcroForm, Action, FieldActionEvent, FormField, PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::action::Action as ReadAction;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new().with_calculation_order(vec!["tax".into(), "total".into()]);
    form.add_field(
        page,
        FormField::text("total", [72.0, 700.0, 300.0, 720.0])
            .with_action(
                FieldActionEvent::Calculate,
                Action::JavaScript("calculateTotal()".into()),
            )
            .with_action(
                FieldActionEvent::Format,
                Action::JavaScript("formatTotal()".into()),
            ),
    );
    form.add_field(
        page,
        FormField::text("tax", [72.0, 670.0, 300.0, 690.0]).with_action(
            FieldActionEvent::Calculate,
            Action::JavaScript("calculateTax()".into()),
        ),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let parsed = doc.acro_form().expect("parse form").expect("form present");
    assert_eq!(
        parsed.calculation_order,
        ["tax", "total"],
        "explicit /CO order survives the roundtrip"
    );

    let total = &parsed.fields[0];
    let actions = total.actions.as_ref().expect("/AA present");
    assert_eq!(
        actions.calculate,
        Some(ReadAction::JavaScript("calculateTotal()".into()))
    );
    assert_eq!(
        actions.format,
        Some(ReadAction::JavaScript("formatTotal()".into()))
    );
    assert!(actions.keystroke.is_none());

    let tax = &parsed.fields[1];
    let tax_actions = tax.actions.as_ref().expect("/AA present");
    assert_eq!(
        tax_actions.calculate,
        Some(ReadAction::JavaScript("calculateTax()".into()))
    );
}
