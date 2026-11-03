//! FDF 读端单元测试（12.7.7.3, Table 241/242/243/246）。
//!
//! 全部通过写入端形态的 `Document`（对象预置缓存、无源文件）驱动
//! `Document::fdf()` 路径，逐条覆盖：检测宽容策略、目录/FDF 字典条目
//! 类型化（`/Target` 串形态；`/JavaScript` 见子模块 javascript.rs）、
//! 字段树全限定名连接、直接/间接 Kids、未建模条目原样保留、
//! 整型标志校验、环与深度上限拒绝、便捷枚举接口，以及 `/Pages` 四层
//! 结构与 `/Annots` 的 Table 251 `/Page`（见子模块 pages.rs）。

use crate::core::types::{Array, Dict, Document, Object, StartXref, Trailer, Value, Xref};

use super::{FdfDocument, FdfError, MAX_FDF_FIELD_DEPTH};

/// `/Differences`（stream）与 `/EmbeddedFDFs`（file specification 数组）
/// 的类型化读端测试（Table 243 续表 + Table 244）。
mod embedded;
/// `/JavaScript`（Table 243 续表 + Table 245）的类型化解码测试。
mod javascript;
/// `/Pages`（Table 248/249/250）与 `/Annots`（Table 251）的类型化读端测试，
/// 含上限防护（`pages::limits`）与 Annex J 导入合并（`pages::merge`）。
mod pages;

/// 组装 trailer `/Root → 1 0 R` 的文件级 Document（对象全部预置缓存）。
fn doc_from_objects(objects: Vec<Object>) -> Document {
    let trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    Document::from_objects(objects, Xref::new(), trailer, StartXref(0))
}

/// FDF 目录字典（Table 242）：`/FDF` 指向 FDF 字典。
fn catalog_obj(fdf_target: Value) -> Object {
    Object::new(1, 0, Value::Dict(Dict::new().insert("FDF", fdf_target)))
}

fn fdf_dict_obj(entries: Dict, num: i64) -> Object {
    Object::new(num, 0, Value::Dict(entries))
}

fn name_entry(value: &str) -> Value {
    Value::Name(value.to_string())
}

fn field(name: &str) -> Value {
    Value::Dict(Dict::new().insert("T", Value::String(name.to_string())))
}

fn field_array(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

/// 用「目录 → FDF 字典 → /Fields 数组」三段装配一个最小 FDF 文件。
fn fdf_doc_with_fields(fields: Vec<Value>) -> (Document, FdfDocument) {
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(Dict::new().insert("Fields", field_array(fields)), 2),
    ]);
    let fdf = document
        .fdf()
        .expect("FDF 读端应成功")
        .expect("该文件是 FDF");
    (document, fdf)
}

// ── 检测与宽容策略 ──────────────────────────────────────────────────

#[test]
fn fdf_root_detected_via_catalog_fdf_entry() {
    let (document, _fdf) = fdf_doc_with_fields(vec![]);
    let detected = document.fdf().expect("ok").expect("detected");
    assert!(detected.fdf.fields.expect("fields").is_empty());
}

#[test]
fn trailer_without_root_is_not_an_error() {
    let document = Document::from_objects(
        vec![catalog_obj(Value::Ref(2, 0))],
        Xref::new(),
        Trailer::new(),
        StartXref(0),
    );
    assert!(document.fdf().expect("ok").is_none());
}

#[test]
fn root_without_fdf_entry_is_treated_as_plain_pdf() {
    let document = doc_from_objects(vec![Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", name_entry("Catalog"))),
    )]);
    assert!(document.fdf().expect("ok").is_none());
}

#[test]
fn dangling_root_reference_is_fine() {
    // trailer `/Root` 指向不存在的对象：探测阶段宽容，返回 None 不报错
    let document = doc_from_objects(vec![]);
    assert!(document.fdf().expect("ok").is_none());
}

#[test]
fn fdf_marker_must_reference_dictionary() {
    let document = doc_from_objects(vec![catalog_obj(name_entry("NotADict"))]);
    let err = document.fdf().expect_err("FDF 标志必须指向字典");
    assert!(matches!(err, FdfError::Malformed(_)), "got {err:?}");
    assert!(err.to_string().contains("/FDF"));
}

// ── 目录/FDF 字典条目类型化 ────────────────────────────────────────

#[test]
fn catalog_version_exposed_and_raw_catalog_kept() {
    let catalog = Dict::new()
        .insert("FDF", Value::Ref(2, 0))
        .insert("Version", name_entry("1.4"));
    let document = doc_from_objects(vec![
        Object::new(1, 0, Value::Dict(catalog)),
        fdf_dict_obj(Dict::new(), 2),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(fdf.catalog_version.as_deref(), Some("1.4"));
    // 原始目录字典保留，供未建模条目取回
    assert_eq!(fdf.catalog.get("FDF"), Some(&Value::Ref(2, 0)));
}

#[test]
fn fdf_dictionary_entries_typed_where_thin() {
    let fdf_entries = Dict::new()
        .insert("F", Value::String("tax-form.pdf".to_string()))
        .insert(
            "ID",
            field_array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ]),
        )
        .insert("Encoding", name_entry("utf_16"))
        .insert("Status", Value::String("Saved".to_string()))
        .insert("Target", Value::String("_top".to_string()))
        .insert("JavaScript", Value::Dict(Dict::new()));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(fdf_entries, 2),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(
        fdf.fdf.source_file,
        Some(Value::String("tax-form.pdf".to_string()))
    );
    assert_eq!(fdf.fdf.encoding.as_deref(), Some("utf_16"));
    assert_eq!(fdf.fdf.status.as_deref(), Some("Saved"));
    assert_eq!(fdf.fdf.target, Some(Value::String("_top".to_string())));
    // Table 243 续表 — `/Target`（raw L24927）本轮类型化：串形态解码。
    assert_eq!(fdf.fdf.target_frame.as_deref(), Some("_top"));
    // /ID 以原值保留（本切片不展开建模）
    assert!(fdf.fdf.identifier.is_some());
    // Table 243 续表（raw L24933–L24934）—— `/JavaScript` 本轮类型化：
    // 字典形态给出类型化视图（空字典 = Table 245 四条目皆 None），
    // 原值同时保留（全形态覆盖见 tests/javascript.rs）。
    assert!(fdf.fdf.javascript.is_some());
    let scripts = fdf.fdf.javascript_scripts.expect("字典形态应被类型化");
    assert_eq!(scripts.before, None);
    assert_eq!(scripts.after, None);
    assert_eq!(scripts.after_perms_ready, None);
    assert!(scripts.doc_scripts.is_empty());
    // 全部条目同时可以从原始字典取回
    assert!(fdf.fdf.dict.get("JavaScript").is_some());
    assert!(matches!(
        fdf.fdf.dict.get("JavaScript"),
        Some(Value::Dict(_))
    ));
}

#[test]
fn target_hex_text_string_decodes_to_typed_frame() {
    // `/Target` 十六进制串形态（UTF-16BE 带 BOM 的 "_top"）：与写端
    // `text_string_value` 的十六进制文本串策略配对解码，原值保留。
    let fdf_entries = Dict::new().insert(
        "Target",
        Value::HexString("FEFF005F0074006F0070".to_string()),
    );
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(fdf_entries, 2),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(fdf.fdf.target_frame.as_deref(), Some("_top"));
    assert_eq!(
        fdf.fdf.target,
        Some(Value::HexString("FEFF005F0074006F0070".to_string()))
    );
}

#[test]
fn target_non_string_form_is_tolerated_and_kept_raw() {
    // 字典式 `/Target`（Adobe FDF 2.0 形态，非 ISO 32000-1 基线，待核）：
    // 读端宽容——不报错、类型化字段为 None、原值不丢弃。
    let dict_target = Dict::new().insert("F", Value::String("form.pdf".to_string()));
    let fdf_entries = Dict::new().insert("Target", Value::Dict(dict_target.clone()));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(fdf_entries, 2),
    ]);
    let fdf = document.fdf().expect("非串 /Target 不应报错").expect("FDF");
    assert_eq!(fdf.fdf.target_frame, None);
    assert_eq!(fdf.fdf.target, Some(Value::Dict(dict_target)));
}

#[test]
fn target_absent_leaves_typed_and_raw_none() {
    // 缺省条目不物化：`/Target` 缺失时类型化与原值都是 None。
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(Dict::new(), 2),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert_eq!(fdf.fdf.target_frame, None);
    assert_eq!(fdf.fdf.target, None);
}

#[test]
fn fields_and_pages_conflict_is_diagnosed_not_enforced() {
    // Table 243：/Fields 与 /Pages shall not both be present——读端
    // 宽容保留两侧，仅通过 fields_pages_conflict 暴露冲突
    let fdf_entries = Dict::new()
        .insert("Fields", field_array(vec![field("a")]))
        .insert("Pages", name_entry("placeholder"));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(fdf_entries, 2),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert!(fdf.fdf.fields_pages_conflict());
    assert!(fdf.fdf.pages.is_some());
    assert_eq!(fdf.fdf.fields.expect("both kept").len(), 1);
}

#[test]
fn fields_alone_does_not_flag_conflict() {
    let fdf_entries = Dict::new()
        .insert("Fields", field_array(vec![]))
        .insert("Status", Value::String("Saved".to_string()));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(fdf_entries, 2),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    assert!(!fdf.fdf.fields_pages_conflict());
    assert_eq!(fdf.fdf.status.as_deref(), Some("Saved"));
}

// ── 字段树（Table 246）──────────────────────────────────────────────

#[test]
fn root_fields_expose_value_and_flags_with_raw_extras() {
    let kid = Dict::new()
        .insert("T", Value::String("tax-form".to_string()))
        .insert("V", Value::String("2024".to_string()))
        .insert("Ff", Value::Integer(4))
        .insert("SetFf", Value::Integer(1))
        .insert("ClrFf", Value::Integer(0))
        .insert("F", Value::Integer(2))
        .insert("Custom", Value::String("raw".to_string()));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(
            Dict::new().insert("Fields", field_array(vec![Value::Dict(kid)])),
            2,
        ),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    let field = &fdf.fdf.fields.expect("fields")[0];
    assert_eq!(field.partial_name, "tax-form");
    assert_eq!(field.fully_qualified_name, "tax-form");
    assert_eq!(field.value, Some(Value::String("2024".to_string())));
    assert_eq!(field.field_flags, Some(4));
    assert_eq!(field.set_field_flags, Some(1));
    assert_eq!(field.clear_field_flags, Some(0));
    assert_eq!(field.widget_flags, Some(2));
    // 未建模的 Custom 条目从原始字典取回
    assert_eq!(field.dict.get("Custom"), Some(&Value::String("raw".into())));
}

#[test]
fn qualified_names_join_parent_and_child() {
    // 与 reader/form（AcroForm 字段树，12.7.3.2）同一连接规则：父名 + '.' + 子名
    let root = Dict::new()
        .insert("T", Value::String("person".to_string()))
        .insert("Kids", field_array(vec![field("name")]));
    let (_, fdf) = fdf_doc_with_fields(vec![Value::Dict(root)]);
    let names: Vec<&str> = fdf
        .all_fields()
        .iter()
        .map(|f| f.fully_qualified_name.as_str())
        .collect();
    assert_eq!(names, vec!["person", "person.name"]);
}

#[test]
fn empty_partial_name_does_not_extend_qualified_name() {
    let root = Dict::new()
        .insert("T", Value::String("group".to_string()))
        .insert(
            "Kids",
            field_array(vec![Value::Dict(
                Dict::new()
                    .insert("T", Value::String(String::new()))
                    .insert("V", Value::String("kept".to_string())),
            )]),
        );
    let (_, fdf) = fdf_doc_with_fields(vec![Value::Dict(root)]);
    assert_eq!(fdf.all_fields()[1].fully_qualified_name, "group");
    assert_eq!(
        fdf.all_fields()[1].value,
        Some(Value::String("kept".to_string()))
    );
}

#[test]
fn nested_kids_resolve_full_paths_recursively() {
    let leaf = Dict::new()
        .insert("T", Value::String("b".to_string()))
        .insert("V", Value::Integer(1));
    let mid = Dict::new()
        .insert("T", Value::String("a".to_string()))
        .insert("Kids", field_array(vec![Value::Dict(leaf)]));
    let root = Dict::new()
        .insert("T", Value::String("q".to_string()))
        .insert("Kids", field_array(vec![Value::Dict(mid)]));
    let (_, fdf) = fdf_doc_with_fields(vec![Value::Dict(root)]);
    let names: Vec<&str> = fdf
        .all_fields()
        .iter()
        .map(|f| f.fully_qualified_name.as_str())
        .collect();
    assert_eq!(names, vec!["q", "q.a", "q.a.b"]);
    assert_eq!(
        fdf.field("q.a.b").expect("lookup").value,
        Some(Value::Integer(1))
    );
}

#[test]
fn direct_child_field_objects_are_supported() {
    // Table 246 /Kids：FDF 允许直接对象与间接引用混排
    let root = Dict::new()
        .insert("T", Value::String("top".to_string()))
        .insert("Kids", field_array(vec![field("direct"), Value::Ref(3, 0)]));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(
            Dict::new().insert("Fields", field_array(vec![Value::Dict(root)])),
            2,
        ),
        Object::new(3, 0, field("indirect")),
    ]);
    let fdf = document.fdf().expect("ok").expect("FDF");
    let kids = &fdf.fdf.fields.expect("fields")[0].kids;
    assert_eq!(kids[0].partial_name, "direct");
    assert_eq!(kids[1].partial_name, "indirect");
}

#[test]
fn integer_flag_entries_must_be_integers() {
    let bad = Dict::new()
        .insert("T", Value::String("x".to_string()))
        .insert("Ff", name_entry("not-int"));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(
            Dict::new().insert("Fields", field_array(vec![Value::Dict(bad)])),
            2,
        ),
    ]);
    let err = document.fdf().expect_err("Ff 必须是整数");
    assert!(matches!(err, FdfError::Malformed(_)));
    assert!(err.to_string().contains("/Ff"));
}

#[test]
fn field_missing_required_partial_name_is_malformed() {
    let bad = Dict::new().insert("V", Value::String("orphan".to_string()));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(
            Dict::new().insert("Fields", field_array(vec![Value::Dict(bad)])),
            2,
        ),
    ]);
    let err = document.fdf().expect_err("/T 必填");
    assert!(err.to_string().contains("missing required /T"));
}

#[test]
fn reference_cycle_is_rejected() {
    // 字段 2 的 /Kids 再次引用自身 → 环必须被拒绝
    let looping = Dict::new()
        .insert("T", Value::String("loop".to_string()))
        .insert("Kids", field_array(vec![Value::Ref(2, 0)]));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(3, 0)),
        Object::new(
            3,
            0,
            Value::Dict(Dict::new().insert("Fields", field_array(vec![Value::Ref(2, 0)]))),
        ),
        Object::new(2, 0, Value::Dict(looping)),
    ]);
    let err = document.fdf().expect_err("环必须被拒绝");
    assert!(matches!(err, FdfError::Malformed(_)), "got {err:?}");
    assert!(err.to_string().contains("cycle"));
}

#[test]
fn overly_deep_field_nesting_hits_limit() {
    // 构造 MAX_FDF_FIELD_DEPTH + 2 层嵌套，断言深度上限生效
    let depth = MAX_FDF_FIELD_DEPTH + 2;
    let mut current = field("leaf");
    for _ in 0..depth {
        current = Value::Dict(
            Dict::new()
                .insert("T", Value::String("n".to_string()))
                .insert("Kids", field_array(vec![current])),
        );
    }
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(Dict::new().insert("Fields", field_array(vec![current])), 2),
    ]);
    let err = document.fdf().expect_err("深度上限必须生效");
    assert!(matches!(err, FdfError::LimitExceeded(_)), "got {err:?}");
}

// ── 便捷枚举接口 ────────────────────────────────────────────────────

#[test]
fn fdf_field_values_lists_names_and_values() {
    let a = Dict::new()
        .insert("T", Value::String("a".to_string()))
        .insert("V", Value::String("1".to_string()));
    let b = Dict::new()
        .insert("T", Value::String("b".to_string()))
        .insert("Kids", field_array(vec![field("c")]));
    let document = doc_from_objects(vec![
        catalog_obj(Value::Ref(2, 0)),
        fdf_dict_obj(
            Dict::new().insert("Fields", field_array(vec![Value::Dict(a), Value::Dict(b)])),
            2,
        ),
    ]);
    let listing = document.fdf_field_values().expect("ok");
    assert_eq!(
        listing,
        vec![
            ("a".to_string(), Some(Value::String("1".into()))),
            ("b".to_string(), None),
            ("b.c".to_string(), None),
        ]
    );
}

#[test]
fn plain_pdf_gets_empty_field_listing() {
    let document = doc_from_objects(vec![Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", name_entry("Catalog"))),
    )]);
    assert_eq!(document.fdf_field_values().expect("ok"), Vec::new());
}
