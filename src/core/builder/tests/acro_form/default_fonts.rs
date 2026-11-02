//! AcroForm `/DR` 默认字体资源：`/AcroForm /DR /Font` 写出类型化字体字典引用、
//! 与页面 `/Resources /Font` 共享同一间接对象号（同名同字体去重）。
//! 非字体默认资源见 `tests/acro_form_resources.rs`。

use super::super::*;
use super::support::{acro_dict, object_dict};

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
