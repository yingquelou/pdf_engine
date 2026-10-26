//! 可选内容组（`/OCG`）与目录 `/OCProperties` 写端测试。

use super::*;
use crate::core::types::{PdfTextString, PdfVersion};

fn page_dict(doc: &Document, index: i64) -> Dict {
    let page_num = 3 + index * 2;
    let object = doc
        .get_object(page_num)
        .expect("page lookup")
        .expect("page object");
    object
        .value
        .as_dict()
        .expect("page object should be a dictionary")
        .clone()
}

fn content_stream(doc: &Document, index: i64) -> String {
    let content_num = 4 + index * 2;
    let object = doc
        .get_object(content_num)
        .expect("content lookup")
        .expect("content object");
    let bytes = doc.stream_bytes(&object.value).expect("content bytes");
    String::from_utf8_lossy(&bytes).into_owned()
}

fn catalog(doc: &Document) -> Dict {
    doc.get_object(1)
        .expect("catalog lookup")
        .expect("catalog object")
        .value
        .as_dict()
        .expect("catalog should be a dictionary")
        .clone()
}

fn ocg_objects(doc: &Document) -> Vec<(i64, Dict)> {
    let mut objects = Vec::new();
    for num in 1..=64 {
        if let Some(object) = doc.get_object(num).expect("object lookup")
            && let Value::Dict(dict) = &object.value
            && dict.get("Type").and_then(Value::as_name) == Some("OCG")
        {
            objects.push((num, dict.clone()));
        }
    }
    objects
}

fn hex_of(text: &str) -> String {
    let bytes = PdfTextString::from(text).encode();
    let mut hex = String::new();
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

fn refs(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| {
            item.as_ref()
                .unwrap_or_else(|| panic!("expected an indirect reference, got {item:?}"))
                .0
        })
        .collect()
}

#[test]
fn optional_content_group_writes_ocg_and_catalog_properties() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let visible = b.optional_content_group(&OptionalContentGroup::new("Visible Layer"));
    let hidden = b.optional_content_group(
        &OptionalContentGroup::new("Hidden Layer")
            .with_visibility(false)
            .with_intents(&["View", "Print"]),
    );
    b.begin_optional_content(p, visible);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Shown");
    b.end_optional_content(p);
    b.begin_optional_content(p, hidden);
    b.text(p, &Font::helvetica(12.0), 72.0, 680.0, "Hidden");
    b.end_optional_content(p);
    let doc = b.assemble_document().expect("assemble");

    // 目录 /OCProperties：两组都登记，默认配置区分可见性。
    let catalog = catalog(&doc);
    let properties = catalog
        .get("OCProperties")
        .and_then(Value::as_dict)
        .expect("/OCProperties");
    let group_refs = refs(properties.get("OCGs").expect("/OCGs"));
    assert_eq!(group_refs.len(), 2, "{properties:?}");
    let configuration = properties
        .get("D")
        .and_then(Value::as_dict)
        .expect("default configuration");
    assert_eq!(
        configuration.get("BaseState").and_then(Value::as_name),
        Some("ON")
    );
    assert_eq!(refs(configuration.get("ON").expect("/ON")), [group_refs[0]]);
    assert_eq!(
        refs(configuration.get("OFF").expect("/OFF")),
        [group_refs[1]]
    );
    assert_eq!(
        refs(configuration.get("Order").expect("/Order")),
        group_refs
    );

    // OCG 字典：文本串名称 + 可选 /Intent（多值写成数组）。
    let ocgs = ocg_objects(&doc);
    assert_eq!(ocgs.len(), 2, "{ocgs:?}");
    assert_eq!(ocgs[0].0, group_refs[0]);
    assert_eq!(
        ocgs[0].1.get("Name"),
        Some(&Value::HexString(hex_of("Visible Layer")))
    );
    assert!(!ocgs[0].1.0.contains_key("Intent"));
    assert_eq!(ocgs[1].0, group_refs[1]);
    assert_eq!(
        ocgs[1].1.get("Name"),
        Some(&Value::HexString(hex_of("Hidden Layer")))
    );
    let intent = ocgs[1]
        .1
        .get("Intent")
        .and_then(Value::as_array)
        .expect("/Intent");
    assert_eq!(
        intent.iter().filter_map(Value::as_name).collect::<Vec<_>>(),
        ["View", "Print"]
    );

    // 页资源 /Properties 与内容流 BDC 标记。
    let resources = page_dict(&doc, 0)
        .get("Resources")
        .and_then(Value::as_dict)
        .cloned()
        .expect("/Resources");
    let page_properties = resources
        .get("Properties")
        .and_then(Value::as_dict)
        .expect("page /Properties");
    assert_eq!(
        page_properties.get("OC1").and_then(Value::as_ref),
        Some((group_refs[0], 0))
    );
    assert_eq!(
        page_properties.get("OC2").and_then(Value::as_ref),
        Some((group_refs[1], 0))
    );
    let content = content_stream(&doc, 0);
    assert_eq!(content.matches("/OC /OC1 BDC\n").count(), 1, "{content}");
    assert_eq!(content.matches("/OC /OC2 BDC\n").count(), 1, "{content}");
    assert_eq!(content.matches("EMC\n").count(), 2, "{content}");
}

#[test]
fn group_used_on_two_pages_shares_object_with_page_local_names() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let p1 = b.page(PageSize::A4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Shared"));
    b.begin_optional_content(p0, group);
    b.text(p0, &Font::helvetica(12.0), 72.0, 700.0, "One");
    b.end_optional_content(p0);
    b.begin_optional_content(p1, group);
    b.text(p1, &Font::helvetica(12.0), 72.0, 700.0, "Two");
    b.end_optional_content(p1);
    let doc = b.assemble_document().expect("assemble");

    let group_refs = refs(
        catalog(&doc)
            .get("OCProperties")
            .and_then(Value::as_dict)
            .and_then(|properties| properties.get("OCGs"))
            .expect("/OCGs"),
    );
    assert_eq!(group_refs.len(), 1, "跨页应复用同一 OCG 对象");
    assert_eq!(ocg_objects(&doc).len(), 1);

    for page in [0, 1] {
        let resources = page_dict(&doc, page)
            .get("Resources")
            .and_then(Value::as_dict)
            .cloned()
            .expect("/Resources");
        let properties = resources
            .get("Properties")
            .and_then(Value::as_dict)
            .expect("page /Properties");
        assert_eq!(
            properties.get("OC1").and_then(Value::as_ref),
            Some((group_refs[0], 0)),
            "page {page}"
        );
        assert!(
            content_stream(&doc, page).contains("/OC /OC1 BDC\n"),
            "page {page}"
        );
    }
}

#[test]
fn unused_and_unknown_groups_are_not_written() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.optional_content_group(&OptionalContentGroup::new("Unused"));
    b.begin_optional_content(p, 42);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Plain");
    let doc = b.assemble_document().expect("assemble");

    assert!(
        catalog(&doc).get("OCProperties").is_none(),
        "未使用的组不应写出 /OCProperties"
    );
    assert!(ocg_objects(&doc).is_empty(), "未使用的组不应写出 OCG 对象");
    let resources = page_dict(&doc, 0)
        .get("Resources")
        .and_then(Value::as_dict)
        .cloned()
        .expect("/Resources");
    assert!(resources.get("Properties").is_none());
}

#[test]
fn invalid_optional_content_group_fails_the_build() {
    let assert_invalid = |group: OptionalContentGroup, expected: &str| {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let handle = b.optional_content_group(&group);
        b.begin_optional_content(p, handle);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法可选内容组应被拒绝");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    };

    assert_invalid(
        OptionalContentGroup::new(""),
        "group name must not be empty",
    );
    assert_invalid(
        OptionalContentGroup::new("Layer").with_intent("Not A Name"),
        "must be a valid PDF name",
    );
}

#[test]
fn optional_content_requires_pdf_1_5() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Layer"));
    b.begin_optional_content(p, group);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "OC");
    b.end_optional_content(p);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.4 must reject optional content");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("optional content"),
        "错误信息应说明可选内容特性: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p = b.page(PageSize::A4);
    let group = b.optional_content_group(&OptionalContentGroup::new("Layer"));
    b.begin_optional_content(p, group);
    b.end_optional_content(p);
    let mut out = Vec::new();
    b.build(&mut out)
        .expect("PDF 1.5 supports optional content");
    assert!(String::from_utf8_lossy(&out).contains("/OCProperties"));
}
