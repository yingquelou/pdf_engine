//! 命名模板页写出 → 解析 → 回读往返（12.7.6 Named Pages、Table 31
//! `/Templates`）：名称、`/MediaBox`、`/Type /Template` 与页面树计数。

use super::*;

#[test]
fn builder_templates_roundtrip_through_parser() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder};
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_3);
    builder.page(PageSize::Letter);
    builder.add_template("Cover", PageSize::A4);
    builder.add_template("Sticker", PageSize::Custom(144.0, 72.0));

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build templates");

    let doc = parse_buf(&buf);
    assert_eq!(doc.page_count().unwrap(), 1, "模板页不得进入页面树");
    doc.load_all_objects().expect("load all objects");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(
        features.contains(&PdfFeature::PageTemplates),
        "特性画像应登记 PageTemplates"
    );

    // 目录 `/Names /Templates` 是单层叶子名称树。
    let catalog = {
        let (root, _) = doc
            .trailer
            .dict
            .get("Root")
            .and_then(Value::as_ref)
            .expect("/Root");
        doc.get_object(root)
            .expect("catalog lookup")
            .expect("catalog object")
            .value
            .as_dict()
            .expect("catalog dict")
            .clone()
    };
    let tree = catalog
        .get("Names")
        .and_then(Value::as_dict)
        .and_then(|names| names.get("Templates"))
        .and_then(Value::as_dict)
        .expect("/Names /Templates");
    assert!(tree.get("Kids").is_none(), "模板名称树应为叶子节点");

    let templates = doc.page_templates().expect("read templates");
    assert_eq!(templates.len(), 2);
    let keys: Vec<&str> = templates.keys().map(String::as_str).collect();
    assert_eq!(keys, ["Cover", "Sticker"]);

    let cover = &templates["Cover"];
    assert_eq!(
        cover.dict.get("Type").and_then(Value::as_name),
        Some("Template")
    );
    assert_eq!(cover.dict.get("Parent"), None);
    assert_eq!(cover.dict.get("B"), None);
    assert_eq!(
        cover
            .media_box
            .map(|rect| (rect.llx(), rect.lly(), rect.urx(), rect.ury())),
        Some((0.0, 0.0, 595.0, 842.0))
    );

    let sticker = &templates["Sticker"];
    assert_eq!(
        sticker
            .media_box
            .map(|rect| (rect.llx(), rect.lly(), rect.urx(), rect.ury())),
        Some((0.0, 0.0, 144.0, 72.0))
    );
}
