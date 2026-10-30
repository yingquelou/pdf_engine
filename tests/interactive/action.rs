//! E3 Hide 动作的写出 → 解析 → 回读往返（12.6.4.10，Table 210）。

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
