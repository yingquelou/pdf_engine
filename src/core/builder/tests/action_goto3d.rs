//! Go-To-3D-View 动作写端测试（12.6.4.15 Table 216）：`/TA` 目标与
//! `/V` 各形态的写出、目标注解与 `/VA` 相对校验、PDF 1.6 门禁。

use super::support::{annot_object, annot_object_with_num};
use super::*;
use crate::core::types::PdfFeature;

fn camera() -> [f64; 12] {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 30.0]
}

fn two_view_artwork() -> ThreeDArtwork {
    ThreeDArtwork::new(vec![1, 2, 3, 4])
        .with_view(
            ThreeDView::new("Front")
                .with_internal_name("front")
                .with_matrix_source(ThreeDMatrixSource::M)
                .with_camera_to_world(camera()),
        )
        .with_view(
            ThreeDView::new("Back")
                .with_internal_name("back")
                .with_matrix_source(ThreeDMatrixSource::M)
                .with_camera_to_world(camera()),
        )
}

fn three_d_annotation(handle: usize) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0]).with_three_d(handle)
}

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
fn go_to_3d_view_action_writes_target_and_view_forms() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(two_view_artwork());
    let target = b.add_annotation(p0, three_d_annotation(artwork));
    b.add_annotation(
        p0,
        link_with_action(Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Index(1),
        ))),
    );
    b.add_annotation(
        p0,
        link_with_action(Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Name("back".into()),
        ))),
    );
    b.add_annotation(
        p0,
        link_with_action(Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::First,
        ))),
    );

    let doc = b.assemble_document().expect("assemble GoTo3DView actions");
    let (target_num, target_dict) = annot_object_with_num(&doc, 3, 0);
    assert_eq!(target_dict.get("Subtype"), Some(&Value::Name("3D".into())));

    let index = action_dict(&doc, 1);
    assert_eq!(index.get("S"), Some(&Value::Name("GoTo3DView".into())));
    assert_eq!(index.get("TA"), Some(&Value::Ref(target_num, 0)));
    assert_eq!(index.get("V"), Some(&Value::Integer(1)));

    let name = action_dict(&doc, 2);
    assert_eq!(name.get("TA"), Some(&Value::Ref(target_num, 0)));
    assert_eq!(name.get("V"), Some(&Value::HexString("6261636B".into())));

    let first = action_dict(&doc, 3);
    assert_eq!(first.get("V"), Some(&Value::Name("F".into())));

    assert!(doc.features.borrow().contains(PdfFeature::GoTo3DViewAction));
}

/// 以两视图 artwork 为默认夹具构建并断言非法输入返回 `InvalidObject`。
fn expect_invalid(action: impl FnOnce(usize) -> Action) {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(two_view_artwork());
    let target = b.add_annotation(p0, three_d_annotation(artwork));
    b.add_annotation(p0, link_with_action(action(target)));
    let error = b.build(&mut out).expect_err("invalid GoTo3DView target");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error:?}");
    assert_eq!(out.len(), 0);
}

#[test]
fn go_to_3d_view_action_rejects_invalid_targets() {
    // 未知注解句柄。
    expect_invalid(|_| Action::GoTo3DView(GoTo3DViewAction::new(9, GoTo3DViewSelection::First)));
    // 目标注解不是 3D（句柄 1 是承载动作的 Link 注解本身）。
    expect_invalid(|target| {
        Action::GoTo3DView(GoTo3DViewAction::new(
            target + 1,
            GoTo3DViewSelection::First,
        ))
    });
    // `/VA` 下标越界。
    expect_invalid(|target| {
        Action::GoTo3DView(GoTo3DViewAction::new(target, GoTo3DViewSelection::Index(2)))
    });
    // 名称不匹配任何视图 `/IN`。
    expect_invalid(|target| {
        Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Name("top".into()),
        ))
    });
    // 内联视图缺 `/XN`。
    expect_invalid(|target| {
        Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Inline(Box::new(ThreeDView::new(""))),
        ))
    });
    // 内联视图 `/C2W` 缺 `/MS /M` 依赖。
    expect_invalid(|target| {
        Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Inline(Box::new(
                ThreeDView::new("Top").with_camera_to_world(camera()),
            )),
        ))
    });
}

#[test]
fn go_to_3d_view_action_requires_pdf_1_6() {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(two_view_artwork());
    let target = b.add_annotation(p0, three_d_annotation(artwork));
    b.add_annotation(
        p0,
        link_with_action(Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Default,
        ))),
    );
    let error = b
        .build(&mut out)
        .expect_err("GoTo3DView action needs PDF 1.6");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(
        format!("{error}").contains("go-to-3D-view actions"),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
