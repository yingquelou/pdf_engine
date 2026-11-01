//! 查看器首选项读端测试（ISO 32000-1 12.2，Table 150）。

use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

use super::{Duplex, NonFullScreenPageMode, PageBoundaryName, PrintScaling, ReadingDirection};

const CATALOG: i64 = 1;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 最小目录文档：目录字典为给定条目，`/ViewerPreferences` 由调用方注入。
fn catalog_document(catalog: Dict) -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        CATALOG,
        Value::Dict(catalog.insert("Type", name("Catalog"))),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

fn viewer_dict(entries: Vec<(&str, Value)>) -> Value {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

/// 带 `/ViewerPreferences` 直接字典的文档。
fn document_with(entries: Vec<(&str, Value)>) -> Document {
    catalog_document(Dict::new().insert("ViewerPreferences", viewer_dict(entries)))
}

fn read(doc: &Document) -> Option<super::ViewerPreferences> {
    doc.viewer_preferences()
        .unwrap_or_else(|error| panic!("查看器首选项读取不应 panic: {error}"))
}

#[test]
fn viewer_preferences_missing_returns_none() {
    let doc = catalog_document(Dict::new());
    assert!(read(&doc).is_none(), "无 /ViewerPreferences 应返回 None");
}

#[test]
fn viewer_preferences_absent_entries_materialize_table_defaults() {
    // 空字典：全部条目按 Table 150 缺省物化。
    let doc = document_with(Vec::new());
    let prefs = read(&doc).expect("空字典仍是合法首选项");
    assert!(!prefs.hide_toolbar);
    assert!(!prefs.hide_menubar);
    assert!(!prefs.hide_window_ui);
    assert!(!prefs.fit_window);
    assert!(!prefs.center_window);
    assert!(!prefs.display_doc_title);
    assert_eq!(
        prefs.non_full_screen_page_mode,
        NonFullScreenPageMode::UseNone
    );
    assert_eq!(prefs.direction, ReadingDirection::LeftToRight);
    assert_eq!(prefs.view_area, PageBoundaryName::CropBox);
    assert_eq!(prefs.view_clip, PageBoundaryName::CropBox);
    assert_eq!(prefs.print_area, PageBoundaryName::CropBox);
    assert_eq!(prefs.print_clip, PageBoundaryName::CropBox);
    assert_eq!(prefs.print_scaling, PrintScaling::AppDefault);
    assert_eq!(prefs.duplex, None, "/Duplex 无规范缺省，应保持未声明");
    assert_eq!(prefs.pick_tray_by_pdf_size, None);
    assert!(prefs.print_page_range.is_empty());
    assert_eq!(prefs.num_copies, None);
    assert!(prefs.enforce.is_empty());
}

#[test]
fn viewer_preferences_boolean_flags_read_as_declared() {
    let doc = document_with(vec![
        ("HideToolbar", Value::Bool(true)),
        ("HideMenubar", Value::Bool(true)),
        ("HideWindowUI", Value::Bool(false)),
        ("FitWindow", Value::Bool(true)),
        ("CenterWindow", Value::Bool(false)),
        ("DisplayDocTitle", Value::Bool(true)),
        ("PickTrayByPDFSize", Value::Bool(false)),
    ]);
    let prefs = read(&doc).expect("读取应成功");
    assert!(prefs.hide_toolbar);
    assert!(prefs.hide_menubar);
    assert!(!prefs.hide_window_ui);
    assert!(prefs.fit_window);
    assert!(!prefs.center_window);
    assert!(prefs.display_doc_title);
    // /PickTrayByPDFSize 缺省由阅读器定义：显式 false 必须与未声明区分。
    assert_eq!(prefs.pick_tray_by_pdf_size, Some(false));
}

#[test]
fn viewer_preferences_page_mode_and_direction_read_typed_names() {
    let doc = document_with(vec![
        ("NonFullScreenPageMode", name("UseOC")),
        ("Direction", name("R2L")),
    ]);
    let prefs = read(&doc).expect("读取应成功");
    assert_eq!(
        prefs.non_full_screen_page_mode,
        NonFullScreenPageMode::UseOC
    );
    assert_eq!(prefs.direction, ReadingDirection::RightToLeft);
}

#[test]
fn viewer_preferences_unknown_page_mode_name_returns_typed_error() {
    // 封闭集合未知名字：拒绝是本库策略（规范未定义阅读器行为）。
    let doc = document_with(vec![("NonFullScreenPageMode", name("UseBogus"))]);
    let error = doc
        .viewer_preferences()
        .expect_err("未知页面模式名字应返回类型化错误");
    assert!(error.to_string().contains("NonFullScreenPageMode"));
}

#[test]
fn viewer_preferences_unknown_direction_name_returns_typed_error() {
    let doc = document_with(vec![("Direction", name("T2B"))]);
    assert!(doc.viewer_preferences().is_err(), "L2R/R2L 之外应拒绝");
}

#[test]
fn viewer_preferences_print_scaling_unrecognized_falls_back_to_app_default() {
    // 规范 shall：未识别值按 AppDefault 处理（规范强制的宽松）。
    let doc = document_with(vec![("PrintScaling", name("Whatever"))]);
    let prefs = read(&doc).expect("宽松读取不应报错");
    assert_eq!(prefs.print_scaling, PrintScaling::AppDefault);
    let doc = document_with(vec![("PrintScaling", name("None"))]);
    assert_eq!(read(&doc).expect("读取").print_scaling, PrintScaling::None);
}

#[test]
fn viewer_preferences_boundary_entries_read_names_and_reject_unknown() {
    let doc = document_with(vec![
        ("ViewArea", name("TrimBox")),
        ("ViewClip", name("BleedBox")),
        ("PrintArea", name("ArtBox")),
        ("PrintClip", name("MediaBox")),
    ]);
    let prefs = read(&doc).expect("读取应成功");
    assert_eq!(prefs.view_area, PageBoundaryName::TrimBox);
    assert_eq!(prefs.view_clip, PageBoundaryName::BleedBox);
    assert_eq!(prefs.print_area, PageBoundaryName::ArtBox);
    assert_eq!(prefs.print_clip, PageBoundaryName::MediaBox);
    let doc = document_with(vec![("ViewArea", name("Hbox"))]);
    assert!(doc.viewer_preferences().is_err(), "非法边界名应拒绝");
}

#[test]
fn viewer_preferences_duplex_reads_valid_names_and_rejects_unknown() {
    let doc = document_with(vec![("Duplex", name("DuplexFlipShortEdge"))]);
    assert_eq!(
        read(&doc).expect("读取").duplex,
        Some(Duplex::DuplexFlipShortEdge)
    );
    let doc = document_with(vec![("Duplex", name("DuplexFlipMiddleEdge"))]);
    assert!(
        doc.viewer_preferences().is_err(),
        "Table 150 三值之外应拒绝"
    );
}

#[test]
fn viewer_preferences_print_page_range_pairs_read_and_odd_count_rejected() {
    let doc = document_with(vec![(
        "PrintPageRange",
        arr(vec![
            Value::Integer(1),
            Value::Integer(3),
            Value::Integer(7),
            Value::Integer(9),
        ]),
    )]);
    let prefs = read(&doc).expect("读取应成功");
    assert_eq!(prefs.print_page_range, vec![(1, 3), (7, 9)]);
    let doc = document_with(vec![("PrintPageRange", arr(vec![Value::Integer(1)]))]);
    let error = doc
        .viewer_preferences()
        .expect_err("奇数个页码违反规范 shall 成对");
    assert!(error.to_string().contains("even"));
    let doc = document_with(vec![(
        "PrintPageRange",
        arr(vec![Value::Integer(1), name("3")]),
    )]);
    assert!(doc.viewer_preferences().is_err(), "非整数页码应拒绝");
}

#[test]
fn viewer_preferences_print_page_range_rejects_below_one_and_keeps_reversed_pairs() {
    // 页码 <1：首页 denoted by 1，读取端拒绝。
    let doc = document_with(vec![(
        "PrintPageRange",
        arr(vec![Value::Integer(0), Value::Integer(2)]),
    )]);
    assert!(doc.viewer_preferences().is_err(), "页码必须从 1 起");
    // 倒序子区间：规范未禁止，读取端保留原值（写出端拒绝才是库策略）。
    let doc = document_with(vec![(
        "PrintPageRange",
        arr(vec![Value::Integer(9), Value::Integer(2)]),
    )]);
    let prefs = read(&doc).expect("倒序子区间应宽松保留");
    assert_eq!(prefs.print_page_range, vec![(9, 2)]);
}

#[test]
fn viewer_preferences_num_copies_keeps_any_integer_value() {
    // /NumCopies 的范围表述留给阅读器；读取端保留任意整数（写出 ≥1 是库策略）。
    let doc = document_with(vec![("NumCopies", Value::Integer(0))]);
    assert_eq!(read(&doc).expect("读取").num_copies, Some(0));
    let doc = document_with(vec![("NumCopies", Value::Float(2.0))]);
    assert!(
        doc.viewer_preferences().is_err(),
        "非整数 /NumCopies 应返回类型化错误"
    );
}

#[test]
fn viewer_preferences_enforce_keeps_name_array_and_rejects_non_name_items() {
    let doc = document_with(vec![(
        "Enforce",
        arr(vec![name("SomethingNew"), name("CropBox")]),
    )]);
    let prefs = read(&doc).expect("PDF 2.0 开放名集合应原文保留");
    assert_eq!(prefs.enforce, vec!["SomethingNew", "CropBox"]);
    let doc = document_with(vec![("Enforce", arr(vec![Value::Integer(1)]))]);
    assert!(doc.viewer_preferences().is_err(), "非名元素应拒绝");
    let doc = document_with(vec![("Enforce", name("CropBox"))]);
    assert!(doc.viewer_preferences().is_err(), "/Enforce 必须是数组");
}

#[test]
fn viewer_preferences_type_mismatch_returns_typed_error_not_panic() {
    let doc = document_with(vec![("HideToolbar", Value::Integer(1))]);
    let error = doc
        .viewer_preferences()
        .expect_err("布尔条目写成整数应返回类型化错误");
    assert!(error.to_string().contains("HideToolbar"));
}

#[test]
fn viewer_preferences_unknown_keys_are_ignored() {
    // 通用字典透传语义：未定义键不报错、不进模型。
    let doc = document_with(vec![("VendorExtension", Value::Bool(true))]);
    assert!(read(&doc).is_some(), "未知键应被忽略");
}

#[test]
fn viewer_preferences_indirect_dictionary_is_resolved() {
    let doc = catalog_document(Dict::new().insert("ViewerPreferences", Value::Ref(9, 0)));
    insert(&doc, 9, viewer_dict(vec![("FitWindow", Value::Bool(true))]));
    let prefs = read(&doc).expect("间接引用字典应解一层后读取");
    assert!(prefs.fit_window);
}

#[test]
fn viewer_preferences_non_dictionary_entry_returns_malformed() {
    let doc = catalog_document(Dict::new().insert("ViewerPreferences", name("nope")));
    let error = doc
        .viewer_preferences()
        .expect_err("非字典值应返回类型化错误");
    assert!(matches!(error, super::NavigationError::Malformed(_)));
}

#[test]
fn viewer_preferences_catalog_missing_returns_typed_error() {
    let doc = Document::new();
    let error = doc
        .viewer_preferences()
        .expect_err("无目录应返回类型化错误");
    assert!(matches!(error, super::NavigationError::CatalogMissing));
}
