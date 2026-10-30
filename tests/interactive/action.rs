//! E3 动作的写出 → 解析 → 回读往返：Hide（12.6.4.10，Table 210）与
//! Transition（12.6.4.14，Table 215）。

use super::*;

/// 写入 → 重新解析：Link 注解的 `/A /Hide` 目标数组经读端还原为注解引用
/// 与字段全限定名，`/H` 显式值与单目标形态分别断言。
#[test]
fn builder_hide_action_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, HideAction, HideTarget, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::action::{Action as ReadAction, HideTarget as ReadHideTarget};

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let popup = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 120.0, 720.0]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(
            Action::Hide(
                HideAction::new(vec![
                    HideTarget::Annotation(popup),
                    HideTarget::FieldName("group.note".into()),
                ])
                .with_hidden(false),
            ),
        ),
    );
    // 单字段名目标：按 Table 210 写成标量文本串，`/H` 缺省不物化。
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 500.0, 120.0, 520.0]).with_action(
            Action::Hide(HideAction::new(vec![HideTarget::FieldName(
                "group.hint".into(),
            )])),
        ),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build hide actions");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 3);
    let (popup_num, _) = annotations[0]
        .reference
        .expect("text is an indirect object");
    match doc
        .action(annotations[1].action.as_ref().expect("/A"))
        .expect("parse hide action")
    {
        ReadAction::Hide(hide) => {
            assert_eq!(hide.hide, Some(false));
            assert_eq!(
                hide.targets,
                vec![
                    ReadHideTarget::Annotation {
                        number: popup_num,
                        generation: 0,
                    },
                    ReadHideTarget::FieldName("group.note".into()),
                ]
            );
        }
        other => panic!("unexpected action: {other:?}"),
    }
    match doc
        .action(annotations[2].action.as_ref().expect("/A"))
        .expect("parse single-target hide action")
    {
        ReadAction::Hide(hide) => {
            assert_eq!(hide.hide, None, "缺省 /H 视为隐藏");
            assert_eq!(
                hide.targets,
                vec![ReadHideTarget::FieldName("group.hint".into())]
            );
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

/// 写入 → 重新解析：Link 注解的 `/A /Trans` 过渡字典经读端还原为样式与
/// 专有条目；未声明的条目保持 `None`（缺省不物化）。
#[test]
fn builder_transition_action_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, Transition,
        TransitionDimension, TransitionDirection, TransitionMotion, TransitionStyle,
    };
    use pdf_engine::core::reader::action::Action as ReadAction;
    use pdf_engine::core::reader::transition::{
        Transition as ReadTransition, TransitionDimension as ReadDimension,
        TransitionDirection as ReadDirection, TransitionMotion as ReadMotion,
        TransitionStyle as ReadStyle,
    };

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(
            Action::Transition(
                Transition::new(TransitionStyle::Split)
                    .with_duration(2.5)
                    .with_dimension(TransitionDimension::Vertical)
                    .with_motion(TransitionMotion::Outward),
            ),
        ),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 560.0, 300.0, 580.0]).with_action(
            Action::Transition(
                Transition::new(TransitionStyle::Fly)
                    .with_direction(TransitionDirection::None)
                    .with_scale(0.25)
                    .with_border(true),
            ),
        ),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build transition actions");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);
    match doc
        .action(annotations[0].action.as_ref().expect("/A"))
        .expect("parse split transition action")
    {
        ReadAction::Transition(transition) => assert_eq!(
            transition,
            ReadTransition {
                style: ReadStyle::Split,
                duration: Some(2.5),
                dimension: Some(ReadDimension::Vertical),
                motion: Some(ReadMotion::Outward),
                direction: None,
                scale: None,
                border: None,
            }
        ),
        other => panic!("unexpected action: {other:?}"),
    }
    match doc
        .action(annotations[1].action.as_ref().expect("/A"))
        .expect("parse fly transition action")
    {
        ReadAction::Transition(transition) => assert_eq!(
            transition,
            ReadTransition {
                style: ReadStyle::Fly,
                duration: None,
                dimension: None,
                motion: None,
                direction: Some(ReadDirection::None),
                scale: Some(0.25),
                border: Some(true),
            }
        ),
        other => panic!("unexpected action: {other:?}"),
    }
}
