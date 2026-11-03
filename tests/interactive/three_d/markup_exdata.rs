//! `/ExData` 的 Markup3D 载荷（13.6.6 Table 313）的写出→解析往返。

use super::*;

/// `/ExData` 的 Markup3D 载荷（13.6.6 Table 313）写出 → 解析 → 回读：
/// 标记注解经 `/3DA` 关联 3D 注解，`/3DV` 内联视图与 `/MD5` 按类型读回，
/// 特性画像记录 PDF 1.7 的 `ThreeDMarkup`。
#[test]
fn builder_markup_3d_exdata_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, Markup3D as BuilderMarkup3D, PageSize, PdfAnnotation, PdfBuilder,
        ThreeDArtwork, ThreeDView as BuilderView,
    };
    use pdf_engine::core::reader::annotation::{ExDataSubtype, Markup3DAnnotation};
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    let artwork = builder.add_three_d_artwork(ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0])
            .with_three_d(artwork),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FreeText, [72.0, 400.0, 200.0, 500.0])
            .with_default_appearance("/F1 12 Tf 0 g")
            .with_markup_3d(
                BuilderMarkup3D::new(
                    artwork,
                    BuilderView::new("CommentView1").with_internal_name("comment"),
                )
                .with_md5([0xAB; 16]),
            ),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build markup 3D /ExData");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let three_d_num = annotations[0]
        .reference
        .expect("3D annotation indirect object")
        .0;
    let ex_data = annotations[1].ex_data.as_ref().expect("/ExData");
    assert_eq!(ex_data.subtype, ExDataSubtype::Markup3D);
    let markup = ex_data
        .markup_3d
        .as_ref()
        .expect("/Subtype /Markup3D payload");
    assert_eq!(
        markup.annotation,
        Markup3DAnnotation::Reference {
            number: three_d_num,
            generation: 0,
        }
    );
    // 写端文本串统一按大写十六进制编码。
    assert_eq!(
        markup.view.external_name,
        Value::HexString("436F6D6D656E745669657731".into())
    );
    assert_eq!(
        markup.view.internal_name,
        Some(Value::HexString("636F6D6D656E74".into()))
    );
    assert_eq!(markup.md5, Some([0xAB; 16]));
    assert!(
        doc.features.borrow().contains(PdfFeature::ThreeDMarkup),
        "parsed feature profile should include 3D markup"
    );
}
