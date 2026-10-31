//! Printer's mark 注解（14.11.3，Table 362/363）写出 → 解析 → 回读往返：
//! `/F` 恒为 68、`/MN` 名称、`/AP`/`/AS` 选定的表单与 `/MarkStyle`
//! `/Colorants` 条目。

use super::*;

#[test]
fn builder_printer_mark_annotation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, ColorSpace, ExponentialFunction, FormXObject, Function, PageSize,
        PdfAnnotation, PdfBuilder, Separation,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = builder.page(PageSize::A4);
    let cyan = ColorSpace::Separation(Separation::new(
        "Cyan",
        ColorSpace::DeviceGray,
        Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
    ));
    let registration = builder.form_xobject(
        &FormXObject::new([0.0, 0.0, 50.0, 50.0], ContentStream::new())
            .with_mark_style("Cut")
            .with_colorant("Cyan", cyan.clone()),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::PrinterMark, [0.0, 0.0, 50.0, 50.0])
            .with_printer_mark_name("Registration")
            .with_appearance(registration),
    );

    let on = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 50.0, 50.0],
        ContentStream::new(),
    ));
    let off = builder.form_xobject(
        &FormXObject::new([0.0, 0.0, 50.0, 50.0], ContentStream::new())
            .with_mark_style("Bleed")
            .with_colorant("Cyan", cyan),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::PrinterMark, [60.0, 0.0, 110.0, 50.0])
            .with_printer_mark_name("ColorBar")
            .with_appearance_state("On", on)
            .with_appearance_state("Off", off)
            .with_selected_appearance_state("Off"),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build printer's mark annotations");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);
    // 注解与其外观表单已按需解析，扫描到 `/Subtype /PrinterMark`。
    assert!(
        doc.features.borrow().contains(PdfFeature::PrinterMarks),
        "printer's marks 应进入特性画像"
    );

    let first = &annotations[0];
    assert_eq!(first.subtype, AnnotationSubtype::PrinterMark);
    assert!(first.flags.print(), "Table 362 要求 Print 置位");
    assert!(first.flags.read_only(), "Table 362 要求 ReadOnly 置位");
    let mark = first.printer_mark.as_ref().expect("/MN and form entries");
    assert_eq!(mark.mark_name.as_deref(), Some("Registration"));
    assert!(mark.form.reference.is_some());
    assert_eq!(mark.form.mark_style.as_deref(), Some("Cut"));
    assert_eq!(mark.form.colorants["Cyan"].colorant, "Cyan");

    let second = &annotations[1];
    assert_eq!(second.subtype, AnnotationSubtype::PrinterMark);
    assert!(second.flags.print() && second.flags.read_only());
    let mark = second.printer_mark.as_ref().expect("/MN and form entries");
    assert_eq!(mark.mark_name.as_deref(), Some("ColorBar"));
    assert_eq!(
        mark.form.mark_style.as_deref(),
        Some("Bleed"),
        "/AS 应选中 /Off 状态的外观表单"
    );
    assert_eq!(mark.form.colorants["Cyan"].colorant, "Cyan");
}
