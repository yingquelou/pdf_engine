//! 3D 视图 PDF 1.7 条目（13.6.4.1 Table 304）写端：`/RM` `/LS` `/SA`
//! `/NA` `/NR` 的版本门禁与 `/NA` → `/NR` 依赖校验。来源覆盖 artwork
//! `/VA`、内联 `/DV`、注解内联 `/3DV` 与 GoTo3DView 内联 `/V`。

use super::support::{annot_object, assert_text_entry};
use super::*;
use crate::core::types::{Array, Dict, Document, PdfFeature, Value};

fn camera() -> [f64; 12] {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 30.0]
}

/// 带 `/MS /M` 与 `/C2W` 的合法视图。
fn camera_view(name: &str) -> ThreeDView {
    ThreeDView::new(name)
        .with_matrix_source(ThreeDMatrixSource::M)
        .with_camera_to_world(camera())
}

fn three_d_annotation(handle: usize) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0]).with_three_d(handle)
}

/// `/RM` 渲染模式字典（`Solid`，其余条目缺省）。
fn render_mode() -> ThreeDRenderMode {
    ThreeDRenderMode::new(ThreeDRenderModeType::Solid)
}

/// `/NA` 节点字典数组。
fn nodes() -> Vec<ThreeDNode> {
    vec![ThreeDNode::new("Wheel")]
}

/// 取条目中的间接引用对象号；缺失或非引用即 panic。
fn referenced_num(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Ref(num, 0)) => *num,
        other => panic!("expected indirect reference, got {other:?}"),
    }
}

/// 取 3D 流对象 `/VA[0]` 的视图字典。
fn first_view(doc: &Document, stream_num: i64) -> Dict {
    let object = doc
        .get_object(stream_num)
        .expect("3D stream object")
        .expect("3D stream exists");
    let Value::Stream(dict, _) = &object.value else {
        panic!(
            "expected 3D stream at object {stream_num}, got {:?}",
            object.value
        );
    };
    dict.get("VA")
        .and_then(Value::as_array)
        .and_then(|views| views.0.first())
        .and_then(Value::as_dict)
        .cloned()
        .expect("/VA[0] view dictionary")
}

#[test]
fn three_d_view_pdf17_entries_require_pdf_1_7() {
    // `/VA` 视图的 `/RM`：PDF 1.6 拒绝且零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(
        ThreeDArtwork::new(vec![1, 2, 3, 4])
            .with_view(camera_view("Front").with_render_mode(render_mode())),
    );
    b.add_annotation(p0, three_d_annotation(artwork));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.6 must reject 3D view /RM");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(
        error.to_string().contains("3D view PDF 1.7 entries"),
        "{error}"
    );
    assert!(out.is_empty(), "build failure must not emit bytes");

    // PDF 1.7 写出 `/VA[0] /RM` 并登记特性。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(
        ThreeDArtwork::new(vec![1, 2, 3, 4])
            .with_view(camera_view("Front").with_render_mode(render_mode())),
    );
    b.add_annotation(p0, three_d_annotation(artwork));
    let doc = b.assemble_document().expect("assemble 3D view /RM");
    let annot = annot_object(&doc, 3, 0);
    let stream_num = referenced_num(annot.get("3DD"));
    let view = first_view(&doc, stream_num);
    assert_eq!(view.get("RM"), Some(&Value::Dict(render_mode().to_dict())));
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ThreeDViewExtensions)
    );

    // 内联 `/DV` 的 `/LS`：1.6 同样拒绝（扫描含 `/DV` 路径）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(
        ThreeDArtwork::new(vec![1, 2, 3, 4]).with_default_view(ThreeDViewSelection::Inline(
            Box::new(
                camera_view("Default")
                    .with_lighting(ThreeDLightingScheme::new(ThreeDLightingSchemeType::Artwork)),
            ),
        )),
    );
    b.add_annotation(p0, three_d_annotation(artwork));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.6 must reject inline /DV /LS");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "build failure must not emit bytes");
}

#[test]
fn three_d_view_nodes_require_restore_nodes() {
    // `/NA` 缺 `/NR`：拒绝且零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(
        ThreeDArtwork::new(vec![1, 2, 3, 4]).with_view(camera_view("Front").with_nodes(nodes())),
    );
    b.add_annotation(p0, three_d_annotation(artwork));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("3D view /NA without /NR must be rejected");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("3D view /NA is meaningful only if /NR is present (Table 304)"),
        "{error}"
    );
    assert!(out.is_empty(), "build failure must not emit bytes");

    // 补充 `/NR true` 后可构建，`/NA` 与 `/NR` 都写出。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(
        ThreeDArtwork::new(vec![1, 2, 3, 4]).with_view(
            camera_view("Front")
                .with_nodes(nodes())
                .with_restore_nodes(true),
        ),
    );
    b.add_annotation(p0, three_d_annotation(artwork));
    let mut out = Vec::new();
    b.build(&mut out).expect("3D view with /NA and /NR builds");
    assert!(!out.is_empty(), "successful build must emit bytes");

    let doc = b.assemble_document().expect("assemble 3D view /NA");
    let annot = annot_object(&doc, 3, 0);
    let stream_num = referenced_num(annot.get("3DD"));
    let view = first_view(&doc, stream_num);
    let na = view.get("NA").and_then(Value::as_array).expect("/NA array");
    assert_eq!(na.0.len(), 1);
    let node = na.0[0].as_dict().expect("/NA[0] dictionary");
    assert_eq!(node.get("Type"), Some(&Value::Name("3DNode".into())));
    assert_text_entry(node, "N");
    assert_eq!(view.get("NR"), Some(&Value::Bool(true)));
}

#[test]
fn annotation_inline_view_pdf17_entries_require_pdf_1_7() {
    // 注解内联 `/3DV` 的 `/SA`（空数组）：1.6 拒绝且零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(ThreeDArtwork::new(vec![1, 2, 3, 4]));
    b.add_annotation(
        p0,
        three_d_annotation(artwork).with_three_d_view(ThreeDViewSelection::Inline(Box::new(
            camera_view("Inline").with_cross_sections(Vec::new()),
        ))),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.6 must reject inline /3DV /SA");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "build failure must not emit bytes");

    // PDF 1.7 写出 `/3DV /SA` 并登记特性。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(ThreeDArtwork::new(vec![1, 2, 3, 4]));
    b.add_annotation(
        p0,
        three_d_annotation(artwork).with_three_d_view(ThreeDViewSelection::Inline(Box::new(
            camera_view("Inline").with_cross_sections(Vec::new()),
        ))),
    );
    let doc = b.assemble_document().expect("assemble inline /3DV /SA");
    let annot = annot_object(&doc, 3, 0);
    let view = annot
        .get("3DV")
        .and_then(Value::as_dict)
        .expect("annotation /3DV dictionary");
    assert_eq!(view.get("SA"), Some(&Value::Array(Array(Vec::new()))));
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ThreeDViewExtensions)
    );

    // GoTo3DView 内联 `/V` 的 `/RM`：1.6 同样拒绝。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    let artwork =
        b.add_three_d_artwork(ThreeDArtwork::new(vec![1, 2, 3, 4]).with_view(camera_view("Front")));
    let target = b.add_annotation(p0, three_d_annotation(artwork));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(
            Action::GoTo3DView(GoTo3DViewAction::new(
                target,
                GoTo3DViewSelection::Inline(Box::new(
                    camera_view("Top").with_render_mode(render_mode()),
                )),
            )),
        ),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.6 must reject GoTo3DView inline /V /RM");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "build failure must not emit bytes");
}
