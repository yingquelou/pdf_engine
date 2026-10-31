//! 分色字典（ISO 32000-1 14.11.4 Table 364）写出 → 解析 → 回读往返。

use super::*;

/// `/Pages` 节点（对象 2）的 `/Kids` 引用序列。
fn page_refs(doc: &pdf_engine::core::types::Document) -> Vec<(i64, i64)> {
    let pages = doc
        .get_object(2)
        .expect("pages lookup")
        .expect("pages exists")
        .value
        .as_dict()
        .expect("pages dict")
        .clone();
    pages
        .get("Kids")
        .expect("/Kids")
        .as_array()
        .expect("/Kids array")
        .iter()
        .map(|kid| kid.as_ref().expect("page reference"))
        .collect()
}

/// 两页共享同一组分色字典：解析后两页的 `/Pages`、色料名与 Separation
/// 颜色空间一致，且引用恰为解析后的页对象。
#[test]
fn builder_separation_info_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, DeviceColorant, ExponentialFunction, Function, PageSize, PdfBuilder,
        Separation, SeparationInfo,
    };
    use pdf_engine::core::reader::color_space::ColorSpaceModel;
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder.add_separation_group(
        &[0, 1],
        SeparationInfo::new(DeviceColorant::name("Cyan")).with_color_space(ColorSpace::Separation(
            Separation::new(
                "Cyan",
                ColorSpace::DeviceGray,
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
            ),
        )),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build separation info");
    let doc = parse_buf(&buf);

    let first = doc
        .page_separation_info(0)
        .expect("separation info 0")
        .expect("declared");
    let second = doc
        .page_separation_info(1)
        .expect("separation info 1")
        .expect("declared");
    assert_eq!(first.device_colorant.as_str(), "Cyan");
    assert_eq!(second.device_colorant.as_str(), "Cyan");
    assert_eq!(first.pages, second.pages, "同组页共享相同的 /Pages");
    assert_eq!(
        first.pages,
        page_refs(&doc),
        "/Pages 应恰为解析后的页对象引用"
    );
    match &first.color_space {
        Some(ColorSpaceModel::Separation(space)) => assert_eq!(space.colorant, "Cyan"),
        other => panic!("expected Separation colour space, got {other:?}"),
    }
    assert_eq!(second.color_space, first.color_space);
    assert_eq!(
        first.dict.get("DeviceColorant"),
        Some(&Value::Name("Cyan".into()))
    );
    assert!(
        doc.features.borrow().contains(PdfFeature::SeparationInfo),
        "解析后特性画像应含 separation dictionaries"
    );
}
