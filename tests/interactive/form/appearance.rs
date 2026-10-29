//! AcroForm Widget 外观状态、`/DR` 非字体默认资源与 `/Ff` 标志的写出→解析往返。

use super::*;

/// 写入 → 重新解析：Widget `/AP /N` 状态子字典与 `/AS` 应经
/// `Document::acro_form` 回读为类型化状态列表（12.5.5 Table 168）。
#[test]
fn builder_form_appearance_states_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AcroForm, FormField, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::ContentStream;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let off = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 20.0, 20.0],
        ContentStream::new(),
    ));
    let mut on_content = ContentStream::new();
    on_content
        .set_fill_gray(0.0)
        .rect(2.0, 2.0, 16.0, 16.0)
        .fill();
    let on = builder.form_xobject(&FormXObject::new([0.0, 0.0, 20.0, 20.0], on_content));

    let mut form = AcroForm::new();
    form.add_field(
        p0,
        FormField::radio("agree", [72.0, 700.0, 92.0, 720.0])
            .with_appearance_state("Off", off)
            .with_appearance_state("Yes", on)
            .with_selected_appearance_state("Yes"),
    );
    builder.set_acro_form(form);
    let mut bytes = Vec::new();
    builder.build(&mut bytes).expect("build");

    let doc = parse_buf(&bytes);
    let form = doc.acro_form().expect("parse").expect("form present");
    let field = &form.fields[0];
    assert_eq!(field.appearance_state.as_deref(), Some("Yes"));
    let normal = field
        .appearance
        .as_ref()
        .and_then(|value| value.as_dict())
        .and_then(|ap| ap.get("N"))
        .and_then(|value| value.as_dict())
        .expect("/AP /N state dictionary");
    let written_off = normal.get("Off").expect("Off state entry");
    let written_on = normal.get("Yes").expect("Yes state entry");
    assert_ne!(written_off, written_on, "states reference distinct forms");
    let states: Vec<(&str, &Value)> = field
        .appearance_normal_states
        .iter()
        .map(|(state, value)| (state.as_str(), value))
        .collect();
    assert_eq!(
        states,
        vec![("Off", written_off), ("Yes", written_on)],
        "state names are sorted and references preserved"
    );
    for (state, value) in &field.appearance_normal_states {
        let (num, _) = value
            .as_ref()
            .unwrap_or_else(|| panic!("{state} state entry is a reference"));
        let object = doc.get_object(num).expect("lookup").expect("exists");
        assert!(matches!(object.value, Value::Stream(ref dict, _)
            if dict.get("Subtype") == Some(&Value::Name("Form".into()))));
    }
}

/// `/DR` 非字体默认资源（`/ExtGState`、`/ColorSpace`、`/ProcSet`）经写入→
/// 解析后由 `AcroForm::default_resources` 原样回读，字体引用可解析。
#[test]
fn builder_acro_form_default_nonfont_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AcroForm, ColorSpace, Font, FormField, GraphicsState, Indexed, PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::color_space::ColorSpaceModel;
    use pdf_engine::core::reader::graphics_state::BlendMode;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
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
                vec![0, 0, 0, 0xFF, 0xFF, 0xFF],
            )),
        )
        .with_proc_set(vec!["PDF".into(), "Text".into()]);
    form.add_field(
        p0,
        FormField::text("name", [72.0, 700.0, 300.0, 720.0]).with_value("Alice"),
    );
    builder.set_acro_form(form);
    let mut bytes = Vec::new();
    builder.build(&mut bytes).expect("build");

    let doc = parse_buf(&bytes);
    let parsed = doc.acro_form().expect("parse").expect("form present");
    let dr = parsed.default_resources.as_ref().expect("/DR");
    let (font_num, _) = dr
        .fonts
        .get("Helv")
        .and_then(|value| value.as_ref())
        .expect("/DR /Font /Helv");
    let font = doc
        .get_object(font_num)
        .expect("font lookup")
        .expect("font object exists");
    assert!(matches!(&font.value, Value::Dict(dict)
        if dict.get("BaseFont") == Some(&Value::Name("Helvetica".into()))));

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
}

/// 写端 `FILE_SELECT`/`DO_NOT_SCROLL`/`COMB`/`RICH_TEXT` 标志经 `/Ff`
/// 写出后应被读端按 Table 228 解码（12.7.4.3）。
#[test]
fn builder_text_field_flags_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AcroForm, FormField, PageSize, PdfBuilder};
    use pdf_engine::core::reader::form::FieldType;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("path", [72.0, 700.0, 300.0, 720.0]).with_flags(
            FormField::FILE_SELECT
                | FormField::DO_NOT_SCROLL
                | FormField::COMB
                | FormField::RICH_TEXT,
        ),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let form = doc.acro_form().expect("parse form").expect("form present");
    let field = &form.fields[0];
    assert_eq!(field.field_type, Some(FieldType::Text));
    assert!(field.flags.file_select());
    assert!(field.flags.do_not_scroll());
    assert!(field.flags.comb());
    assert!(field.flags.rich_text());
    assert!(!field.flags.multiline());
    assert!(!field.flags.multi_select());
}
