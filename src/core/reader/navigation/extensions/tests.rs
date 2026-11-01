//! 扩展字典读端测试（7.12、Table 50）：缺省 `None`、前缀字典解析、
//! `/Type` 校验、必填条目缺失与未建模条目保留。

use crate::core::types::{Dict, Document, Object, Trailer, Value};

use super::*;

const PAGES_OBJECT: i64 = 2;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 单页文档；`catalog` 条目并入目录。
fn document_with_catalog(catalog: Dict) -> Document {
    let mut doc = Document::new();
    let catalog = catalog
        .insert("Type", name("Catalog"))
        .insert("Pages", Value::Ref(PAGES_OBJECT, 0));
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Count", Value::Integer(1)),
        ),
    );
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn adbe_extension() -> Value {
    Value::Dict(
        Dict::new()
            .insert("BaseVersion", name("1.7"))
            .insert("ExtensionLevel", Value::Integer(3)),
    )
}

#[test]
fn extensions_absent_returns_none() {
    let doc = document_with_catalog(Dict::new());
    assert_eq!(doc.extensions().expect("read"), None);
}

#[test]
fn extensions_reads_prefix_with_required_entries() {
    let doc = document_with_catalog(Dict::new().insert(
        "Extensions",
        Value::Dict(Dict::new().insert("ADBE", adbe_extension())),
    ));
    let extensions = doc.extensions().expect("read").expect("present");
    assert_eq!(extensions.type_name, None);
    assert_eq!(extensions.prefixes.len(), 1);
    let adbe = &extensions.prefixes["ADBE"];
    assert_eq!(adbe.base_version, "1.7");
    assert_eq!(adbe.base_version_parsed(), Some(PdfVersion::V1_7));
    assert_eq!(adbe.extension_level, 3);
    assert_eq!(adbe.type_name, None);
}

#[test]
fn extensions_accepts_optional_type_entries() {
    let doc = document_with_catalog(
        Dict::new().insert(
            "Extensions",
            Value::Dict(
                Dict::new().insert("Type", name("Extensions")).insert(
                    "GLGR",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", name("DeveloperExtensions"))
                            .insert("BaseVersion", name("1.7"))
                            .insert("ExtensionLevel", Value::Integer(1002)),
                    ),
                ),
            ),
        ),
    );
    let extensions = doc.extensions().expect("read").expect("present");
    assert_eq!(extensions.type_name.as_deref(), Some("Extensions"));
    let glgr = &extensions.prefixes["GLGR"];
    assert_eq!(glgr.type_name.as_deref(), Some("DeveloperExtensions"));
    assert_eq!(glgr.extension_level, 1002);
}

#[test]
fn extensions_keeps_multiple_prefixes_sorted() {
    let doc = document_with_catalog(
        Dict::new().insert(
            "Extensions",
            Value::Dict(
                Dict::new()
                    .insert("GLGR", adbe_extension())
                    .insert("ADBE", adbe_extension()),
            ),
        ),
    );
    let extensions = doc.extensions().expect("read").expect("present");
    let keys: Vec<&str> = extensions.prefixes.keys().map(String::as_str).collect();
    assert_eq!(keys, ["ADBE", "GLGR"]);
}

#[test]
fn extensions_preserves_unmodelled_entries_raw() {
    let doc = document_with_catalog(
        Dict::new().insert(
            "Extensions",
            Value::Dict(
                Dict::new().insert(
                    "ADBE",
                    Value::Dict(
                        Dict::new()
                            .insert("BaseVersion", name("1.7"))
                            .insert("ExtensionLevel", Value::Integer(3))
                            .insert("Level", Value::Integer(5))
                            .insert("Prefix", name("ADBE")),
                    ),
                ),
            ),
        ),
    );
    let extensions = doc.extensions().expect("read").expect("present");
    let adbe = &extensions.prefixes["ADBE"];
    assert_eq!(adbe.dict.get("Level"), Some(&Value::Integer(5)));
    assert_eq!(adbe.dict.get("Prefix"), Some(&name("ADBE")));
}

#[test]
fn extensions_resolves_indirect_prefix_dictionary() {
    // 规范要求直接对象；读端对间接引用宽容解引用。
    let doc = document_with_catalog(Dict::new().insert("Extensions", Value::Ref(7, 0)));
    insert(
        &doc,
        7,
        Value::Dict(Dict::new().insert("extensions-outer-key", Value::Ref(8, 0))),
    );
    // 前缀键任意非 Type 名即可；外层值再指到开发者字典。
    doc.set_object(Object::new(8, 0, adbe_extension()));
    let extensions = doc.extensions().expect("read").expect("present");
    let entry = &extensions.prefixes["extensions-outer-key"];
    assert_eq!(entry.base_version, "1.7");
}

#[test]
fn extensions_rejects_non_dictionary() {
    let doc = document_with_catalog(Dict::new().insert("Extensions", name("Nope")));
    let error = doc.extensions().expect_err("must reject");
    assert!(matches!(error, NavigationError::Malformed(_)));
}

#[test]
fn extensions_rejects_wrong_outer_type() {
    let doc = document_with_catalog(Dict::new().insert(
        "Extensions",
        Value::Dict(Dict::new().insert("Type", name("Bogus"))),
    ));
    let error = doc.extensions().expect_err("must reject");
    assert!(matches!(error, NavigationError::Malformed(_)));
}

#[test]
fn extensions_rejects_missing_base_version() {
    let doc = document_with_catalog(Dict::new().insert(
        "Extensions",
        Value::Dict(Dict::new().insert(
            "ADBE",
            Value::Dict(Dict::new().insert("ExtensionLevel", Value::Integer(3))),
        )),
    ));
    let error = doc.extensions().expect_err("must reject");
    assert!(matches!(error, NavigationError::Malformed(_)));
}

#[test]
fn extensions_rejects_missing_or_non_integer_extension_level() {
    let missing = document_with_catalog(Dict::new().insert(
        "Extensions",
        Value::Dict(Dict::new().insert(
            "ADBE",
            Value::Dict(Dict::new().insert("BaseVersion", name("1.7"))),
        )),
    ));
    assert!(matches!(
        missing.extensions().expect_err("must reject"),
        NavigationError::Malformed(_)
    ));
    let float = document_with_catalog(
        Dict::new().insert(
            "Extensions",
            Value::Dict(
                Dict::new().insert(
                    "ADBE",
                    Value::Dict(
                        Dict::new()
                            .insert("BaseVersion", name("1.7"))
                            .insert("ExtensionLevel", Value::Float(3.0)),
                    ),
                ),
            ),
        ),
    );
    assert!(matches!(
        float.extensions().expect_err("must reject"),
        NavigationError::Malformed(_)
    ));
}

#[test]
fn extensions_rejects_base_version_not_a_name() {
    let doc = document_with_catalog(
        Dict::new().insert(
            "Extensions",
            Value::Dict(
                Dict::new().insert(
                    "ADBE",
                    Value::Dict(
                        Dict::new()
                            .insert("BaseVersion", Value::Float(1.7))
                            .insert("ExtensionLevel", Value::Integer(3)),
                    ),
                ),
            ),
        ),
    );
    assert!(matches!(
        doc.extensions().expect_err("must reject"),
        NavigationError::Malformed(_)
    ));
}

#[test]
fn extensions_rejects_wrong_prefix_type() {
    let doc = document_with_catalog(
        Dict::new().insert(
            "Extensions",
            Value::Dict(
                Dict::new().insert(
                    "ADBE",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", name("Extensions"))
                            .insert("BaseVersion", name("1.7"))
                            .insert("ExtensionLevel", Value::Integer(3)),
                    ),
                ),
            ),
        ),
    );
    assert!(matches!(
        doc.extensions().expect_err("must reject"),
        NavigationError::Malformed(_)
    ));
}

#[test]
fn extensions_rejects_empty_extensions_dictionary() {
    // 7.12.1：扩展字典如存在，shall 含一个或多个开发者条目。
    let bare = document_with_catalog(Dict::new().insert("Extensions", Value::Dict(Dict::new())));
    assert!(matches!(
        bare.extensions().expect_err("must reject"),
        NavigationError::Malformed(_)
    ));
    let typed_only = document_with_catalog(Dict::new().insert(
        "Extensions",
        Value::Dict(Dict::new().insert("Type", name("Extensions"))),
    ));
    assert!(matches!(
        typed_only.extensions().expect_err("must reject"),
        NavigationError::Malformed(_)
    ));
}

#[test]
fn extensions_requires_catalog() {
    let doc = Document::new();
    assert!(matches!(
        doc.extensions().expect_err("must reject"),
        NavigationError::CatalogMissing
    ));
}
