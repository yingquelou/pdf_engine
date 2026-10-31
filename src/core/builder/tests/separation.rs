//! 分色字典写端测试（ISO 32000-1 14.11.4 Table 364）：同组共享
//! `/Pages`、色料名与颜色空间写出、PDF 1.3 门禁与零字节校验契约。

use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::builder::{DeviceColorant, SeparationInfo};
use crate::core::types::{PdfFeature, PdfVersion};
use crate::core::writer::WriteError;

fn separation(colorant: &str) -> ColorSpace {
    ColorSpace::Separation(Separation::new(
        colorant,
        ColorSpace::DeviceGray,
        Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
    ))
}

fn page_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("page object")
        .expect("page exists")
        .value
        .as_dict()
        .expect("page dict")
        .clone()
}

fn separation_dict(page: &Dict) -> Dict {
    page.get("SeparationInfo")
        .expect("/SeparationInfo")
        .as_dict()
        .expect("separation dictionary")
        .clone()
}

/// `/Pages` 数组的对象号序列。
fn page_numbers(info: &Dict) -> Vec<i64> {
    info.get("Pages")
        .expect("/Pages")
        .as_array()
        .expect("/Pages array")
        .iter()
        .map(|value| value.as_ref().expect("page reference").0)
        .collect()
}

fn build_error(configure: impl FnOnce(&mut PdfBuilder)) -> WriteError {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    configure(&mut b);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("invalid separation group must be rejected");
    assert!(out.is_empty(), "校验失败必须零字节输出");
    error
}

#[test]
fn separation_group_writes_shared_pages_and_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.add_separation_group(
        &[0, 1],
        SeparationInfo::new(DeviceColorant::name("Cyan")).with_color_space(separation("Cyan")),
    );
    b.add_separation_group(
        &[2],
        SeparationInfo::new(DeviceColorant::text("PANTONE 35 CV")),
    );
    let doc = b.assemble_document().expect("assemble");

    let first = separation_dict(&page_dict(&doc, 3));
    let second = separation_dict(&page_dict(&doc, 5));
    assert_eq!(page_numbers(&first), vec![3, 5]);
    assert_eq!(page_numbers(&second), vec![3, 5]);
    assert_eq!(
        first.get("DeviceColorant"),
        Some(&Value::Name("Cyan".into()))
    );
    let Some(Value::Array(space)) = first.get("ColorSpace") else {
        panic!("/ColorSpace must be an array")
    };
    assert_eq!(space.0.first(), Some(&Value::Name("Separation".into())));
    assert_eq!(first.get("ColorSpace"), second.get("ColorSpace"));

    // 文本串色料按十六进制文本串写出。
    let third = separation_dict(&page_dict(&doc, 7));
    assert_eq!(page_numbers(&third), vec![7]);
    assert_eq!(
        third.get("DeviceColorant"),
        Some(&text_string_value("PANTONE 35 CV"))
    );
    assert_eq!(third.get("ColorSpace"), None);

    assert!(
        doc.features.borrow().contains(PdfFeature::SeparationInfo),
        "特性画像应含 separation dictionaries"
    );
}

#[test]
fn separation_group_requires_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    b.page(PageSize::A4);
    b.add_separation_group(&[0], SeparationInfo::new(DeviceColorant::name("Cyan")));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.2 must reject separation dictionaries");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(
        error.to_string().contains("separation dictionaries"),
        "{error}"
    );
    assert!(error.to_string().contains("1.3"), "{error}");
    assert!(out.is_empty(), "版本门禁失败必须零字节");
}

#[test]
fn invalid_separation_groups_are_rejected_without_output() {
    // 组为空。
    let error = build_error(|b| {
        b.add_separation_group(&[], SeparationInfo::new(DeviceColorant::name("Cyan")));
    });
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(error.to_string().contains("at least one page"), "{error}");

    // 页号越界。
    let error = build_error(|b| {
        b.add_separation_group(&[2], SeparationInfo::new(DeviceColorant::name("Cyan")));
    });
    assert!(error.to_string().contains("out of range"), "{error}");

    // 组内重复。
    let error = build_error(|b| {
        b.add_separation_group(&[0, 0], SeparationInfo::new(DeviceColorant::name("Cyan")));
    });
    assert!(error.to_string().contains("more than once"), "{error}");

    // 组间重叠。
    let error = build_error(|b| {
        b.add_separation_group(&[0], SeparationInfo::new(DeviceColorant::name("Cyan")));
        b.add_separation_group(
            &[0, 1],
            SeparationInfo::new(DeviceColorant::name("Magenta")),
        );
    });
    assert!(
        error.to_string().contains("already in separation group 0"),
        "{error}"
    );

    // 名字型色料含空格；空文本串色料。
    let error = build_error(|b| {
        b.add_separation_group(&[0], SeparationInfo::new(DeviceColorant::name("Bad Name")));
    });
    assert!(matches!(&error, WriteError::InvalidObject(_)));
    assert!(error.to_string().contains("/DeviceColorant"), "{error}");
    let error = build_error(|b| {
        b.add_separation_group(&[0], SeparationInfo::new(DeviceColorant::text("")));
    });
    assert!(error.to_string().contains("must not be empty"), "{error}");

    // /ColorSpace 为设备空间。
    let error = build_error(|b| {
        b.add_separation_group(
            &[0],
            SeparationInfo::new(DeviceColorant::name("Cyan"))
                .with_color_space(ColorSpace::DeviceRGB),
        );
    });
    assert!(
        error.to_string().contains("Separation or DeviceN"),
        "{error}"
    );

    // /ColorSpace 声明为 Separation 但取值非法（空色料名）。
    let error = build_error(|b| {
        b.add_separation_group(
            &[0],
            SeparationInfo::new(DeviceColorant::name("Cyan")).with_color_space(separation("")),
        );
    });
    assert!(
        error.to_string().contains("colorant must not be empty"),
        "{error}"
    );
}
