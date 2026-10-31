//! OPI 代理字典写端测试（ISO 32000-1 14.11.7 Table 368–370）。
//!
//! 覆盖图像与表单 XObject 的版本字典写出、直接对象（不额外分配对象
//! 号）、PDF 1.2 门禁与非法取值下的零字节契约。

use super::*;
use crate::core::content::ContentStream;
use crate::core::reader::opi::{
    Opi13Dictionary, Opi20Dictionary, OpiColor, OpiColorType, OpiInks, OpiProxy, OpiTagText,
};
use crate::core::types::{PdfFeature, PdfVersion};

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

fn empty_form(dictionary: OpiProxy) -> FormXObject {
    FormXObject::new([0.0, 0.0, 10.0, 10.0], ContentStream::new()).with_opi(dictionary)
}

/// 写端不回填原始字典；比较类型化条目时清空读端保留的 `dict`。
fn normalize(mut proxy: OpiProxy) -> OpiProxy {
    match &mut proxy {
        OpiProxy::V1_3(dictionary) => dictionary.dict = crate::core::types::Dict::new(),
        OpiProxy::V2_0(dictionary) => dictionary.dict = crate::core::types::Dict::new(),
    }
    proxy
}

fn assert_version_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("构建应失败");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

fn assert_form_rejected(form: FormXObject, expected: &str) {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&form);
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法表单参数应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn image_with_opi_writes_version_dictionary() {
    let proxy = v13_proxy();
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(
        p,
        0.0,
        0.0,
        10.0,
        10.0,
        &gray_image().with_opi(proxy.clone()),
    );

    let doc = b.assemble_document().expect("assemble image OPI");
    let value = doc
        .page_resource(0, "XObject", "Im1")
        .expect("page resource")
        .expect("image resource");
    let image = doc.image(&value).expect("image model");
    assert_eq!(
        image.opi.as_ref().map(|opi| normalize(opi.clone())),
        Some(proxy)
    );
    assert!(
        doc.features.borrow().contains(PdfFeature::OpiDictionaries),
        "写端应登记 OPI 特性"
    );

    // 版本字典为直接对象，不占用额外的间接对象号。
    let s = render(&mut b);
    assert!(s.contains("/OPI << /1.3 <<"), "{s}");
    assert!(s.contains("/F <686967687265732E746966>"), "{s}");
    assert!(s.contains("/Size [100 200]"), "{s}");
    assert!(s.contains("/CropRect [1 2 90 180]"), "{s}");
    assert!(
        s.contains("/Color [0.1 0.2 0.3 0.4 <4D6167656E7461>]"),
        "{s}"
    );
    assert_eq!(
        s.matches("/Subtype /Image").count(),
        1,
        "直接版本字典不应产生额外图像对象: {s}"
    );
}

#[test]
fn form_with_opi_writes_version_dictionary() {
    let proxy = v20_proxy();
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&empty_form(proxy.clone()));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    let doc = b.assemble_document().expect("assemble form OPI");
    let value = doc
        .page_resource(0, "XObject", "Fm1")
        .expect("page resource")
        .expect("form resource");
    let dictionary = doc.xobject_opi(&value).expect("form OPI").expect("present");
    assert_eq!(normalize(dictionary), proxy);
    assert!(
        doc.features.borrow().contains(PdfFeature::OpiDictionaries),
        "写端应登记 OPI 特性"
    );

    let s = render(&mut b);
    assert!(s.contains("/OPI << /2.0 <<"), "{s}");
    assert!(
        s.contains("/Inks [/monochrome <4379616E> 0.5 <4D6167656E7461> 1.0]"),
        "{s}"
    );
    assert_eq!(
        s.matches("/Subtype /Form").count(),
        1,
        "直接版本字典不应产生额外表单对象: {s}"
    );
}

#[test]
fn opi_requires_pdf_1_2() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &gray_image().with_opi(v13_proxy()));
    assert_version_error(&mut b, "OPI dictionaries");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&empty_form(v20_proxy()));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_version_error(&mut b, "OPI dictionaries");

    // PDF 1.2 恰好满足最低版本，可正常写出。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &gray_image().with_opi(v13_proxy()));
    let s = render(&mut b);
    assert!(s.contains("/OPI << /1.3 <<"), "{s}");
}

#[test]
fn invalid_opi_values_are_rejected_without_output() {
    // 1.3 必填 /F 为空。
    assert_image_rejected(
        gray_image().with_opi(OpiProxy::v1_3(Opi13Dictionary::new(
            "",
            [10, 10],
            [0, 0, 10, 10],
            [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0],
        ))),
        "/F must not be empty",
    );

    // 1.3 裁切框超出代理尺寸。
    assert_image_rejected(
        gray_image().with_opi(OpiProxy::v1_3(Opi13Dictionary::new(
            "highres.tif",
            [10, 10],
            [0, 0, 20, 20],
            [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0],
        ))),
        "must lie within /Size",
    );

    // 1.3 GrayMap 长度与 2^bits 不符。
    assert_image_rejected(
        gray_image().with_opi(OpiProxy::v1_3(
            Opi13Dictionary::new(
                "highres.tif",
                [10, 10],
                [0, 0, 10, 10],
                [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0],
            )
            .with_image_type([8, 2])
            .with_gray_map(vec![0]),
        )),
        "/GrayMap must contain",
    );

    // 2.0 /Size 与 /CropRect 必须成对。
    let mut unpaired = Opi20Dictionary::new("proxy.jpg");
    unpaired.size = Some([10.0, 10.0]);
    assert_form_rejected(empty_form(OpiProxy::v2_0(unpaired)), "must both be present");

    // 2.0 /IncludedImageQuality 只接受 1、2、3。
    assert_form_rejected(
        empty_form(OpiProxy::v2_0(
            Opi20Dictionary::new("proxy.jpg").with_included_image_quality(4),
        )),
        "/IncludedImageQuality must be 1, 2 or 3",
    );
}
