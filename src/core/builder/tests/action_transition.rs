//! Transition 动作写端测试（ISO 32000-1 12.6.4.14，Table 215；过渡字典
//! Table 162）：`/Trans` 字典的写出、样式专有条目的取值校验与 PDF 1.5 门禁。

use super::support::annot_object;
use super::*;
use crate::core::types::PdfFeature;

fn link_with_action(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(action)
}

fn transition(transition: Transition) -> Action {
    Action::Transition(transition)
}

#[test]
fn transition_action_writes_transition_dictionary() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(transition(
            Transition::new(TransitionStyle::Split)
                .with_duration(2.5)
                .with_dimension(TransitionDimension::Vertical)
                .with_motion(TransitionMotion::Outward),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::TransitionAction));

    let link = annot_object(&doc, 3, 0);
    let action = link.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(action.get("S"), Some(&Value::Name("Trans".into())));
    let trans = action
        .get("Trans")
        .and_then(Value::as_dict)
        .expect("/Trans");
    assert_eq!(trans.get("Type"), Some(&Value::Name("Trans".into())));
    assert_eq!(trans.get("S"), Some(&Value::Name("Split".into())));
    assert_eq!(trans.get("D"), Some(&Value::Float(2.5)));
    assert_eq!(trans.get("Dm"), Some(&Value::Name("V".into())));
    assert_eq!(trans.get("M"), Some(&Value::Name("O".into())));
    assert!(trans.get("Di").is_none(), "未声明的 /Di 不物化");
}

#[test]
fn transition_action_writes_fly_entries_and_none_direction() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(transition(
            Transition::new(TransitionStyle::Fly)
                .with_direction(TransitionDirection::None)
                .with_scale(0.25)
                .with_border(true),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    let link = annot_object(&doc, 3, 0);
    let trans = link
        .get("A")
        .and_then(Value::as_dict)
        .and_then(|action| action.get("Trans"))
        .and_then(Value::as_dict)
        .expect("/Trans");
    assert_eq!(trans.get("S"), Some(&Value::Name("Fly".into())));
    assert_eq!(trans.get("Di"), Some(&Value::Name("None".into())));
    assert_eq!(trans.get("SS"), Some(&Value::Float(0.25)));
    assert_eq!(trans.get("B"), Some(&Value::Bool(true)));
}

#[test]
fn transition_action_requires_pdf_1_5() {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(transition(Transition::new(TransitionStyle::Dissolve))),
    );
    let error = b
        .build(&mut out)
        .expect_err("Transition action needs PDF 1.5");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(format!("{error}").contains("transition action"), "{error}");
    assert_eq!(out.len(), 0);
}

#[test]
fn invalid_transition_entries_are_rejected() {
    let cases = [
        Transition::new(TransitionStyle::Split).with_duration(-1.0),
        Transition::new(TransitionStyle::Dissolve).with_dimension(TransitionDimension::Horizontal),
        Transition::new(TransitionStyle::Dissolve).with_motion(TransitionMotion::Inward),
        Transition::new(TransitionStyle::Dissolve).with_direction(TransitionDirection::Degrees(0)),
        Transition::new(TransitionStyle::Wipe).with_direction(TransitionDirection::Degrees(315)),
        Transition::new(TransitionStyle::Glitter).with_direction(TransitionDirection::Degrees(90)),
        Transition::new(TransitionStyle::Wipe).with_direction(TransitionDirection::Degrees(45)),
        Transition::new(TransitionStyle::Box).with_direction(TransitionDirection::None),
        Transition::new(TransitionStyle::Dissolve).with_scale(2.0),
        Transition::new(TransitionStyle::Dissolve).with_border(true),
        Transition::new(TransitionStyle::Fly).with_scale(0.0),
    ];
    for case in cases {
        let mut out = Vec::new();
        let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, link_with_action(transition(case.clone())));
        let error = b.build(&mut out).expect_err("invalid transition entries");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{case:?}: {error}"
        );
        assert_eq!(out.len(), 0, "{case:?}");
    }
}
