//! AcroForm `/DR` 非字体默认资源测试（Table 218 / 资源字典 Table 34）：
//! `/ExtGState` 直接参数字典、`/ColorSpace` 值构造与 `/ProcSet` 名称数组。

use super::*;
use crate::core::writer::WriteError;

fn catalog(doc: &Document) -> Dict {
    doc.get_object(1)
        .expect("catalog lookup")
        .expect("catalog exists")
        .value
        .as_dict()
        .expect("catalog is a dictionary")
        .clone()
}

fn default_resources(doc: &Document) -> Dict {
    catalog(doc)
        .get("AcroForm")
        .and_then(|value| value.as_dict())
        .and_then(|acro| acro.get("DR"))
        .and_then(|value| value.as_dict())
        .expect("/AcroForm /DR")
        .clone()
}

fn build_error(mut form: AcroForm) -> WriteError {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    form.add_field(page, FormField::text("name", [72.0, 700.0, 300.0, 720.0]));
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("invalid /DR must be rejected");
    assert!(out.is_empty(), "failed build must not write bytes");
    error
}

#[test]
fn default_resources_write_ext_gstate_color_space_and_proc_set() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new()
        .with_default_font("Helv", Font::helvetica(12.0))
        .with_default_ext_gstate(
            "GS1",
            GraphicsState::new()
                .with_stroke_alpha(0.5)
                .with_fill_alpha(0.25)
                .with_blend_mode("Multiply"),
        )
        .with_default_color_space("CS1", ColorSpace::DeviceRGB)
        .with_default_color_space(
            "CS2",
            ColorSpace::Indexed(Indexed::new(
                ColorSpace::DeviceRGB,
                1,
                vec![0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00],
            )),
        )
        .with_proc_set(vec!["PDF".into(), "Text".into()]);
    form.add_field(
        page,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let dr = default_resources(&doc);
    let fonts = dr
        .get("Font")
        .and_then(|value| value.as_dict())
        .expect("/DR /Font");
    let (font_num, _) = fonts
        .get("Helv")
        .and_then(|value| value.as_ref())
        .expect("/Helv reference");
    let font = doc
        .get_object(font_num)
        .expect("font lookup")
        .expect("font object exists")
        .value
        .as_dict()
        .expect("font dictionary")
        .clone();
    assert_eq!(font.get("BaseFont"), Some(&Value::Name("Helvetica".into())));

    let states = dr
        .get("ExtGState")
        .and_then(|value| value.as_dict())
        .expect("/DR /ExtGState");
    let gs = states
        .get("GS1")
        .and_then(|value| value.as_dict())
        .expect("/GS1");
    assert_eq!(gs.get("Type"), Some(&Value::Name("ExtGState".into())));
    assert_eq!(gs.get("CA"), Some(&Value::Float(0.5)));
    assert_eq!(gs.get("ca"), Some(&Value::Float(0.25)));
    assert_eq!(gs.get("BM"), Some(&Value::Name("Multiply".into())));

    let spaces = dr
        .get("ColorSpace")
        .and_then(|value| value.as_dict())
        .expect("/DR /ColorSpace");
    assert_eq!(spaces.get("CS1"), Some(&Value::Name("DeviceRGB".into())));
    let indexed = spaces
        .get("CS2")
        .and_then(|value| value.as_array())
        .expect("/CS2 array");
    assert_eq!(indexed.0[0], Value::Name("Indexed".into()));
    assert_eq!(indexed.0[1], Value::Name("DeviceRGB".into()));
    assert_eq!(indexed.0[2], Value::Integer(1));
    assert_eq!(indexed.0[3], Value::HexString("FF000000FF00".into()));

    let proc_set = dr
        .get("ProcSet")
        .and_then(|value| value.as_array())
        .expect("/DR /ProcSet");
    assert_eq!(
        proc_set.0,
        vec![Value::Name("PDF".into()), Value::Name("Text".into())]
    );
}

#[test]
fn default_ext_gstate_rejects_external_references() {
    let cases = [
        GraphicsState::new().with_font(Font::helvetica(12.0), 12.0),
        GraphicsState::new().with_transfer_function(0),
    ];
    for state in cases {
        let error = build_error(AcroForm::new().with_default_ext_gstate("GS1", state));
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "unexpected error: {error:?}"
        );
    }
}

#[test]
fn invalid_default_resource_names_and_duplicates_are_rejected() {
    let states = [
        AcroForm::new().with_default_ext_gstate("", GraphicsState::new()),
        AcroForm::new().with_default_ext_gstate("Bad Name", GraphicsState::new()),
        AcroForm::new().with_default_color_space("CS/1", ColorSpace::DeviceRGB),
        AcroForm::new()
            .with_default_ext_gstate("GS1", GraphicsState::new())
            .with_default_ext_gstate("GS1", GraphicsState::new()),
        AcroForm::new().with_proc_set(vec!["PDF".into(), "PDF".into()]),
        AcroForm::new().with_proc_set(vec!["PDF".into(), "Text PDF".into()]),
    ];
    for form in states {
        let error = build_error(form);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "unexpected error: {error:?}"
        );
    }
}

#[test]
fn default_color_space_values_are_validated() {
    let error = build_error(AcroForm::new().with_default_color_space(
        "CS1",
        ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceRGB, 1, vec![0; 5])),
    ));
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error: {error:?}"
    );
}
