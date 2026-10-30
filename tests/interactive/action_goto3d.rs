//! Go-To-3D-View 动作（12.6.4.15 Table 216）的写出 → 解析 → 回读往返。

use super::*;

const CAMERA: [f64; 12] = [
    1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 10.0, 20.0, 30.0,
];

/// 写入 → 重新解析：`/TA` 指向 3D 注解对象，`/V` 的下标、`/IN` 名称与
/// 内联视图字典经读端还原。
#[test]
fn builder_go_to_3d_view_action_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, GoTo3DViewAction, GoTo3DViewSelection, PageSize, PdfAnnotation,
        PdfBuilder, ThreeDArtwork, ThreeDMatrixSource, ThreeDView,
    };
    use pdf_engine::core::reader::action::{
        Action as ReadAction, GoTo3DViewSelection as ReadSelection,
    };
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    let artwork = builder.add_three_d_artwork(
        ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00])
            .with_view(
                ThreeDView::new("Front")
                    .with_internal_name("front")
                    .with_matrix_source(ThreeDMatrixSource::M)
                    .with_camera_to_world(CAMERA),
            )
            .with_view(
                ThreeDView::new("Back")
                    .with_internal_name("back")
                    .with_matrix_source(ThreeDMatrixSource::U3D)
                    .with_u3d_path(vec!["Back".into()]),
            ),
    );
    let target = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0])
            .with_three_d(artwork),
    );
    let link = |action: Action| {
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(action)
    };
    builder.add_annotation(
        p0,
        link(Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Index(1),
        ))),
    );
    builder.add_annotation(
        p0,
        link(Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Name("back".into()),
        ))),
    );
    builder.add_annotation(
        p0,
        link(Action::GoTo3DView(GoTo3DViewAction::new(
            target,
            GoTo3DViewSelection::Inline(Box::new(
                ThreeDView::new("Inline")
                    .with_internal_name("inline")
                    .with_matrix_source(ThreeDMatrixSource::M)
                    .with_camera_to_world(CAMERA),
            )),
        ))),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build GoTo3DView actions");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 4);
    let three_d = annotations[0].reference.expect("3D annotation reference");
    assert_eq!(annotations[0].subtype, AnnotationSubtype::ThreeD);

    let action = |index: usize| {
        doc.action(annotations[index].action.as_ref().expect("/A"))
            .expect("parse GoTo3DView action")
    };

    match action(1) {
        ReadAction::GoTo3DView(action) => {
            assert_eq!(action.target, three_d);
            assert_eq!(action.view, ReadSelection::Index(1));
        }
        other => panic!("unexpected action: {other:?}"),
    }

    match action(2) {
        ReadAction::GoTo3DView(action) => {
            assert_eq!(action.target, three_d);
            assert_eq!(
                action.view,
                ReadSelection::Name(Value::HexString("6261636B".into()))
            );
        }
        other => panic!("unexpected action: {other:?}"),
    }

    match action(3) {
        ReadAction::GoTo3DView(action) => {
            assert_eq!(action.target, three_d);
            let ReadSelection::Inline(view) = action.view else {
                panic!("expected inline view, got {:?}", action.view);
            };
            assert_eq!(view.external_name, Value::HexString("496E6C696E65".into()));
            assert_eq!(
                view.internal_name,
                Some(Value::HexString("696E6C696E65".into()))
            );
            assert_eq!(view.matrix_source, Some(ThreeDMatrixSource::M));
            assert_eq!(view.camera_to_world, Some(CAMERA));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}
