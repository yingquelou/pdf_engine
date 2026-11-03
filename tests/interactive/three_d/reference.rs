//! 3D artwork 引用字典（13.6.3.1 Table 303）与 `/OnInstantiate` JavaScript 流的写出→解析往返。

use super::*;

/// 启用引用字典（Table 303）与 `/OnInstantiate` JavaScript 流的 3D
/// artwork：两个注解共享引用字典与运行实例，脚本流经间接引用回读。
#[test]
fn builder_three_d_reference_and_on_instantiate_roundtrip() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, ThreeDArtwork,
    };
    use pdf_engine::core::types::StreamData;

    const SCRIPT: &[u8] = b"app.alert('3D');";
    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let artwork = builder.add_three_d_artwork(
        ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00])
            .with_reference_dictionary()
            .with_on_instantiate(SCRIPT.to_vec()),
    );
    for y in [600.0, 400.0] {
        builder.add_annotation(
            p0,
            PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, y, 200.0, y + 80.0])
                .with_three_d(artwork),
        );
    }

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build 3D reference dictionary and /OnInstantiate");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let first = annotations[0].three_d.as_ref().expect("/3DD");
    let second = annotations[1].three_d.as_ref().expect("/3DD");
    let reference = first.reference.expect("reference dictionary");
    assert_eq!(second.reference, Some(reference));
    let source = first.source.expect("3D stream reference");
    assert_eq!(second.source, Some(source));
    assert_ne!(reference, source, "reference dictionary wraps the stream");

    let script_num = match first.stream.on_instantiate.as_ref() {
        Some(Value::Ref(num, 0)) => *num,
        other => panic!("expected /OnInstantiate reference, got {other:?}"),
    };
    let object = doc
        .get_object(script_num)
        .expect("OnInstantiate object")
        .expect("OnInstantiate exists");
    let Value::Stream(_, data) = &object.value else {
        panic!("expected /OnInstantiate stream, got {:?}", object.value);
    };
    assert!(
        matches!(
            data,
            StreamData::Ref { .. } | StreamData::EncryptedRef { .. }
        ),
        "parsed streams reference file bytes, not owned copies"
    );
    assert_eq!(
        doc.stream_bytes(&object.value).expect("script bytes"),
        SCRIPT
    );
}
