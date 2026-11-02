//! `/SMask` 软蒙版字典：`S`/`BC`/`G`/`TR`/`SMaskInData`/`Matte` 条目写出、
//! 透明组 Form XObject 引用与不登记页面资源、Alpha 缺省省略背景色、非法
//! 句柄/非透明组表单/非法 `BC`/`SMaskInData`/`Matte` 的构建期拒绝，以及
//! PDF 1.4/1.6 版本门禁。

use super::super::*;
use super::{assert_build_error, content_stream, ext_g_state, one_output_function, page_dict};
use crate::core::content::ContentStream;
use crate::core::types::PdfVersion;

/// 供 `/SMask` 引用的透明组表单；不绘制到页面，只由图形状态引用。
fn mask_form() -> FormXObject {
    let mut content = ContentStream::new();
    content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 100.0, 100.0)
        .fill();
    FormXObject::new([0.0, 0.0, 100.0, 100.0], content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray))
}

#[test]
fn soft_mask_writes_dictionary_and_form_reference() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new()
            .with_fill_alpha(0.5)
            .with_soft_mask(SoftMask::luminosity(mask).with_backdrop([0.0, 0.5, 1.0])),
    );
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Hi");
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let mask_dict = gs
        .get("SMask")
        .expect("/SMask")
        .as_dict()
        .expect("/SMask dictionary");
    assert_eq!(
        mask_dict.get("S").and_then(Value::as_name),
        Some("Luminosity")
    );
    assert_eq!(
        numbers(mask_dict.get("BC").expect("/BC")),
        vec![0.0, 0.5, 1.0]
    );
    let (group_num, _) = mask_dict
        .get("G")
        .expect("/G")
        .as_ref()
        .expect("/G should be an indirect reference");

    let group = doc
        .get_object(group_num)
        .expect("group lookup")
        .expect("group object");
    let Value::Stream(group_dict, _) = &group.value else {
        panic!("/G must reference a Form XObject stream");
    };
    assert_eq!(
        group_dict.get("Subtype").and_then(Value::as_name),
        Some("Form")
    );
    assert!(
        group_dict.get("Group").and_then(Value::as_dict).is_some(),
        "/SMask 表单应为透明组"
    );

    // 软蒙版表单只为 /G 写出，不登记为页面 XObject 资源。
    let resources = page_dict(&doc, 0)
        .get("Resources")
        .and_then(Value::as_dict)
        .cloned()
        .expect("resources");
    assert!(
        resources.get("XObject").is_none(),
        "软蒙版表单不应登记为页面 XObject: {resources:?}"
    );
    assert!(content_stream(&doc, 0).contains("/GS1 gs\n"));
}

#[test]
fn alpha_soft_mask_without_backdrop_omits_bc() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask)),
    );
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let mask_dict = gs
        .get("SMask")
        .expect("/SMask")
        .as_dict()
        .expect("/SMask dictionary");
    assert_eq!(mask_dict.get("S").and_then(Value::as_name), Some("Alpha"));
    assert!(!mask_dict.0.contains_key("BC"), "未设置背景色时应省略 /BC");
}

#[test]
fn invalid_soft_mask_parameters_fail_the_build() {
    let assert_invalid = |mut b: PdfBuilder, expected: &str| {
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法软蒙版应被拒绝");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    };

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.set_graphics_state(p, &GraphicsState::new().with_soft_mask(SoftMask::alpha(42)));
    assert_invalid(b, "unknown form handle 42");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mut content = ContentStream::new();
    content.rect(0.0, 0.0, 10.0, 10.0).fill();
    let handle = b.form_xobject(&FormXObject::new([0.0, 0.0, 10.0, 10.0], content));
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(handle)),
    );
    assert_invalid(b, "transparency group Form XObject");

    for backdrop in [Vec::new(), vec![1.5], vec![f64::NAN]] {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let mask = b.form_xobject(&mask_form());
        b.set_graphics_state(
            p,
            &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask).with_backdrop(backdrop)),
        );
        assert_invalid(b, "/SMask /BC");
    }
}

#[test]
fn soft_mask_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask)),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject /SMask");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("transparency"),
        "错误信息应说明透明度特性: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask)),
    );
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports /SMask");
    assert!(String::from_utf8_lossy(&out).contains("/SMask <<"));
}

#[test]
fn soft_mask_writes_transfer_function_smask_in_data_and_matte() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    let transfer = b.function(&one_output_function());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(
            SoftMask::luminosity(mask)
                .with_transfer_function(transfer)
                .with_smask_in_data(1)
                .with_matte([0.2, 0.4, 0.6]),
        ),
    );
    let doc = b.assemble_document().expect("assemble");

    let (_, gs) = ext_g_state(&doc, 0, "GS1");
    let mask_dict = gs
        .get("SMask")
        .expect("/SMask")
        .as_dict()
        .expect("/SMask dictionary");
    let transfer_dict = mask_dict
        .get("TR")
        .expect("/TR")
        .as_dict()
        .expect("inline transfer function dictionary");
    assert_eq!(
        transfer_dict
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        mask_dict.get("SMaskInData").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        numbers(mask_dict.get("Matte").expect("/Matte")),
        vec![0.2, 0.4, 0.6]
    );
}

#[test]
fn soft_mask_extension_parameters_are_validated() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask).with_smask_in_data(3)),
    );
    assert_build_error(&mut b, "/SMask /SMaskInData must be 0, 1 or 2");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask).with_matte(Vec::new())),
    );
    assert_build_error(&mut b, "/SMask /Matte must not be empty");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    let matte = {
        let mut components = vec![0.5];
        components.push(f64::NAN);
        components
    };
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask).with_matte(matte)),
    );
    assert_build_error(&mut b, "/SMask /Matte components must be finite");
}

#[test]
fn matte_requires_pdf_1_6() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask).with_matte([0.5])),
    );
    assert_build_error(&mut b, "soft mask matte color");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p = b.page(PageSize::A4);
    let mask = b.form_xobject(&mask_form());
    b.set_graphics_state(
        p,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask).with_matte([0.5])),
    );
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.6 supports /Matte");
    assert!(String::from_utf8_lossy(&out).contains("/Matte"));
}
