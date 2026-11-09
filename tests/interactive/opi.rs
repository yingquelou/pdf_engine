//! OPI 代理字典（ISO 32000-1 14.11.7 Table 368–370）写出 → 解析 → 回读往返。

use super::*;

use pdf_engine::core::builder::{
    ColorSpace, FormXObject, ImageData, Opi13Dictionary, Opi20Dictionary, OpiColor, OpiColorType,
    OpiInks, OpiProxy, OpiTagText, PageSize, PdfBuilder,
};
use pdf_engine::core::content::ContentStream;
use pdf_engine::core::reader::opi::OpiVersion;
use pdf_engine::core::types::{Dict, PdfFeature, PdfVersion};

fn gray_image() -> ImageData {
    ImageData {
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
}

fn v13_proxy() -> OpiProxy {
    OpiProxy::v1_3(
        Opi13Dictionary::new(
            "highres.tif",
            [100, 200],
            [1, 2, 90, 180],
            [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0],
        )
        .with_type_entry()
        .with_id([0_u8, 0xFF])
        .with_comments("proxy comment")
        .with_crop_fixed([1.5, 2.5, 89.5, 179.5])
        .with_resolution([300.0, 300.0])
        .with_color_type(OpiColorType::Process)
        .with_color(OpiColor {
            cmyk: [0.1, 0.2, 0.3, 0.4],
            color_name: b"Magenta".to_vec(),
        })
        .with_tint(0.5)
        .with_overprint(true)
        .with_image_type([8, 2])
        .with_gray_map(vec![0, 1, 2, 65535])
        .with_transparency(false)
        .with_tag(1, "tag-a"),
    )
}

fn v20_proxy() -> OpiProxy {
    OpiProxy::v2_0(
        Opi20Dictionary::new("proxy.jpg")
            .with_type_entry()
            .with_main_image("main.tif")
            .with_tag(7, OpiTagText::Text("single".into()))
            .with_tag(9, OpiTagText::Texts(vec!["first".into(), "second".into()]))
            .with_size_and_crop_rect([200.0, 100.0], [10.0, 20.0, 190.0, 90.0])
            .with_overprint(true)
            .with_inks(OpiInks::Monochrome(vec![
                (b"Cyan".to_vec(), 0.5),
                (b"Magenta".to_vec(), 1.0),
            ]))
            .with_included_image_dimensions([4000, 2000])
            .with_included_image_quality(3),
    )
}

/// 读端保留原始字典，写端不回填；比较全部类型化字段前清空该保留字段。
fn normalize(mut proxy: OpiProxy) -> OpiProxy {
    match &mut proxy {
        OpiProxy::V1_3(dictionary) => dictionary.dict = Dict::new(),
        OpiProxy::V2_0(dictionary) => dictionary.dict = Dict::new(),
    }
    proxy
}

#[test]
fn builder_opi_proxies_roundtrip_through_parser() {
    let image_proxy = v13_proxy();
    let form_proxy = v20_proxy();

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    let page = builder.page(PageSize::A4);
    builder.image(
        page,
        0.0,
        0.0,
        10.0,
        10.0,
        &gray_image().with_opi(image_proxy.clone()),
    );
    let form =
        FormXObject::new([0.0, 0.0, 10.0, 10.0], ContentStream::new()).with_opi(form_proxy.clone());
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 20.0, 0.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build OPI proxies");
    let doc = parse_buf(&buf);

    // 页面图像资源 /Im1 → 图像 XObject 的 /OPI。
    let image = doc
        .page_image(0, "Im1")
        .expect("page image")
        .expect("image present");
    let image_opi = image.opi.expect("image OPI");
    assert_eq!(image_opi.version(), OpiVersion::V1_3);
    assert!(!image_opi.dict().0.is_empty(), "读端应保留原始 OPI 字典");
    assert_eq!(normalize(image_opi), image_proxy);

    // 页面表单资源 /Fm1 → 表单 XObject 的 /OPI。
    let form_value = doc
        .page_resource(0, "XObject", "Fm1")
        .expect("page resource")
        .expect("form present");
    let form_opi = doc
        .xobject_opi(&form_value)
        .expect("form OPI")
        .expect("declared");
    assert_eq!(form_opi.version(), OpiVersion::V2_0);
    assert!(!form_opi.dict().0.is_empty(), "读端应保留原始 OPI 2.0 字典");
    assert_eq!(normalize(form_opi), form_proxy);

    assert!(
        doc.features.borrow().contains(PdfFeature::OpiDictionaries),
        "解析后特性画像应含 OPI dictionaries"
    );
}
