//! Form XObject 自有 `/Resources` 与表单级 `/OC`（D7）。

use super::*;
use crate::core::content::ContentStream;
use crate::core::types::PdfVersion;

fn rgb(width: u32, height: u32, fill: u8) -> ImageData {
    ImageData {
        width,
        height,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![fill; (width * height * 3) as usize],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
    }
}

fn form_with_resources() -> FormXObject {
    let mut content = ContentStream::new();
    content
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 40.0)
        .show_text("Hi")
        .end_text();
    content
        .save_state()
        .concat_matrix([20.0, 0.0, 0.0, 20.0, 10.0, 10.0])
        .draw_xobject("Im1")
        .restore_state();
    FormXObject::new([0.0, 0.0, 100.0, 50.0], content)
}

fn resources_of() -> FormResources {
    FormResources::new()
        .with_font(Font::helvetica(12.0))
        .with_image(rgb(1, 1, 128))
}

/// 找出唯一的 Form XObject 流对象字典（测试里的文档最多一张表单）。
fn form_dict(doc: &Document) -> Dict {
    for num in 1..=64 {
        if let Some(object) = doc.get_object(num).expect("object lookup")
            && let Value::Stream(dict, _) = &object.value
            && dict.get("Subtype").and_then(Value::as_name) == Some("Form")
        {
            return dict.clone();
        }
    }
    panic!("form XObject object not found");
}

fn page_resources(doc: &Document) -> Dict {
    let page = doc
        .get_object(3)
        .expect("page lookup")
        .expect("page object")
        .value
        .as_dict()
        .expect("page dictionary")
        .clone();
    page.get("Resources")
        .and_then(Value::as_dict)
        .expect("page /Resources")
        .clone()
}

fn assert_build_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("构建应失败");
    assert!(
        error.to_string().contains(expected),
        "error {error:?} should mention {expected:?}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn form_own_resources_write_font_and_image_dictionaries() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&form_with_resources().with_resources(resources_of()));
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let doc = b.assemble_document().expect("assemble");

    let dict = form_dict(&doc);
    let resources = dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("表单应写出 /Resources");
    let font_dict = resources
        .get("Font")
        .and_then(Value::as_dict)
        .expect("/Font 子字典");
    let font_ref = font_dict
        .get("F1")
        .and_then(Value::as_ref)
        .expect("表单字体 /F1 应为对象引用");
    let xobj_dict = resources
        .get("XObject")
        .and_then(Value::as_dict)
        .expect("/XObject 子字典");
    let image_ref = xobj_dict
        .get("Im1")
        .and_then(Value::as_ref)
        .expect("表单图片 /Im1 应为对象引用");

    // 表单字体与图片确实写出为对象。
    let font_object = doc
        .get_object(font_ref.0)
        .expect("font lookup")
        .expect("font object");
    assert_eq!(
        font_object.value.as_dict().and_then(|d| d.get("Type")),
        Some(&Value::Name("Font".into()))
    );
    let image_object = doc
        .get_object(image_ref.0)
        .expect("image lookup")
        .expect("image object");
    assert!(matches!(image_object.value, Value::Stream(_, _)));

    // 页面只登记表单资源名 /Fm1，表单字体与图片不应泄漏到页面。
    let page = page_resources(&doc);
    assert!(page.get("Font").is_none(), "页面不应获得表单字体: {page:?}");
    let page_xobjects = page
        .get("XObject")
        .and_then(Value::as_dict)
        .expect("页面 /XObject 子字典");
    assert!(
        page_xobjects.0.contains_key("Fm1"),
        "页面应登记表单资源名: {page_xobjects:?}"
    );
    assert!(
        !page_xobjects.0.contains_key("Im1"),
        "页面不应获得表单图片: {page_xobjects:?}"
    );
}

#[test]
fn form_resources_share_objects_with_page_resources() {
    let font = Font::helvetica(12.0);
    let image = rgb(1, 1, 200);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 72.0, 700.0, "Page");
    b.image(p, 0.0, 0.0, 10.0, 10.0, &image);
    let form = form_with_resources()
        .with_resources(FormResources::new().with_font(font).with_image(image));
    let handle = b.form_xobject(&form);
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let doc = b.assemble_document().expect("assemble");

    let page = page_resources(&doc);
    let page_font = page
        .get("Font")
        .and_then(Value::as_dict)
        .and_then(|d| d.get("F1"))
        .and_then(Value::as_ref)
        .expect("页面字体引用");
    let page_image = page
        .get("XObject")
        .and_then(Value::as_dict)
        .and_then(|d| d.get("Im1"))
        .and_then(Value::as_ref)
        .expect("页面图片引用");
    let form = form_dict(&doc);
    let form_resources = form
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("form /Resources");
    let form_font = form_resources
        .get("Font")
        .and_then(Value::as_dict)
        .and_then(|d| d.get("F1"))
        .and_then(Value::as_ref)
        .expect("表单字体引用");
    let form_image = form_resources
        .get("XObject")
        .and_then(Value::as_dict)
        .and_then(|d| d.get("Im1"))
        .and_then(Value::as_ref)
        .expect("表单图片引用");

    assert_eq!(form_font, page_font, "相同字体应共享对象号");
    assert_eq!(form_image, page_image, "相同图片应共享对象号");
    let image_objects = (1..=64)
        .filter(|num| {
            doc.get_object(*num)
                .expect("object lookup")
                .is_some_and(|object| {
                    object.value.as_dict().is_some_and(|dict| {
                        dict.get("Subtype").and_then(Value::as_name) == Some("Image")
                    })
                })
        })
        .count();
    assert_eq!(image_objects, 1, "图片流对象应只写一次");
}

#[test]
fn unused_form_resources_are_not_written() {
    let mut b = PdfBuilder::new();
    let _ = b.page(PageSize::A4);
    let _ = b.form_xobject(&form_with_resources().with_resources(resources_of()));
    let s = render(&mut b);

    assert!(!s.contains("/Subtype /Form"), "未绘制的表单不应写出: {s}");
    assert!(
        !s.contains("/BaseFont /Helvetica"),
        "未绘制表单的字体不应写出: {s}"
    );
    assert!(
        !s.contains("/Subtype /Image"),
        "未绘制表单的图片不应写出: {s}"
    );
}

#[test]
fn empty_form_resources_replace_inherited_resources() {
    let mut content = ContentStream::new();
    content.rect(0.0, 0.0, 5.0, 5.0).fill();
    let form =
        FormXObject::new([0.0, 0.0, 10.0, 10.0], content).with_resources(FormResources::new());
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(&form);
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let doc = b.assemble_document().expect("assemble");

    let dict = form_dict(&doc);
    let resources = dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("显式空资源也应写出 /Resources");
    assert!(resources.0.is_empty(), "{resources:?}");
}

#[test]
fn form_optional_content_writes_oc_and_properties_references() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Layer"));
    let form = FormXObject::new([0.0, 0.0, 10.0, 10.0], ContentStream::new())
        .with_optional_content(group)
        .with_resources(
            FormResources::new()
                .with_font(Font::helvetica(10.0))
                .with_optional_content(group),
        );
    let handle = b.form_xobject(&form);
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let doc = b.assemble_document().expect("assemble");

    let dict = form_dict(&doc);
    let oc_ref = dict
        .get("OC")
        .and_then(Value::as_ref)
        .expect("表单应写 /OC 引用");
    let properties = dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|d| d.get("Properties"))
        .and_then(Value::as_dict)
        .expect("/Properties 子字典");
    let property_ref = properties
        .get("OC1")
        .and_then(Value::as_ref)
        .expect("/OC1 应为 OCG 引用");
    assert_eq!(oc_ref, property_ref, "/OC 与 /Properties 应引用同一 OCG");

    let ocg = doc
        .get_object(oc_ref.0)
        .expect("OCG lookup")
        .expect("OCG object");
    assert_eq!(
        ocg.value.as_dict().and_then(|d| d.get("Type")),
        Some(&Value::Name("OCG".into()))
    );

    let catalog = doc
        .get_object(1)
        .expect("catalog lookup")
        .expect("catalog object")
        .value
        .as_dict()
        .expect("catalog")
        .clone();
    let ocgs = catalog
        .get("OCProperties")
        .and_then(Value::as_dict)
        .and_then(|d| d.get("OCGs"))
        .and_then(Value::as_array)
        .expect("/OCProperties /OCGs");
    assert_eq!(
        ocgs.iter().filter_map(Value::as_ref).collect::<Vec<_>>(),
        vec![oc_ref],
        "表单引用的组应写入目录配置"
    );
}

#[test]
fn form_optional_content_requires_pdf_1_5() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Layer"));
    let handle = b.form_xobject(
        &FormXObject::new([0.0, 0.0, 10.0, 10.0], ContentStream::new())
            .with_optional_content(group),
    );
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_build_error(&mut b, "optional content");
}

#[test]
fn unknown_form_optional_content_handles_fail_the_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(
        &FormXObject::new([0.0, 0.0, 10.0, 10.0], ContentStream::new()).with_optional_content(3),
    );
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_build_error(&mut b, "unknown optional content group handle 3");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let handle = b.form_xobject(
        &FormXObject::new([0.0, 0.0, 10.0, 10.0], ContentStream::new())
            .with_resources(FormResources::new().with_optional_content(5)),
    );
    b.draw_form(p, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_build_error(&mut b, "unknown optional content group handle 5");
}
