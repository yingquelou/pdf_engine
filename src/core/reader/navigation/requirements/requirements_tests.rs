//! 文档需求读端单元测试（ISO 32000-1 12.10：Table 264/265 与 12.10.1 的
//! `RH` shall）。形状沿用 `navigation/extensions` 切片：目录级字典 +
//! 类型化模型 + 违例诊断；损坏输入不 panic、不整读失败。

use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

use super::{
    DocumentRequirements, HandlerType, NavigationError, RequirementType, RequirementsIssue,
};

const CATALOG: i64 = 1;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn dict(entries: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 目录带 `/Requirements`（值由调用方给定，可为任意类型）的文档。
fn document_with_entry(value: Value) -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        CATALOG,
        dict(vec![("Type", name("Catalog")), ("Requirements", value)]),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

/// 目录 `/Requirements` 直接是给定的数组。
fn document_with(entries: Vec<Value>) -> Document {
    document_with_entry(arr(entries))
}

/// 只有目录、没有 `/Requirements` 的文档。
fn bare_document() -> Document {
    let mut doc = Document::new();
    insert(&doc, CATALOG, dict(vec![("Type", name("Catalog"))]));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

/// 合法的需求字典（Table 264 全条目齐备）。
fn enable_javascripts() -> Value {
    dict(vec![
        ("Type", name("Requirement")),
        ("S", name("EnableJavaScripts")),
    ])
}

fn js_handler(script: &str) -> Value {
    dict(vec![
        ("Type", name("ReqHandler")),
        ("S", name("JS")),
        ("Script", Value::String(script.to_owned())),
    ])
}

/// 带 `EnableJavaScripts` 与给定 `RH` 数组的需求字典。
fn requirement_with_handlers(handlers: Vec<Value>) -> Value {
    dict(vec![
        ("S", name("EnableJavaScripts")),
        ("RH", arr(handlers)),
    ])
}

/// 读取；除目录缺失路径外，任何 `Err` 都视为实现缺陷。
fn read(doc: &Document) -> Option<DocumentRequirements> {
    doc.requirements()
        .unwrap_or_else(|error| panic!("需求读取不应失败: {error}"))
}

/// 12.10.1 的 `RH` shall 违例简写（多个用例都要断言它）。
fn rh_shall(index: usize, handler_count: usize) -> RequirementsIssue {
    RequirementsIssue::RhUsedWithEnableJavaScripts {
        index,
        handler_count,
    }
}

#[test]
fn requirements_absent_returns_none() {
    assert_eq!(read(&bare_document()), None);
    assert_eq!(
        bare_document().requirements_issues().expect("read"),
        Vec::new()
    );
    assert!(!bare_document().requires_javascript().expect("read"));
}

#[test]
fn requirements_read_without_catalog_reports_catalog_missing() {
    let doc = Document::new();
    assert_eq!(
        doc.requirements().expect_err("无目录须报错"),
        NavigationError::CatalogMissing
    );
    assert_eq!(
        doc.requirements_issues().expect_err("无目录须报错"),
        NavigationError::CatalogMissing
    );
    assert_eq!(
        doc.requires_javascript().expect_err("无目录须报错"),
        NavigationError::CatalogMissing
    );
}

#[test]
fn requirements_reads_conforming_entries_and_keeps_array_order() {
    let doc = document_with(vec![
        enable_javascripts(),
        dict(vec![("S", name("EnableJavaScripts"))]),
    ]);
    let model = read(&doc).expect("present");
    assert!(model.is_conforming(), "合规需求不应有违例: {model:?}");
    assert!(model.issues.is_empty());
    assert_eq!(model.requirements.len(), 2, "数组顺序原样保留");
    let entry = &model.requirements[0];
    assert_eq!(entry.type_name.as_deref(), Some("Requirement"));
    assert_eq!(entry.subtype, RequirementType::EnableJavaScripts);
    assert!(entry.handlers.is_empty());
    assert_eq!(entry.dict.get("S"), Some(&name("EnableJavaScripts")));
    assert_eq!(model.requirements[1].type_name, None, "/Type 可选");
    assert!(doc.requires_javascript().expect("read"));
}

#[test]
fn requirements_resolves_indirect_array_and_entries() {
    // 规范要求直接对象；读端宽容解引用（同 extensions.rs 的库策略）。
    let doc = document_with_entry(Value::Ref(7, 0));
    insert(&doc, 7, arr(vec![Value::Ref(8, 0)]));
    insert(&doc, 8, enable_javascripts());
    let model = read(&doc).expect("present");
    assert_eq!(model.requirements.len(), 1);
    assert_eq!(
        model.requirements[0].subtype,
        RequirementType::EnableJavaScripts
    );
    assert!(model.is_conforming());
}

#[test]
fn requirements_reports_non_array_as_degraded_view() {
    // Table 3 / 12.10.1：/Requirements shall 是数组；非数组时降级而非失败。
    let doc = document_with_entry(name("EnableJavaScripts"));
    let model = read(&doc).expect("present");
    assert_eq!(
        model.issues,
        vec![RequirementsIssue::CatalogEntryNotArray { found: "name" }]
    );
    assert_eq!(model.issues[0].requirement_index(), None);
    assert!(model.requirements.is_empty());
    assert!(!model.is_conforming());
    assert!(!doc.requires_javascript().expect("read"));
}

#[test]
fn requirements_reports_non_dictionary_entry_and_keeps_index() {
    let doc = document_with(vec![
        enable_javascripts(),
        Value::Integer(42),
        dict(vec![("S", name("EnableJavaScripts"))]),
    ]);
    let model = read(&doc).expect("present");
    assert_eq!(
        model.issues,
        vec![RequirementsIssue::EntryNotDictionary {
            index: 1,
            found: "integer"
        }]
    );
    assert_eq!(model.requirements.len(), 2, "其余元素照常建模");
    assert_eq!(model.issues[0].requirement_index(), Some(1), "保留数组下标");
}

#[test]
fn requirements_reports_unresolvable_entry_without_failing() {
    let doc = document_with(vec![enable_javascripts(), Value::Ref(99, 0)]);
    let model = read(&doc).expect("present");
    assert_eq!(model.requirements.len(), 1);
    match model.issues[0].clone() {
        RequirementsIssue::EntryUnresolvable { index, message } => {
            assert_eq!(index, 1);
            assert!(message.contains("not found"), "原错误保留: {message}");
        }
        other => panic!("悬空元素应记 EntryUnresolvable: {other:?}"),
    }
}

#[test]
fn requirements_reports_type_entry_violations() {
    // Table 264：/Type 可选，存在时 shall 为 Requirement；违例里保留原值。
    let wrong = read(&document_with(vec![dict(vec![
        ("Type", name("Catalog")),
        ("S", name("EnableJavaScripts")),
    ])]))
    .expect("present");
    assert_eq!(
        wrong.requirements[0].issues,
        vec![RequirementsIssue::RequirementTypeNotRequirement {
            index: 0,
            name: "Catalog".to_owned()
        }]
    );
    assert_eq!(
        wrong.requirements[0].type_name.as_deref(),
        Some("Catalog"),
        "原名字保留"
    );

    let not_name = read(&document_with(vec![dict(vec![
        ("Type", Value::Integer(1)),
        ("S", name("EnableJavaScripts")),
    ])]))
    .expect("present");
    assert_eq!(
        not_name.requirements[0].issues,
        vec![RequirementsIssue::RequirementTypeNotName {
            index: 0,
            found: "integer"
        }]
    );
    assert_eq!(not_name.requirements[0].type_name, None);
}

#[test]
fn requirements_reports_unknown_subtype_without_dropping_entry() {
    // 12.10.1：PDF 1.7 唯一定义 EnableJavaScripts；未知名字保留 + 记违例。
    let doc = document_with(vec![dict(vec![("S", name("EnableRichMedia"))])]);
    let model = read(&doc).expect("present");
    let entry = &model.requirements[0];
    assert_eq!(
        entry.subtype,
        RequirementType::Unknown("EnableRichMedia".into())
    );
    assert_eq!(
        entry.issues,
        vec![RequirementsIssue::SubtypeNotEnableJavaScripts {
            index: 0,
            name: "EnableRichMedia".to_owned()
        }]
    );
    assert!(!doc.requires_javascript().expect("read"));
}

#[test]
fn requirements_reports_missing_or_non_name_subtype() {
    // Table 264：/S 是 Required 条目且 shall 是 name。
    let missing = read(&document_with(vec![dict(vec![])])).expect("present");
    assert_eq!(missing.requirements[0].subtype, RequirementType::Missing);
    assert_eq!(
        missing.requirements[0].issues,
        vec![RequirementsIssue::SubtypeMissing { index: 0 }]
    );
    assert!(!missing.is_conforming());

    let not_name =
        read(&document_with(vec![dict(vec![("S", Value::Integer(7))])])).expect("present");
    assert_eq!(
        not_name.requirements[0].subtype,
        RequirementType::NotName("integer")
    );
    assert_eq!(
        not_name.requirements[0].issues,
        vec![RequirementsIssue::SubtypeNotName {
            index: 0,
            found: "integer"
        }]
    );
}

#[test]
fn requirements_reports_rh_used_with_enable_javascripts() {
    // 12.10.1 L26123–L26125：「The RH entry shall not be used in PDF 1.7」。
    let doc = document_with(vec![requirement_with_handlers(vec![js_handler("Hello")])]);
    let model = read(&doc).expect("present");
    let entry = &model.requirements[0];
    assert_eq!(
        entry.issues,
        vec![rh_shall(0, 1)],
        "RH 与 EnableJavaScripts 同现须类型化报出"
    );
    assert_eq!(entry.handlers.len(), 1, "违例不丢处理器");
    assert!(!entry.is_conforming());
}

#[test]
fn requirements_reads_javascript_handler_script_forms() {
    // Table 265：S = JS 时 Script 合法，值是文档名称字典里的动作名；
    // 字面文本串与十六进制文本串（PDFDocEncoding）都要解码。
    let literal = dict(vec![
        ("Type", name("ReqHandler")),
        ("S", name("JS")),
        ("Script", Value::String("InitDoc".to_owned())),
    ]);
    let hex = dict(vec![
        ("Type", name("ReqHandler")),
        ("S", name("JS")),
        ("Script", Value::HexString("48656C6C6F".to_owned())),
    ]);
    let doc = document_with(vec![requirement_with_handlers(vec![literal, hex])]);
    let model = read(&doc).expect("present");
    let entry = &model.requirements[0];
    assert_eq!(entry.handlers[0].type_name.as_deref(), Some("ReqHandler"));
    assert_eq!(entry.handlers[0].subtype, HandlerType::JavaScript);
    assert_eq!(entry.handlers[0].script.as_deref(), Some("InitDoc"));
    assert_eq!(entry.handlers[1].script.as_deref(), Some("Hello"));
    assert_eq!(
        entry.issues,
        vec![rh_shall(0, 2)],
        "EnableJavaScripts 带 RH 本身即 12.10.1 违例，/Script 自身合规"
    );
}

#[test]
fn requirements_reads_handler_without_rh_shall_when_subtype_is_unknown() {
    // RH 的 shall 只针对 EnableJavaScripts；未知需求类型下不报该违例。
    let handler = dict(vec![
        ("S", name("JS")),
        ("Script", Value::String("InitDoc".to_owned())),
    ]);
    let doc = document_with(vec![dict(vec![
        ("S", name("EnableRichMedia")),
        ("RH", arr(vec![handler])),
    ])]);
    let model = read(&doc).expect("present");
    let entry = &model.requirements[0];
    assert_eq!(
        entry.issues,
        vec![RequirementsIssue::SubtypeNotEnableJavaScripts {
            index: 0,
            name: "EnableRichMedia".to_owned()
        }]
    );
    assert_eq!(entry.handlers[0].subtype, HandlerType::JavaScript);
    assert_eq!(entry.handlers[0].script.as_deref(), Some("InitDoc"));
}

#[test]
fn requirements_reads_noop_handler() {
    // Table 265：NoOp 不给字典添加任何条目，让老阅读器忽略无法识别的需求。
    let handler = dict(vec![("Type", name("ReqHandler")), ("S", name("NoOp"))]);
    let doc = document_with(vec![requirement_with_handlers(vec![handler])]);
    let model = read(&doc).expect("present");
    let parsed = &model.requirements[0].handlers[0];
    assert_eq!(parsed.subtype, HandlerType::NoOp);
    assert_eq!(parsed.script, None);
    assert_eq!(parsed.dict.0.len(), 2, "NoOp 只带 Type/S");
}

#[test]
fn requirements_reports_handler_subtype_violations() {
    let doc = document_with(vec![requirement_with_handlers(vec![
        dict(vec![("S", name("Python"))]),
        dict(vec![("Type", Value::Integer(3)), ("S", name("JS"))]),
        dict(vec![("Type", name("ReqHandler"))]),
        dict(vec![("S", Value::Integer(1))]),
    ])]);
    let model = read(&doc).expect("present");
    let entry = &model.requirements[0];
    assert_eq!(entry.handlers.len(), 4);
    assert_eq!(
        entry.handlers[0].subtype,
        HandlerType::Unknown("Python".into())
    );
    assert_eq!(entry.handlers[2].subtype, HandlerType::Missing);
    assert_eq!(entry.handlers[3].subtype, HandlerType::NotName("integer"));
    assert_eq!(
        entry.issues,
        vec![
            RequirementsIssue::HandlerSubtypeNotPermitted {
                index: 0,
                handler: 0,
                name: "Python".to_owned()
            },
            RequirementsIssue::HandlerTypeNotName {
                index: 0,
                handler: 1,
                found: "integer"
            },
            RequirementsIssue::HandlerSubtypeMissing {
                index: 0,
                handler: 2
            },
            RequirementsIssue::HandlerSubtypeNotName {
                index: 0,
                handler: 3,
                found: "integer"
            },
            // 12.10.1 的 RH shall 判定在解析完数组后追加，故排在最后。
            rh_shall(0, 4),
        ]
    );
}

#[test]
fn requirements_reports_handler_wrong_type_entry() {
    let handler = dict(vec![("Type", name("Requirement")), ("S", name("JS"))]);
    let doc = document_with(vec![requirement_with_handlers(vec![handler])]);
    let model = read(&doc).expect("present");
    assert_eq!(
        model.requirements[0].issues,
        vec![
            RequirementsIssue::HandlerTypeNotReqHandler {
                index: 0,
                handler: 0,
                name: "Requirement".to_owned()
            },
            rh_shall(0, 1),
        ]
    );
    assert_eq!(
        model.requirements[0].handlers[0].type_name.as_deref(),
        Some("Requirement")
    );
}

#[test]
fn requirements_reports_script_violations() {
    // Table 265：/Script「valid only if the S entry has a value of JS」，
    // 且 shall 是 text string。
    let on_noop = dict(vec![
        ("S", name("NoOp")),
        ("Script", Value::String("Hello".to_owned())),
    ]);
    let model = read(&document_with(vec![requirement_with_handlers(vec![
        on_noop,
    ])]))
    .expect("present");
    let entry = &model.requirements[0];
    assert_eq!(
        entry.issues,
        vec![
            RequirementsIssue::ScriptOnlyForJavaScript {
                index: 0,
                handler: 0
            },
            rh_shall(0, 1)
        ]
    );
    assert_eq!(
        entry.handlers[0].script.as_deref(),
        Some("Hello"),
        "违例仍保留原值"
    );

    let not_text = dict(vec![("S", name("JS")), ("Script", name("Hello"))]);
    let model = read(&document_with(vec![requirement_with_handlers(vec![
        not_text,
    ])]))
    .expect("present");
    assert_eq!(
        model.requirements[0].issues,
        vec![
            RequirementsIssue::ScriptNotTextString {
                index: 0,
                handler: 0,
                found: "name"
            },
            rh_shall(0, 1),
        ]
    );
    assert_eq!(model.requirements[0].handlers[0].script, None);
}

#[test]
fn requirements_reports_handler_entry_shape_violations() {
    let doc = document_with(vec![requirement_with_handlers(vec![
        Value::Integer(5),
        Value::Ref(98, 0),
    ])]);
    let model = read(&doc).expect("present");
    let entry = &model.requirements[0];
    assert!(entry.handlers.is_empty(), "非字典元素不建模");
    assert!(
        matches!(
            entry.issues[0],
            RequirementsIssue::HandlerEntryNotDictionary {
                index: 0,
                handler: 0,
                found: "integer"
            }
        ),
        "应为 HandlerEntryNotDictionary: {:?}",
        entry.issues
    );
    assert!(
        matches!(
            entry.issues[1],
            RequirementsIssue::HandlerEntryUnresolvable { handler: 1, .. }
        ),
        "悬空处理器元素须保留: {:?}",
        entry.issues
    );
}

#[test]
fn requirements_reports_rh_shape_violations() {
    // Table 264：/RH shall 是 requirement handler 字典数组。
    let not_array = read(&document_with(vec![dict(vec![
        ("S", name("EnableJavaScripts")),
        ("RH", name("ReqHandler")),
    ])]))
    .expect("present");
    assert_eq!(
        not_array.requirements[0].issues,
        vec![
            RequirementsIssue::HandlersNotArray {
                index: 0,
                found: "name"
            },
            rh_shall(0, 0)
        ]
    );

    let dangling = read(&document_with(vec![dict(vec![
        ("S", name("EnableJavaScripts")),
        ("RH", Value::Ref(97, 0)),
    ])]))
    .expect("present");
    match &dangling.requirements[0].issues[0] {
        RequirementsIssue::HandlersUnresolvable { index, message } => {
            assert_eq!(*index, 0);
            assert!(message.contains("not found"), "原错误保留: {message}");
        }
        other => panic!("悬空 RH 应记 HandlersUnresolvable: {other:?}"),
    }
}

#[test]
fn requirements_issues_view_orders_array_level_first() {
    let doc = document_with(vec![
        Value::Integer(1),
        requirement_with_handlers(vec![js_handler("Hello")]),
    ]);
    let issues = doc.requirements_issues().expect("read");
    assert_eq!(issues.len(), 2, "数组级在前、逐条在后: {issues:?}");
    assert!(matches!(
        issues[0],
        RequirementsIssue::EntryNotDictionary { index: 0, .. }
    ));
    assert!(matches!(
        issues[1],
        RequirementsIssue::RhUsedWithEnableJavaScripts { index: 1, .. }
    ));
    assert!(
        issues[0].to_string().contains("12.10.1"),
        "Display 须带条款依据: {}",
        issues[0]
    );
    assert!(
        issues[1]
            .to_string()
            .contains("the RH entry shall not be used in PDF 1.7"),
        "Display 须带原文: {}",
        issues[1]
    );
}

#[test]
fn requirements_dangling_catalog_entry_fails_read() {
    // `/Requirements` 自身悬空属目录级错误（同 extensions.rs 的宽容边界）。
    let doc = document_with_entry(Value::Ref(96, 0));
    let error = doc.requirements().expect_err("悬空引用须报错");
    assert!(
        matches!(error, NavigationError::Malformed(_)),
        "应为 Malformed: {error:?}"
    );
    assert!(
        error.to_string().contains("96"),
        "错误信息应带对象号: {error}"
    );
}
