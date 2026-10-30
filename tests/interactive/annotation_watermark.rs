//! E2 Watermark 注解的固定打印字典 `/FixedPrint`（12.5.6.22，Table 190/191）
//! 写出 → 解析 → 回读往返。

use super::*;

/// 写入 → 重新解析：Watermark 的 `/FixedPrint` 直接字典经页面 `/Annots`
/// 回读为读端类型化字段，未声明的条目保持 `None`。
#[test]
fn builder_watermark_fixed_print_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, FixedPrint, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Watermark, [0.0, 0.0, 300.0, 200.0]).with_fixed_print(
            FixedPrint::new()
                .with_matrix([1.0, 0.0, 0.0, 1.0, 72.0, -72.0])
                .with_horizontal_offset(0.25)
                .with_vertical_offset(1.0),
        ),
    );
    // 只给平移量的第二个字典：`/Matrix` 缺省由渲染端按 Table 191 补单位矩阵。
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Watermark, [0.0, 0.0, 300.0, 200.0])
            .with_fixed_print(FixedPrint::new().with_vertical_offset(0.5)),
    );
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build watermark with /FixedPrint");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let first = &annotations[0];
    assert_eq!(first.subtype, AnnotationSubtype::Watermark);
    let fixed = first.fixed_print.as_ref().expect("/FixedPrint");
    assert_eq!(fixed.matrix, Some([1.0, 0.0, 0.0, 1.0, 72.0, -72.0]));
    assert_eq!(fixed.horizontal, Some(0.25));
    assert_eq!(fixed.vertical, Some(1.0));

    let second = &annotations[1];
    assert_eq!(second.subtype, AnnotationSubtype::Watermark);
    let fixed = second.fixed_print.as_ref().expect("/FixedPrint");
    assert_eq!(fixed.matrix, None);
    assert_eq!(fixed.horizontal, None);
    assert_eq!(fixed.vertical, Some(0.5));
}
