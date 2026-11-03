//! FDF `/Pages`（12.7.7.3.3：Table 248 页 → Table 249 模板 → Table 250
//! 命名页引用）与 `/Annots`（12.7.7.3.4：Table 251）类型化读端单元测试。
//!
//! 断言依据（`pdftotext -raw docs/PDF32000_2008.pdf`，行号为 raw 抽取行）：
//! - L25055–L25057「The optional Pages field in an FDF dictionary (see
//!   Table 243) shall contain an array of FDF page dictionaries (PDF 1.3)
//!   describing new pages that shall be added to the target document」；
//! - L24874–L24877 `/Pages`（Optional; PDF 1.3）与「The Fields and Status
//!   entries shall not be present together with this entry」；
//! - L24885–L24889 `/Annots`（Optional; PDF 1.3）与「except Link, Movie,
//!   Widget, PrinterMark, Screen, and TrapNet」；
//! - Table 248（L25085–L25089）`Templates` **Required** array、`Info`
//!   Optional dictionary；
//! - Table 249（L25113–L25123）`TRef` **Required** dictionary、`Fields`
//!   Optional array of references、`Rename` Optional boolean
//!   「Default value: true」；
//! - Table 250（L25124–L25128）`Name` **Required** string、`F` Optional
//!   file specification「If this entry is absent, it shall be assumed that
//!   the page resides in the associated PDF file」；
//! - Table 251（L25129–L25132）`Page` integer「**Required for annotations
//!   in FDF files** … where page 0 is the first page」。
//!
//! 覆盖：四层结构逐层类型化与声明序、可选条目缺省不物化、原值永不丢弃、
//! 四条 Required 条目（Table 248/249/250/251）的 shall 违例降级为
//! [`FdfPagesIssue`] 类型化诊断表（含 `Display` 里的条款行号）、PDF 1.3
//! 版本语义只报告不拒绝、Table 251 `/Page` 序数与 Table 243 注解类型
//! 黑名单。Required 违例与版本语义测试见子模块 `diagnostics`，`/Annots`
//! 逐项测试见子模块 `annots`，上限与不可信输入防护见子模块 `limits`，
//! Annex J 导入合并见子模块 `merge`。

use super::*;
use crate::core::reader::fdf::{FdfAnnots, FdfPages, FdfPagesIssue};

/// 单数组条目上限、全局条目预算与引用环/深度防护的测试。
mod limits;
/// Annex J（informative）口径的导入合并报告测试。
mod merge;

/// Table 251 `/Page` 序数与 Table 243 注解类型黑名单的测试。
mod annots;
/// `/Pages`/`/Annots` 四张表的 Required 条目违例、诊断 `Display` 引文、
/// Table 243 `/Pages` 互斥 shall 与 PDF 1.3 版本语义报告的测试。
mod diagnostics;

/// 「目录 → FDF 字典」两段装配，FDF 字典条目与附加间接对象由调用方给出。
fn doc_with_fdf_entries(entries: Dict, extra: Vec<Object>) -> Document {
    let mut objects = vec![catalog_obj(Value::Ref(2, 0)), fdf_dict_obj(entries, 2)];
    objects.extend(extra);
    doc_from_objects(objects)
}

/// 只带 `/Pages` 的文件。
fn pages_view_of(pages: Value, extra: Vec<Object>) -> FdfPages {
    doc_with_fdf_entries(Dict::new().insert("Pages", pages), extra)
        .fdf_pages()
        .expect("读端宽容，不应报错")
        .expect("/Pages 存在时总有视图")
}

/// 只带 `/Annots` 的文件。
fn annots_view_of(annots: Value, extra: Vec<Object>) -> FdfAnnots {
    doc_with_fdf_entries(Dict::new().insert("Annots", annots), extra)
        .fdf_annots()
        .expect("读端宽容，不应报错")
        .expect("/Annots 存在时总有视图")
}

fn array(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn text(value: &str) -> Value {
    Value::String(value.to_string())
}

fn integer(value: i64) -> Value {
    Value::Integer(value)
}

/// 按键序装配一个字典值。
fn dict_value(pairs: &[(&str, Value)]) -> Value {
    let mut dict = Dict::new();
    for (key, value) in pairs {
        dict = dict.insert(*key, value.clone());
    }
    Value::Dict(dict)
}

/// Table 250 的命名页引用字典（只带 Required 的 `/Name`）。
fn named_page(name: &str) -> Value {
    dict_value(&[("Name", text(name))])
}

/// Table 248 的一个页字典。
fn page(pairs: &[(&str, Value)]) -> Value {
    dict_value(pairs)
}

/// Table 249 的一个模板字典。
fn template(pairs: &[(&str, Value)]) -> Value {
    dict_value(pairs)
}

/// Table 251 的一个注解字典（`/Type /Annot` + 调用方条目）。
fn annotation(pairs: &[(&str, Value)]) -> Value {
    let mut entries: Vec<(&str, Value)> = vec![
        ("Type", name_entry("Annot")),
        ("Subtype", name_entry("Text")),
    ];
    for (key, value) in pairs {
        entries.retain(|(known, _)| *known != *key);
        entries.push((key, value.clone()));
    }
    dict_value(&entries)
}

/// 汇总一个 `/Pages` 视图的全部诊断（数组级 + 逐页 + 逐模板）。
fn pages_issues(view: &FdfPages) -> Vec<FdfPagesIssue> {
    let mut issues = view.issues.clone();
    for entry in &view.entries {
        issues.extend(entry.issues.iter().cloned());
        for item in &entry.templates {
            issues.extend(item.issues.iter().cloned());
        }
    }
    issues
}

/// 汇总一个 `/Annots` 视图的全部诊断。
fn annots_issues(view: &FdfAnnots) -> Vec<FdfPagesIssue> {
    let mut issues = view.issues.clone();
    for entry in &view.entries {
        issues.extend(entry.issues.iter().cloned());
    }
    issues
}

#[test]
fn fdf_pages_and_annots_absent_give_no_view() {
    // Optional 条目缺席 ⇒ None（与「空数组」可区分，同 `/Fields` 口径）。
    let document = doc_with_fdf_entries(
        Dict::new().insert("Fields", array(vec![field("a")])),
        Vec::new(),
    );
    assert!(document.fdf_pages().expect("ok").is_none());
    assert!(document.fdf_annots().expect("ok").is_none());
    // 两者都缺席 ⇒ 导入合并报告也不存在。
    assert!(document.fdf_import_merge().expect("ok").is_none());
}

#[test]
fn plain_pdf_has_no_fdf_pages_view() {
    let document = doc_from_objects(vec![Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", name_entry("Catalog"))),
    )]);
    assert!(document.fdf_pages().expect("ok").is_none());
    assert!(document.fdf_annots().expect("ok").is_none());
    assert!(document.fdf_import_merge().expect("ok").is_none());
}

#[test]
fn fdf_pages_types_page_template_and_named_page_layers() {
    // Table 248 → 249 → 250 三层一次装配；未建模条目从 entry() 取回。
    let view = pages_view_of(
        array(vec![page(&[
            (
                "Templates",
                array(vec![template(&[
                    ("TRef", named_page("Template.A")),
                    ("Rename", Value::Bool(false)),
                ])]),
            ),
            ("Custom", text("raw")),
        ])]),
        Vec::new(),
    );
    assert_eq!(view.len(), 1);
    assert!(
        view.issues.is_empty(),
        "合法结构不应有诊断：{:?}",
        view.issues
    );
    let entry = view.page(0).expect("页 0");
    assert_eq!(entry.entry("Custom"), Some(&text("raw")));
    assert_eq!(entry.templates.len(), 1);
    let item = &entry.templates[0];
    assert_eq!(item.name(), Some("Template.A"));
    assert_eq!(item.rename, Some(false));
    assert!(!item.rename_effective());
    let reference = item.named_page.as_ref().expect("/TRef 已类型化");
    assert_eq!(reference.name.as_deref(), Some("Template.A"));
    // `/Pages` 原值同时保留在 raw（双通道，同 `/Differences` 口径）。
    assert!(matches!(view.raw, Value::Array(_)));
}

#[test]
fn fdf_template_fields_reuse_the_table_246_field_tree() {
    // Table 249 `/Fields`（raw L25117–L25119）：元素是「指向 Table 246
    // 字段字典的引用」，展开后全限定名基线为空父名。
    let root = Dict::new()
        .insert("T", text("person"))
        .insert("Kids", array(vec![field("name")]));
    let item_fields = array(vec![Value::Ref(3, 0), field("extra"), integer(4)]);
    let templates = array(vec![template(&[
        ("TRef", named_page("T")),
        ("Fields", item_fields.clone()),
    ])]);
    let view = pages_view_of(
        array(vec![page(&[("Templates", templates)])]),
        vec![Object::new(3, 0, Value::Dict(root))],
    );
    let item = &view.page(0).expect("页").templates[0];
    let names: Vec<&str> = item
        .fields
        .iter()
        .map(|field| field.fully_qualified_name.as_str())
        .collect();
    assert_eq!(names, vec!["person", "extra"]);
    assert_eq!(item.fields[0].kids[0].fully_qualified_name, "person.name");
    assert!(item.has_declared_fields());
    // 非字段的数组项（整数）降级为诊断，原数组整体不丢。
    assert!(
        pages_issues(&view).contains(&FdfPagesIssue::TemplateFieldSkipped {
            page: 0,
            template: 0,
            field: 2,
            message: "FDF field is not a dictionary".to_string(),
        }),
        "畸形字段项应有类型化诊断：{:?}",
        pages_issues(&view)
    );
    assert_eq!(item.entry("Fields"), Some(&item_fields));
    // `FdfPages::template_fields` 跨页/跨模板汇总（声明序）。
    assert_eq!(view.template_fields().len(), 2);
}

#[test]
fn fdf_pages_optional_entries_are_not_materialized() {
    // Table 249 `/Rename`「Default value: true」与 Table 250 `/F` 的规范
    // 推定都不物化成显式条目：缺席就是 None。
    let view = pages_view_of(
        array(vec![page(&[
            (
                "Templates",
                array(vec![
                    template(&[("TRef", named_page("A"))]),
                    template(&[
                        (
                            "TRef",
                            dict_value(&[("Name", text("B")), ("F", text("tpl.pdf"))]),
                        ),
                        ("Rename", Value::Bool(false)),
                    ]),
                ]),
            ),
            ("CustomPageKey", integer(1)),
        ])]),
        Vec::new(),
    );
    let entry = view.page(0).expect("页");
    assert!(!entry.is_missing_templates());
    assert_eq!(entry.info, None);
    assert_eq!(entry.entry("CustomPageKey"), Some(&integer(1)));
    let first = &entry.templates[0];
    assert_eq!(first.rename, None, "缺省 true 不应物化");
    assert!(first.rename_effective());
    assert!(!first.has_declared_fields());
    let reference = first.named_page.as_ref().expect("/TRef");
    assert_eq!(reference.file, None);
    assert!(
        reference.resides_in_associated_pdf(),
        "raw L25127–L25128：/F 缺席即假定页在关联 PDF 文件里"
    );
    let second = &entry.templates[1];
    assert_eq!(second.rename, Some(false));
    assert!(!second.rename_effective());
    let other = second.named_page.as_ref().expect("/TRef");
    assert_eq!(other.file, Some(text("tpl.pdf")));
    assert!(!other.resides_in_associated_pdf());
}

#[test]
fn fdf_page_info_is_kept_as_untyped_raw_value() {
    // Table 248 `/Info`（raw L25088–L25089）：「An FDF page information
    // dictionary」在 32000-1 全文只出现这一次、没有条目表 ⇒ 不发明键，
    // 整体按原值保留。
    let info = dict_value(&[("Author", text("Adobe")), ("UnknownThing", integer(3))]);
    let view = pages_view_of(
        array(vec![page(&[
            ("Templates", array(vec![])),
            ("Info", info.clone()),
        ])]),
        Vec::new(),
    );
    let entry = view.page(0).expect("页");
    assert_eq!(entry.info.as_ref(), Some(&info));
    assert_eq!(entry.entry("Info"), Some(&info));
    assert!(entry.templates.is_empty());
    assert!(
        !entry.is_missing_templates(),
        "/Templates 是空数组而非缺失，不算 Required 违例"
    );
    // `/Info` 缺席时绝不伪造空字典。
    let bare = pages_view_of(
        array(vec![page(&[("Templates", array(vec![]))])]),
        Vec::new(),
    );
    assert_eq!(bare.page(0).expect("页").info, None);
}

#[test]
fn fdf_pages_reference_form_keeps_raw_and_reference() {
    // `/Pages` 写成间接引用（FDF 交叉引用表可选，12.7.7.1 raw
    // L24763–L24764）：可解引用则照常展开。
    let view = pages_view_of(
        Value::Ref(3, 0),
        vec![Object::new(
            3,
            0,
            array(vec![page(&[("Templates", array(vec![]))])]),
        )],
    );
    assert_eq!(view.reference, Some((3, 0)));
    assert_eq!(view.raw, Value::Ref(3, 0));
    assert_eq!(view.len(), 1);
    // 悬空引用：整体降级，原值仍在 raw，绝不报错。
    let dangling = pages_view_of(Value::Ref(42, 0), Vec::new());
    assert!(dangling.is_empty());
    assert_eq!(dangling.reference, Some((42, 0)));
    assert!(
        dangling
            .issues
            .iter()
            .any(|issue| matches!(issue, FdfPagesIssue::EntryUnresolvable { key: "Pages", .. }))
    );
    // 非数组形态：EntryNotArray + 原值保留。
    let not_array = pages_view_of(name_entry("placeholder"), Vec::new());
    assert!(matches!(
        not_array.issues.first(),
        Some(FdfPagesIssue::EntryNotArray {
            key: "Pages",
            found: "name"
        })
    ));
    assert_eq!(not_array.raw, name_entry("placeholder"));
    assert_eq!(not_array.len(), 0);
}
