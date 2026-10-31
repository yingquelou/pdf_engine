//! Trap 网络注解（14.11.6，Table 366/367）写出 → 解析 → 回读往返：
//! `/F` 恒为 68、`/LastModified` 或 `/Version` + `/AnnotStates` 跟踪条目，
//! 以及 `/AP /N` 表单的 `/PCM` `/SeparationColorNames` `/TrapRegions`
//! `/TrapStyles` `/FontFauxing` 条目。

use super::*;

#[test]
fn trap_network_annotation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, FormXObject, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::annotation::{AnnotationSubtype, ProcessColorModel};
    use pdf_engine::core::types::{PdfDate, PdfFeature, PdfVersion};

    let date = PdfDate::parse("D:20240101120000Z").expect("valid PDF date");
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = builder.page(PageSize::A4);
    let network = builder.form_xobject(
        &FormXObject::new([0.0, 0.0, 50.0, 50.0], ContentStream::new())
            .with_pcm(ProcessColorModel::DeviceCMYK)
            .with_separation_color_name("Spot")
            .with_trap_region(1)
            .with_trap_styles("Red:0.0,Blue:0.0")
            .with_font_fauxing(Dict::new().insert("Type", Value::Name("Font".into()))),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::TrapNet, [0.0, 0.0, 50.0, 50.0])
            .with_trap_network_last_modified(date)
            .with_appearance(network),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build trap network annotation");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    {
        let features = doc.features.borrow();
        assert!(
            features.contains(PdfFeature::TrapNetworks),
            "trap networks 应进入特性画像"
        );
        assert!(
            features.contains(PdfFeature::TrapNetworkModificationDate),
            "/LastModified 应登记 PDF 1.4 条目"
        );
    }

    let annot = &annotations[0];
    assert_eq!(annot.subtype, AnnotationSubtype::TrapNet);
    assert!(annot.flags.print(), "Table 366 要求 Print 置位");
    assert!(annot.flags.read_only(), "Table 366 要求 ReadOnly 置位");
    let trap = annot.trap_network.as_ref().expect("/LastModified and form");
    assert_eq!(trap.last_modified, Some(date));
    assert!(trap.version.is_none());
    assert!(trap.annot_states.is_none());
    assert!(trap.form.reference.is_some());
    assert_eq!(trap.form.pcm, ProcessColorModel::DeviceCMYK);
    assert_eq!(trap.form.separation_color_names, vec!["Spot"]);
    assert_eq!(trap.form.trap_regions, vec![(1, 0)]);
    assert_eq!(trap.form.trap_styles.as_deref(), Some("Red:0.0,Blue:0.0"));
    assert_eq!(trap.form.font_fauxing.len(), 1);
}

#[test]
fn trap_network_version_tracking_roundtrips() {
    use pdf_engine::core::builder::{
        AnnotationKind, FormXObject, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::annotation::{AnnotationSubtype, ProcessColorModel};
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = builder.page(PageSize::A4);
    let network = builder.form_xobject(
        &FormXObject::new([0.0, 0.0, 50.0, 50.0], ContentStream::new())
            .with_pcm(ProcessColorModel::DeviceRGB),
    );
    // `/AnnotStates` 与页面其余注解一一对应，TrapNet 自身不占条目。
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::TrapNet, [0.0, 0.0, 50.0, 50.0])
            .with_trap_network_version(vec![Value::Ref(1, 0)], vec![Some("On".into()), None])
            .with_appearance(network),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build trap network version tracking");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);
    {
        let features = doc.features.borrow();
        assert!(features.contains(PdfFeature::TrapNetworks));
        assert!(
            !features.contains(PdfFeature::TrapNetworkModificationDate),
            "无 /LastModified 时不应登记 PDF 1.4 条目"
        );
    }

    // Trap 网络必须位于 `/Annots` 末尾。
    let annot = &annotations[1];
    assert_eq!(annot.subtype, AnnotationSubtype::TrapNet);
    assert!(annot.flags.print() && annot.flags.read_only());
    let trap = annot
        .trap_network
        .as_ref()
        .expect("/Version and /AnnotStates");
    assert!(trap.last_modified.is_none());
    assert_eq!(trap.version, Some(vec![Value::Ref(1, 0)]));
    assert_eq!(
        trap.annot_states,
        Some(vec![Some("On".into()), None]),
        "null 状态应回读为 None"
    );
    assert_eq!(trap.form.pcm, ProcessColorModel::DeviceRGB);
}
