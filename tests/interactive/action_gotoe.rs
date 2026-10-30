//! E3 Embedded Go-To 动作的写出 → 解析 → 回读往返（12.6.4.4，Table 201/202）。

use super::*;

/// 写入 → 重新解析：`/F` 文件规格、命名目标、`/NewWindow` 与
/// 父 → 子（附件注解）目标链经读端还原。
#[test]
fn builder_embedded_go_to_action_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Action, ActionFileSpec, ActionRemoteDestination, AnnotationKind, GoToEAction, GoToETarget,
        GoToETargetAnnotation, GoToETargetPage, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::action::{
        Action as ReadAction, GoToETargetAnnotation as ReadAnnotation, GoToETargetPage as ReadPage,
        GoToETargetRelation as ReadRelation, RemoteDestination as ReadDestination,
    };
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(
            Action::GoToE(
                GoToEAction::new(ActionRemoteDestination::Named("Chapter 1".into()))
                    .with_file(ActionFileSpec::new("root.pdf"))
                    .with_new_window(true)
                    .with_target(
                        GoToETarget::parent().with_target(GoToETarget::child_attachment(
                            GoToETargetPage::Named("Chapter start".into()),
                            GoToETargetAnnotation::Name("note".into()),
                        )),
                    ),
            ),
        ),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build GoToE action");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    match doc
        .action(annotations[0].action.as_ref().expect("/A"))
        .expect("parse GoToE action")
    {
        ReadAction::GoToE(action) => {
            assert_eq!(action.file.expect("file").file, "root.pdf");
            assert_eq!(
                action.destination,
                ReadDestination::Named("Chapter 1".into())
            );
            assert_eq!(action.new_window, Some(true));
            let parent = action.target.expect("target");
            assert_eq!(parent.relation, ReadRelation::Parent);
            assert_eq!(parent.name, None);
            assert_eq!(parent.page, None);
            assert_eq!(parent.annotation, None);
            let child = parent.target.expect("nested target");
            assert_eq!(child.relation, ReadRelation::Child);
            assert_eq!(child.name, None);
            assert_eq!(child.page, Some(ReadPage::Named("Chapter start".into())));
            assert_eq!(child.annotation, Some(ReadAnnotation::Name("note".into())));
            assert!(child.target.is_none());
        }
        other => panic!("unexpected action: {other:?}"),
    }
}
