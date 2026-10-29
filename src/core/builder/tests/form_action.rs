//! 表单数据动作写端测试（ISO 32000-1 12.7.5.2–12.7.5.4，
//! Table 236/237/238/239/240）：SubmitForm/ResetForm/ImportData 的写出、
//! 取值校验、版本门禁与特性登记。

use super::support::annot_object;
use super::*;
use crate::core::reader::content::text_value;
use crate::core::types::PdfFeature;

fn link_with_action(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 700.0, 300.0, 720.0]).with_action(action)
}

fn build_one(action: Action, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, link_with_action(action));
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

fn assemble_action(action: Action, version: PdfVersion) -> (Dict, Vec<PdfFeature>) {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, link_with_action(action));
    let doc = b.assemble_document().expect("assemble");
    let annot = annot_object(&doc, 3, 0);
    let action = annot.get("A").and_then(Value::as_dict).expect("/A").clone();
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    (action, features)
}

#[test]
fn submit_form_action_writes_url_fields_flags_and_url_type() {
    let (action, features) = assemble_action(
        Action::SubmitForm {
            url: "https://example.com/submit".into(),
            fields: vec!["name".into(), "group.total".into()],
            flags: 0x0005,
            url_type: Some("Browser".into()),
        },
        PdfVersion::V1_7,
    );

    assert_eq!(action.get("S"), Some(&Value::Name("SubmitForm".into())));
    let file = action.get("F").and_then(Value::as_dict).expect("/F");
    assert_eq!(file.get("FS"), Some(&Value::Name("URL".into())));
    assert_eq!(
        file.get("F").and_then(text_value).as_deref(),
        Some("https://example.com/submit")
    );
    let fields = action
        .get("Fields")
        .and_then(Value::as_array)
        .expect("/Fields");
    let names: Vec<String> = fields.0.iter().filter_map(text_value).collect();
    assert_eq!(names, vec!["name", "group.total"]);
    assert_eq!(action.get("Flags"), Some(&Value::Integer(5)));
    assert_eq!(action.get("URLType"), Some(&Value::Name("Browser".into())));

    for feature in [
        PdfFeature::FormDataActions,
        PdfFeature::FormActionFieldNames,
        PdfFeature::SubmitFormUrlType,
    ] {
        assert!(features.contains(&feature), "{feature} must be recorded");
    }
}

#[test]
fn reset_form_and_import_data_actions_write_entries() {
    let (reset, features) = assemble_action(
        Action::ResetForm {
            fields: vec!["total".into()],
            flags: 1,
        },
        PdfVersion::V1_3,
    );
    assert_eq!(reset.get("S"), Some(&Value::Name("ResetForm".into())));
    let fields = reset
        .get("Fields")
        .and_then(Value::as_array)
        .expect("/Fields");
    assert_eq!(
        fields.0.iter().filter_map(text_value).collect::<Vec<_>>(),
        vec!["total"]
    );
    assert_eq!(reset.get("Flags"), Some(&Value::Integer(1)));
    assert!(features.contains(&PdfFeature::FormDataActions));
    assert!(features.contains(&PdfFeature::FormActionFieldNames));

    let (minimal, features) = assemble_action(
        Action::ResetForm {
            fields: Vec::new(),
            flags: 0,
        },
        PdfVersion::V1_2,
    );
    assert_eq!(minimal.get("S"), Some(&Value::Name("ResetForm".into())));
    assert!(minimal.get("Fields").is_none(), "空名录不写");
    assert!(minimal.get("Flags").is_none(), "0 标志不写");
    assert!(features.contains(&PdfFeature::FormDataActions));

    let (import, features) = assemble_action(
        Action::ImportData {
            file: "data.fdf".into(),
        },
        PdfVersion::V1_2,
    );
    assert_eq!(import.get("S"), Some(&Value::Name("ImportData".into())));
    assert_eq!(
        import.get("F").and_then(text_value).as_deref(),
        Some("data.fdf")
    );
    assert_eq!(features, vec![PdfFeature::FormDataActions]);
}

#[test]
fn form_actions_require_pdf_1_2() {
    let submit = || Action::SubmitForm {
        url: "https://example.com/submit".into(),
        fields: Vec::new(),
        flags: 0,
        url_type: None,
    };
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, link_with_action(submit()));
    let error = b.build(&mut out).expect_err("1.1 rejects form actions");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("form data actions") && message.contains("1.2")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    build_one(submit(), PdfVersion::V1_2).expect("form actions at 1.2");
}

#[test]
fn form_action_field_names_require_pdf_1_3() {
    let named = || Action::ResetForm {
        fields: vec!["total".into()],
        flags: 0,
    };
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, link_with_action(named()));
    let error = b
        .build(&mut out)
        .expect_err("1.2 rejects /Fields name strings");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("form action field names") && message.contains("1.3")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    build_one(named(), PdfVersion::V1_3).expect("/Fields names at 1.3");
}

#[test]
fn submit_form_url_type_requires_pdf_1_7() {
    let typed = || Action::SubmitForm {
        url: "https://example.com/submit".into(),
        fields: Vec::new(),
        flags: 0,
        url_type: Some("Browser".into()),
    };
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, link_with_action(typed()));
    let error = b.build(&mut out).expect_err("1.6 rejects /URLType");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("submit-form URL type") && message.contains("1.7")),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    build_one(typed(), PdfVersion::V1_7).expect("/URLType at 1.7");
}

#[test]
fn invalid_form_action_values_are_rejected() {
    let submit = |url: &str, fields: Vec<String>, flags: u32| Action::SubmitForm {
        url: url.into(),
        fields,
        flags,
        url_type: None,
    };
    let cases = [
        submit("", Vec::new(), 0),
        submit("https://example.com/ü", Vec::new(), 0),
        submit("https://example.com", vec![String::new()], 0),
        submit(
            "https://example.com",
            vec!["total".into(), "total".into()],
            0,
        ),
        submit("https://example.com", Vec::new(), 0x4000),
        Action::ResetForm {
            fields: Vec::new(),
            flags: 0x2,
        },
        Action::ImportData { file: " ".into() },
    ];
    for action in cases {
        let mut out = Vec::new();
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, link_with_action(action.clone()));
        let error = b.build(&mut out).expect_err("invalid form action");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "{action:?}: {error}"
        );
        assert_eq!(out.len(), 0, "{action:?}");
    }
}
