//! 3D 流 `/Resources` 名称树（13.6.3.1 Table 300）的写出→解析往返。

use super::*;

/// 3D 流 `/Resources` 名称树（13.6.3.1 Table 300）写出 → 解析 → 回读：
/// 名称按文本串解码，直接对象值原样保留。
#[test]
fn builder_three_d_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, ThreeDArtwork,
    };
    use pdf_engine::core::types::{Dict, PdfVersion};
    use std::collections::BTreeMap;

    let mut resources = BTreeMap::new();
    resources.insert(
        "mode".to_string(),
        Value::Dict(
            Dict::new()
                .insert("Mode", Value::Name("Turntable".into()))
                .insert("Speed", Value::Integer(2)),
        ),
    );
    resources.insert(
        "é".to_string(),
        Value::Dict(Dict::new().insert("Mask", Value::Name("Wheel".into()))),
    );

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    let artwork = builder.add_three_d_artwork(
        ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]).with_resources(resources.clone()),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0])
            .with_three_d(artwork),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build 3D /Resources");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    assert_eq!(
        annotations[0]
            .three_d
            .as_ref()
            .expect("/3DD")
            .stream
            .resources,
        Some(resources),
        "名称树键按文本串解码，值直接往返"
    );
}
