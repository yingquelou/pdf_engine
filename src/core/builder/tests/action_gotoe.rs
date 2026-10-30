//! Embedded Go-To 动作写端测试（12.6.4.4 Table 201/202）：目标字典路径链
//! 的写出、目标形态校验与 PDF 1.6 门禁。

use super::support::annot_object;
use super::*;
use crate::core::reader::content::text_value;
use crate::core::types::PdfFeature;

fn link_with_action(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(action)
}

fn action_dict(doc: &Document, index: usize) -> Dict {
    annot_object(doc, 3, index)
        .get("A")
        .and_then(Value::as_dict)
        .cloned()
        .expect("/A")
}

#[test]
fn embedded_go_to_action_writes_target_chain() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(Action::GoToE(
            GoToEAction::new(ActionRemoteDestination::Named("Chapter 1".into()))
                .with_file(ActionFileSpec::new("root.pdf"))
                .with_new_window(false)
                .with_target(
                    GoToETarget::parent()
                        .with_target(GoToETarget::child_in_name_tree("Embedded document")),
                ),
        )),
    );
    // 仅 /F 的形态：附件注解子目标，命名页 + 注解索引。
    b.add_annotation(
        p0,
        link_with_action(Action::GoToE(
            GoToEAction::new(ActionRemoteDestination::Page(4))
                .with_file(ActionFileSpec::new("a.pdf"))
                .with_target(GoToETarget::child_attachment(
                    GoToETargetPage::Named("Chapter start".into()),
                    GoToETargetAnnotation::Index(1),
                )),
        )),
    );

    let doc = b.assemble_document().expect("assemble");
    let action = action_dict(&doc, 0);
    assert_eq!(action.get("S"), Some(&Value::Name("GoToE".into())));
    assert_eq!(action.get("D"), Some(&Value::String("Chapter 1".into())));
    assert_eq!(action.get("NewWindow"), Some(&Value::Bool(false)));
    let file = action.get("F").and_then(Value::as_dict).expect("/F");
    assert_eq!(file.get("UF").and_then(text_value), Some("root.pdf".into()));
    let target = action.get("T").and_then(Value::as_dict).expect("/T");
    assert_eq!(target.get("R"), Some(&Value::Name("P".into())));
    assert!(target.get("N").is_none());
    let nested = target.get("T").and_then(Value::as_dict).expect("nested /T");
    assert_eq!(nested.get("R"), Some(&Value::Name("C".into())));
    assert_eq!(
        nested.get("N").and_then(text_value),
        Some("Embedded document".into())
    );

    let action = action_dict(&doc, 1);
    assert_eq!(action.get("D"), Some(&Value::Integer(4)));
    assert!(action.get("NewWindow").is_none(), "缺省 /NewWindow 不物化");
    let target = action.get("T").and_then(Value::as_dict).expect("/T");
    assert_eq!(target.get("R"), Some(&Value::Name("C".into())));
    assert_eq!(
        target.get("P").and_then(text_value),
        Some("Chapter start".into())
    );
    assert_eq!(target.get("A"), Some(&Value::Integer(1)));
    assert!(target.get("N").is_none());

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::EmbeddedGoToAction));
}

#[test]
fn embedded_go_to_action_rejects_invalid_targets() {
    let cases = vec![
        // 既无 /F 也无 /T。
        GoToEAction::new(ActionRemoteDestination::Page(0)),
        // 名称树标题为空。
        GoToEAction::new(ActionRemoteDestination::Page(0))
            .with_target(GoToETarget::child_in_name_tree("")),
        GoToEAction::new(ActionRemoteDestination::Page(0))
            .with_target(GoToETarget::child_in_name_tree("   ")),
        // 页索引为负。
        GoToEAction::new(ActionRemoteDestination::Page(0)).with_target(
            GoToETarget::child_attachment(
                GoToETargetPage::Page(-1),
                GoToETargetAnnotation::Index(0),
            ),
        ),
        // 注解索引为负。
        GoToEAction::new(ActionRemoteDestination::Page(0)).with_target(
            GoToETarget::child_attachment(
                GoToETargetPage::Page(0),
                GoToETargetAnnotation::Index(-1),
            ),
        ),
        // 命名页与注解名为空。
        GoToEAction::new(ActionRemoteDestination::Page(0)).with_target(
            GoToETarget::child_attachment(
                GoToETargetPage::Named(String::new()),
                GoToETargetAnnotation::Index(0),
            ),
        ),
        GoToEAction::new(ActionRemoteDestination::Page(0)).with_target(
            GoToETarget::child_attachment(
                GoToETargetPage::Page(0),
                GoToETargetAnnotation::Name(" ".into()),
            ),
        ),
        // 嵌套目标同样校验。
        GoToEAction::new(ActionRemoteDestination::Page(0))
            .with_file(ActionFileSpec::new("a.pdf"))
            .with_target(GoToETarget::parent().with_target(GoToETarget::child_in_name_tree(""))),
    ];
    for action in cases {
        let mut out = Vec::new();
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, link_with_action(Action::GoToE(action.clone())));
        let error = b.build(&mut out).expect_err("invalid GoToE target");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{action:?}: {error}"
        );
        assert_eq!(out.len(), 0, "{action:?}");
    }
}

#[test]
fn embedded_go_to_action_requires_pdf_1_6() {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        link_with_action(Action::GoToE(
            GoToEAction::new(ActionRemoteDestination::Page(0))
                .with_file(ActionFileSpec::new("root.pdf")),
        )),
    );
    let error = b.build(&mut out).expect_err("GoToE action needs PDF 1.6");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(
        format!("{error}").contains("embedded go-to action"),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
