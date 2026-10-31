//! Hide 动作写端测试（ISO 32000-1 12.6.4.10，Table 210）：
//! `/T` 注解引用与字段名的写出、注解句柄解析、取值校验与 PDF 1.2 门禁。

use super::support::annot_object_with_num;
use super::*;
use crate::core::builder::navigation;
use crate::core::types::PdfFeature;

fn text() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 120.0, 720.0])
}

fn link_with_action(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(action)
}

fn hide(targets: Vec<HideTarget>) -> Action {
    Action::Hide(HideAction::new(targets))
}

#[test]
fn hide_action_writes_annotation_and_field_targets() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    let target = b.add_annotation(p0, text());
    b.add_annotation(
        p0,
        link_with_action(Action::Hide(
            HideAction::new(vec![
                HideTarget::Annotation(target),
                HideTarget::FieldName("group.note".into()),
            ])
            .with_hidden(true),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::HideAction));

    let (target_num, _) = annot_object_with_num(&doc, 3, 0);
    let (_, link) = annot_object_with_num(&doc, 3, 1);
    let action = link.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(action.get("S"), Some(&Value::Name("Hide".into())));
    let targets = action.get("T").and_then(Value::as_array).expect("/T");
    assert_eq!(
        targets.0[0],
        Value::Ref(target_num, 0),
        "注解句柄解析为对象引用"
    );
    assert!(
        matches!(targets.0[1], Value::HexString(_)),
        "字段名写成文本串"
    );
    assert_eq!(action.get("H"), Some(&Value::Bool(true)));
}

#[test]
fn outline_hide_action_resolves_annotation_targets() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let target = b.add_annotation(p0, text());
    let handle = b.action(hide(vec![HideTarget::Annotation(target)]));
    b.outline(&[OutlineEntry {
        title: "Hide".into(),
        destination: None,
        action: Some(handle),
        color: None,
        flags: 0,
        expanded: false,
        children: Vec::new(),
    }]);

    let mut doc = b.assemble_document().expect("assemble");
    navigation::apply(
        &mut doc,
        b.version,
        b.pages.len(),
        &b.navigation,
        crate::core::builder::types::ActionRefs {
            annotation_nums: &b.annotation_nums,
            annotation_pages: &b.annotation_pages,
            ocg_nums: &b.ocg_nums,
            sound_nums: &b.sound_nums,
            form_nums: &b.form_nums,
            embedded_nums: &b.embedded_nums,
            rendition_nums: &b.rendition_nums,
            rendition_cursor: Some(&b.rendition_cursor),
        },
    )
    .expect("apply");
    let target_num = b.annotation_nums[target];
    let action = doc
        .objects_for_write()
        .into_iter()
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("S"))
                .and_then(Value::as_name)
                == Some("Hide")
        })
        .expect("outline action object");
    let dict = action.value.as_dict().expect("action dict");
    assert_eq!(dict.get("T"), Some(&Value::Ref(target_num, 0)));
    assert!(dict.get("H").is_none(), "缺省 /H 不物化");
}

#[test]
fn hide_action_requires_pdf_1_2() {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        text().with_action(hide(vec![HideTarget::FieldName("note".into())])),
    );
    let error = b.build(&mut out).expect_err("hide action needs PDF 1.2");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(format!("{error}").contains("hide action"), "{error}");
    assert_eq!(out.len(), 0);
}

#[test]
fn invalid_hide_targets_are_rejected() {
    let cases = [
        HideAction::new(Vec::new()),
        HideAction::new(vec![HideTarget::FieldName("  ".into())]),
        HideAction::new(vec![
            HideTarget::FieldName("name".into()),
            HideTarget::FieldName("name".into()),
        ]),
        HideAction::new(vec![HideTarget::Annotation(7)]),
        HideAction::new(vec![HideTarget::Annotation(0), HideTarget::Annotation(0)]),
    ];
    for case in cases {
        let mut out = Vec::new();
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, text());
        b.add_annotation(p0, link_with_action(Action::Hide(case.clone())));
        let error = b.build(&mut out).expect_err("invalid hide targets");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{case:?}: {error}"
        );
        assert_eq!(out.len(), 0, "{case:?}");
    }
}
