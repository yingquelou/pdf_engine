//! 同全限定名字段的多重表示（12.7.3.2）：无 `/T` 的 Widget 与最近具名祖先
//! 指向同一底层字段，须共享 `/V`、`/DV`；外观类条目（`/Rect`、`/TU`）可各自不同。

use super::*;
use crate::core::writer::WriteError;

const FIRST: [f64; 4] = [72.0, 560.0, 92.0, 580.0];
const SECOND: [f64; 4] = [100.0, 560.0, 120.0, 580.0];

fn on(value: &str) -> FieldValue {
    FieldValue::Name(value.to_string())
}

fn build(field: FormField) -> (Result<(), WriteError>, Vec<u8>) {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, field);
    builder.set_acro_form(form);
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    (result, out)
}

fn assemble(field: FormField) -> Document {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(page, field);
    builder.set_acro_form(form);
    builder.assemble_document().expect("assemble")
}

#[test]
fn widget_representations_fold_into_one_logical_field() {
    let doc = assemble(
        FormField::radio_group("agree")
            .with_value(on("1"))
            .with_default_value(on("Off"))
            .with_child(FormField::widget(FIRST).with_tooltip("first"))
            .with_child(
                FormField::widget(SECOND)
                    .with_tooltip("second")
                    .with_value(on("1")),
            ),
    );
    let form = doc.acro_form().expect("read").expect("form present");
    let group = form.logical_field("agree").expect("logical field");
    assert_eq!(group.representations.len(), 3);
    assert_eq!(group.widgets.len(), 2);
    assert_eq!(group.value, Some(&Value::Name("1".into())));
    assert_eq!(group.default_value, Some(&Value::Name("Off".into())));
    // 无名 Widget 显式声明同名条目 `/V` 与父节点一致，允许写出；
    // 外观类条目（`/Rect`、`/TU`）各表示独立。
    assert_eq!(
        group
            .widgets
            .iter()
            .map(|widget| (
                widget.rect.expect("widget /Rect"),
                widget.dict.get("TU").is_some(),
            ))
            .collect::<Vec<_>>(),
        vec![
            ([72.0, 560.0, 92.0, 580.0], true),
            ([100.0, 560.0, 120.0, 580.0], true)
        ],
    );
}

#[test]
fn conflicting_values_on_representations_are_rejected() {
    let cases = vec![
        (
            "differing /V",
            "/V",
            FormField::radio_group("agree")
                .with_value(on("1"))
                .with_child(FormField::widget(FIRST).with_value(on("0"))),
        ),
        (
            "differing /DV",
            "/DV",
            FormField::radio_group("agree")
                .with_default_value(on("Off"))
                .with_child(FormField::widget(FIRST).with_default_value(on("1"))),
        ),
        (
            "/V only on the widget",
            "/V",
            FormField::radio_group("agree")
                .with_child(FormField::widget(FIRST).with_value(on("0"))),
        ),
        (
            "/DV only on the widget",
            "/DV",
            FormField::radio_group("agree")
                .with_child(FormField::widget(FIRST).with_default_value(on("Off"))),
        ),
    ];
    for (label, entry, field) in cases {
        let (result, bytes) = build(field);
        match result.expect_err(label) {
            WriteError::InvalidObject(message) => assert!(
                message.contains("sharing a fully qualified name") && message.contains(entry),
                "{label}: unexpected message {message:?}"
            ),
            other => panic!("{label} should be InvalidObject, got {other:?}"),
        }
        assert!(bytes.is_empty(), "{label} must not write bytes");
    }
}

#[test]
fn named_children_keep_their_own_values() {
    let doc = assemble(
        FormField::group("shipping")
            .with_child(FormField::radio("home", FIRST).with_value(on("1")))
            .with_child(FormField::radio("work", SECOND).with_value(on("0"))),
    );
    let form = doc.acro_form().expect("read").expect("form present");
    assert_eq!(
        form.logical_fields()
            .iter()
            .map(|group| (
                group.fully_qualified_name.as_str(),
                group.representations.len()
            ))
            .collect::<Vec<_>>(),
        vec![("shipping", 1), ("shipping.home", 1), ("shipping.work", 1)],
        "a distinct /T is a distinct field, not another representation"
    );
    assert_eq!(
        form.logical_field("shipping.home").expect("home").value,
        Some(&Value::Name("1".into()))
    );
    assert_eq!(
        form.logical_field("shipping.work").expect("work").value,
        Some(&Value::Name("0".into()))
    );
}
