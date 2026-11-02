//! 字段附加动作（`/AA`，12.7.5.2）：`/K` `/F` `/V` 触发事件的 JavaScript 动作
//! 写出、读端逐事件类型化回读，以及重复事件、非 JavaScript 动作与无名
//! Calculate 字段在构建期拒绝。

use super::super::*;
use super::support::{acro_dict, assert_text_entry, build_error, object_dict};
use crate::core::writer::WriteError;

#[test]
fn field_actions_write_aa_dictionary_and_roundtrip() {
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("amount", [72.0, 700.0, 300.0, 720.0])
            .with_action(
                FieldActionEvent::Keystroke,
                Action::JavaScript("AFNumber_Keystroke(2)".into()),
            )
            .with_action(
                FieldActionEvent::Format,
                Action::JavaScript("AFNumber_Format(2)".into()),
            )
            .with_action(
                FieldActionEvent::Validate,
                Action::JavaScript("validateAmount()".into()),
            ),
    );
    builder.set_acro_form(form);
    let doc = builder.assemble_document().expect("assemble");

    let fields = acro_dict(&doc)
        .get("Fields")
        .and_then(|value| value.as_array())
        .expect("/Fields")
        .clone();
    let (field_num, _) = fields.0[0].as_ref().expect("field reference");
    let field = object_dict(&doc, field_num);
    let aa = field
        .get("AA")
        .and_then(|value| value.as_dict())
        .expect("/AA")
        .clone();
    assert_eq!(aa.0.len(), 3, "three typed trigger events");
    for key in ["K", "F", "V"] {
        let action = aa
            .get(key)
            .and_then(|value| value.as_dict())
            .unwrap_or_else(|| panic!("/AA /{key}"));
        assert_eq!(
            action.get("S"),
            Some(&Value::Name("JavaScript".into())),
            "/AA /{key} subtype"
        );
        assert_text_entry(action, "JS");
    }

    let parsed = doc.acro_form().expect("read").expect("form present");
    let actions = parsed.fields[0].actions.as_ref().expect("/AA");
    assert_eq!(
        actions.keystroke,
        Some(crate::core::reader::action::Action::JavaScript(
            "AFNumber_Keystroke(2)".into()
        ))
    );
    assert_eq!(
        actions.format,
        Some(crate::core::reader::action::Action::JavaScript(
            "AFNumber_Format(2)".into()
        ))
    );
    assert_eq!(
        actions.validate,
        Some(crate::core::reader::action::Action::JavaScript(
            "validateAmount()".into()
        ))
    );
    assert!(actions.calculate.is_none());
    assert!(actions.other.is_empty());
}

#[test]
fn field_actions_reject_duplicates_non_javascript_and_unnamed_calculate() {
    let rect = [72.0, 700.0, 200.0, 720.0];
    let duplicate = FormField::text("x", rect)
        .with_action(
            FieldActionEvent::Validate,
            Action::JavaScript("first()".into()),
        )
        .with_action(
            FieldActionEvent::Validate,
            Action::JavaScript("second()".into()),
        );
    let error = build_error(duplicate, 0);
    assert!(
        matches!(&error, WriteError::InvalidObject(message) if message.contains("declared twice")),
        "duplicate /AA event: {error:?}"
    );

    let non_javascript = FormField::text("x", rect).with_action(
        FieldActionEvent::Keystroke,
        Action::Uri("https://example.com".into()),
    );
    let error = build_error(non_javascript, 0);
    assert!(
        matches!(&error, WriteError::InvalidObject(message) if message.contains("JavaScript action")),
        "non-JavaScript /AA action: {error:?}"
    );

    let unnamed = FormField::widget(rect).with_action(
        FieldActionEvent::Calculate,
        Action::JavaScript("recalc()".into()),
    );
    let error = build_error(unnamed, 0);
    assert!(
        matches!(&error, WriteError::InvalidObject(message) if message.contains("non-empty partial name")),
        "unnamed calculate field: {error:?}"
    );
}
