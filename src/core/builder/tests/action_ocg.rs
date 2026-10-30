//! Set-OCG-State 动作写端测试（ISO 32000-1 12.6.4.12，Table 213）：
//! `/State` 序列与组引用的写出、动作专有可选内容组的对象分配、取值校验
//! 与 PDF 1.5 门禁。

use super::support::annot_object;
use super::*;
use crate::core::builder::navigation;
use crate::core::types::{Array, PdfFeature, PdfTextString};

fn link_with_action(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(action)
}

fn state(sequences: Vec<OcgStateSequence>) -> Action {
    Action::SetOcgState(SetOcgStateAction::new(sequences))
}

fn hex_of(text: &str) -> String {
    let bytes = PdfTextString::from(text).encode();
    let mut hex = String::new();
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

/// 按对象号顺序收集已写出的 `/Type /OCG` 对象的名称。
fn ocg_names(doc: &crate::core::types::Document) -> Vec<Value> {
    let mut names = Vec::new();
    for num in 1..=64 {
        let Some(object) = doc.get_object(num).expect("object lookup") else {
            continue;
        };
        if let Value::Dict(dict) = object.value
            && dict.get("Type").and_then(Value::as_name) == Some("OCG")
        {
            names.push(dict.get("Name").cloned().expect("/Name"));
        }
    }
    names
}

#[test]
fn set_ocg_state_action_allocates_groups_and_writes_state_array() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let page_group = b.optional_content_group(&OptionalContentGroup::new("Page Layer"));
    let action_group = b.optional_content_group(&OptionalContentGroup::new("Action Layer"));
    let unused_group = b.optional_content_group(&OptionalContentGroup::new("Unused Layer"));
    b.begin_optional_content(p0, page_group);
    b.text(p0, &Font::helvetica(12.0), 72.0, 700.0, "Layer");
    b.end_optional_content(p0);
    b.add_annotation(
        p0,
        link_with_action(Action::SetOcgState(
            SetOcgStateAction::new(vec![
                OcgStateSequence::new(OcgStateOperation::Off, vec![action_group]),
                OcgStateSequence::new(OcgStateOperation::Toggle, vec![page_group]),
            ])
            .with_preserve_radio_buttons(false),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    assert!(b.ocg_nums[page_group] > 0, "页面标记的组有对象号");
    assert!(b.ocg_nums[action_group] > 0, "动作引用的组也有对象号");
    assert_eq!(b.ocg_nums[unused_group], 0, "未引用的组不写出");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::SetOcgState));
    assert!(features.contains(&PdfFeature::OptionalContent));

    let link = annot_object(&doc, 3, 0);
    let action = link.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(action.get("S"), Some(&Value::Name("SetOCGState".into())));
    let state = action
        .get("State")
        .and_then(Value::as_array)
        .expect("/State");
    assert_eq!(
        state.0,
        vec![
            Value::Name("OFF".into()),
            Value::Ref(b.ocg_nums[action_group], 0),
            Value::Name("Toggle".into()),
            Value::Ref(b.ocg_nums[page_group], 0),
        ],
        "/State 按 Table 213 写成扁平的名字/引用序列"
    );
    assert_eq!(action.get("PreserveRB"), Some(&Value::Bool(false)));

    // 动作专有组与页面组都登记 `/OCProperties`；未引用组没有 OCG 对象。
    let names = ocg_names(&doc);
    assert_eq!(
        names,
        vec![
            Value::HexString(hex_of("Page Layer")),
            Value::HexString(hex_of("Action Layer")),
        ]
    );
}

#[test]
fn outline_set_ocg_state_action_resolves_group_objects() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Outline Layer"));
    let handle = b.action(state(vec![OcgStateSequence::new(
        OcgStateOperation::On,
        vec![group],
    )]));
    b.outline(&[OutlineEntry {
        title: "Layers".into(),
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
        &b.annotation_nums,
        &b.ocg_nums,
    )
    .expect("apply");
    assert!(b.ocg_nums[group] > 0, "大纲动作引用的组有对象号");
    let action = doc
        .objects_for_write()
        .into_iter()
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("S"))
                .and_then(Value::as_name)
                == Some("SetOCGState")
        })
        .expect("outline action object");
    let dict = action.value.as_dict().expect("action dict");
    assert_eq!(
        dict.get("State"),
        Some(&Value::Array(Array(vec![
            Value::Name("ON".into()),
            Value::Ref(b.ocg_nums[group], 0),
        ]))),
        "大纲动作 /State 解析为组对象引用"
    );
}

#[test]
fn set_ocg_state_action_requires_pdf_1_5() {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Layer"));
    b.add_annotation(
        p0,
        link_with_action(state(vec![OcgStateSequence::new(
            OcgStateOperation::On,
            vec![group],
        )])),
    );
    let error = b.build(&mut out).expect_err("Set-OCG-State needs PDF 1.5");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(
        format!("{error}").contains("Set-OCG-State action"),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn outline_set_ocg_state_action_requires_pdf_1_5() {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Layer"));
    let handle = b.action(state(vec![OcgStateSequence::new(
        OcgStateOperation::On,
        vec![group],
    )]));
    b.outline(&[OutlineEntry {
        title: "Layers".into(),
        destination: None,
        action: Some(handle),
        color: None,
        flags: 0,
        expanded: false,
        children: Vec::new(),
    }]);
    let error = b
        .build(&mut out)
        .expect_err("outline Set-OCG-State needs PDF 1.5");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(
        format!("{error}").contains("Set-OCG-State action"),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn invalid_set_ocg_state_groups_are_rejected() {
    let cases = [
        SetOcgStateAction::new(Vec::new()),
        SetOcgStateAction::new(vec![OcgStateSequence::new(
            OcgStateOperation::On,
            Vec::new(),
        )]),
        SetOcgStateAction::new(vec![OcgStateSequence::new(
            OcgStateOperation::Toggle,
            vec![7],
        )]),
    ];
    for case in cases {
        let mut out = Vec::new();
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.optional_content_group(&OptionalContentGroup::new("Layer"));
        b.add_annotation(p0, link_with_action(Action::SetOcgState(case.clone())));
        let error = b.build(&mut out).expect_err("invalid Set-OCG-State groups");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{case:?}: {error}"
        );
        assert_eq!(out.len(), 0, "{case:?}");
    }
}
