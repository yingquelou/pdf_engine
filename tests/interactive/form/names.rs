//! 字段命名与选项：同全限定名多重表示（12.7.3.2）、按钮 `/Opt`（Table 227）
//! 与选择字段 `/TI`、`/I`（Table 231）的写出→解析往返。

use super::*;

/// Table 227：单选组 `/Opt` 按 Widget 顺序写出单一文本串，开状态名
/// 位置化（`/0`、`/1`），序列化 → 解析后应类型化回读。
#[test]
fn builder_radio_group_options_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AcroForm, FormField, FormOption, FormXObject, PageSize, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::form::FieldType;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let form_handle = |builder: &mut PdfBuilder| {
        builder.form_xobject(&FormXObject::new(
            [0.0, 0.0, 20.0, 20.0],
            ContentStream::new(),
        ))
    };
    let off = form_handle(&mut builder);
    let first = form_handle(&mut builder);
    let second = form_handle(&mut builder);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::radio_group("agree")
            .with_option(FormOption::new("cardbrand1"))
            .with_option(FormOption::new("cardbrand2"))
            .with_child(
                FormField::widget([72.0, 560.0, 92.0, 580.0])
                    .with_appearance_state("Off", off)
                    .with_appearance_state("0", first)
                    .with_selected_appearance_state("0"),
            )
            .with_child(
                FormField::widget([100.0, 560.0, 120.0, 580.0])
                    .with_appearance_state("Off", off)
                    .with_appearance_state("1", second)
                    .with_selected_appearance_state("Off"),
            ),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let form = doc.acro_form().expect("parse form").expect("form present");
    let group = &form.fields[0];
    assert_eq!(group.field_type, Some(FieldType::Button));
    assert!(group.flags.radio());
    let options = group.options.as_ref().expect("/Opt");
    assert_eq!(
        options
            .iter()
            .map(|option| (option.export(), option.display()))
            .collect::<Vec<_>>(),
        vec![("cardbrand1", None), ("cardbrand2", None)]
    );
    assert_eq!(group.kids.len(), 2);
    assert_eq!(group.kids[0].appearance_state.as_deref(), Some("0"));
    assert_eq!(group.kids[1].appearance_state.as_deref(), Some("Off"));
    let states = |kid: &pdf_engine::core::reader::form::Field| {
        kid.appearance_normal_states
            .iter()
            .map(|(state, _)| state.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(states(&group.kids[0]), vec!["0", "Off"]);
    assert_eq!(states(&group.kids[1]), vec!["1", "Off"]);
}

/// 写入 → 重新解析：多选选择字段的 `/TI` 与 `/I`（Table 231）应类型化回读。
#[test]
fn builder_choice_indices_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AcroForm, FormField, FormOption, PageSize, PdfBuilder};
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::choice("size", [72.0, 500.0, 200.0, 520.0])
            .with_option(FormOption::new("S").with_display("Small"))
            .with_option(FormOption::new("M"))
            .with_option(FormOption::new("L"))
            .with_flags(FormField::MULTI_SELECT)
            .with_top_index(1)
            .with_alternate_indices([0, 2]),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let form = doc.acro_form().expect("parse form").expect("form present");
    let field = &form.fields[0];
    assert_eq!(field.top_index, Some(1));
    assert_eq!(field.alternate_indices, Some(vec![0, 2]));
    let options = field.options.as_ref().expect("/Opt");
    assert_eq!(
        options
            .iter()
            .map(|option| (option.export(), option.display()))
            .collect::<Vec<_>>(),
        vec![("S", Some("Small")), ("M", None), ("L", None)]
    );
}

/// 写入 → 重新解析：无 `/T` 的 Widget 与具名父节点同属一个全限定名
/// （12.7.3.2），解析后归并为一个逻辑字段的多重表示。
#[test]
fn builder_shared_field_representations_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AcroForm, FieldValue, FormField, PageSize, PdfBuilder};
    use pdf_engine::core::reader::form::FieldType;
    use pdf_engine::core::types::{PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::radio_group("cardbrand")
            .with_value(FieldValue::Name("Off".into()))
            .with_default_value(FieldValue::Name("Off".into()))
            .with_child(
                FormField::widget([72.0, 560.0, 92.0, 580.0])
                    .with_tooltip("Visa")
                    .with_value(FieldValue::Name("Off".into())),
            )
            .with_child(FormField::widget([100.0, 560.0, 120.0, 580.0]).with_tooltip("Mastercard")),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let form = doc.acro_form().expect("parse form").expect("form present");
    let groups = form.logical_fields();
    assert_eq!(groups.len(), 1, "three dictionaries, one underlying field");
    let group = &groups[0];
    assert_eq!(group.fully_qualified_name, "cardbrand");
    assert_eq!(group.representations.len(), 3);
    assert_eq!(group.widgets.len(), 2);
    assert_eq!(group.field_type, Some(FieldType::Button));
    assert_eq!(group.value, Some(&Value::Name("Off".into())));
    assert_eq!(group.default_value, Some(&Value::Name("Off".into())));
    assert_eq!(form.logical_field("cardbrand").as_ref(), Some(group));
    assert_eq!(
        group
            .widgets
            .iter()
            .map(|widget| widget.dict.get("TU").is_some())
            .collect::<Vec<_>>(),
        vec![true, true],
        "each representation keeps its own appearance entries"
    );
}
