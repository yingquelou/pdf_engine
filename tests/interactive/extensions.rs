//! 扩展字典写出 → 解析 → 回读往返（ISO 32000-1 7.12、Table 50）：
//! 目录 `/Extensions` 直接对象、前缀字典的 `/BaseVersion` 与
//! `/ExtensionLevel`、特性画像登记。

use super::*;

#[test]
fn builder_extensions_roundtrip_through_parser() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder};
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::Letter);
    builder.add_extension("ADBE", PdfVersion::V1_7, 3);
    builder.add_extension("GLGR", PdfVersion::V1_7, 1002);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build extensions");

    let doc = parse_buf(&buf);
    doc.load_all_objects().expect("load all objects");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(
        features.contains(&PdfFeature::ExtensionsDictionary),
        "特性画像应登记 ExtensionsDictionary"
    );

    // 目录 `/Extensions` 是直接嵌套字典（7.12.1），前缀值为开发者
    // 扩展字典（Table 50）。
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
    let extensions = catalog
        .get("Extensions")
        .and_then(Value::as_dict)
        .expect("catalog /Extensions must be a direct dictionary");
    assert_eq!(extensions.0.len(), 2);

    let read = doc.extensions().expect("read extensions").expect("present");
    let keys: Vec<&str> = read.prefixes.keys().map(String::as_str).collect();
    assert_eq!(keys, ["ADBE", "GLGR"]);
    assert_eq!(read.prefixes["ADBE"].base_version, "1.7");
    assert_eq!(
        read.prefixes["ADBE"].base_version_parsed(),
        Some(PdfVersion::V1_7)
    );
    assert_eq!(read.prefixes["ADBE"].extension_level, 3);
    assert_eq!(
        read.prefixes["ADBE"].type_name.as_deref(),
        Some("DeveloperExtensions")
    );
    assert_eq!(read.prefixes["GLGR"].extension_level, 1002);
}
