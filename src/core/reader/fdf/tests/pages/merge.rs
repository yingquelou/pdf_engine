//! [`Document::fdf_import_merge`] 的测试：Annex J（**informative**）口径的
//! 导入合并报告——Table 249 `/Fields` 全限定名的跨模板冲突、`/Rename`
//! 生效值与 J.2 形态的前缀**建议**、同一命名页被重复导入（12.7.7.3.3 的
//! NOTE，raw L25093–L25095），以及 Table 251 `/Page` ↔ Table 248 `/Pages`
//! 的**提示性**交叉核对（两者地址的页集合不同，故非 shall 违例）。

use super::*;
use crate::core::reader::fdf::{FdfFieldNameConflict, FdfImportMerge};

/// 按「页下标 → 模板」装配一份导入合并报告。
fn merge_of(pages: Vec<Value>, annots: Vec<Value>) -> FdfImportMerge {
    merge_of_with(pages, annots, Vec::new())
}

/// 同上，并可附加间接对象（模板 `/Fields` 引用树需要真实对象才能解引用）。
fn merge_of_with(pages: Vec<Value>, annots: Vec<Value>, extra: Vec<Object>) -> FdfImportMerge {
    let mut fdf = Dict::new();
    if !pages.is_empty() {
        fdf = fdf.insert("Pages", array(pages));
    }
    if !annots.is_empty() {
        fdf = fdf.insert("Annots", array(annots));
    }
    let document = doc_with_fdf_entries(fdf, extra);
    document
        .fdf_import_merge()
        .expect("读端宽容，不应报错")
        .expect("两个数组至少有一个存在时总有报告")
}

/// 一个带模板的页字典（模板名与字段名列表）。
fn page_with(name: &str, fields: &[&str]) -> Value {
    let items: Vec<Value> = fields.iter().map(|partial| field(partial)).collect();
    page(&[(
        "Templates",
        array(vec![template(&[
            ("TRef", named_page(name)),
            ("Fields", array(items)),
        ])]),
    )])
}

#[test]
fn fdf_import_merge_lists_conflicting_names_and_rename_prefixes() {
    // 两个模板导入同名根字段（NOTE L25095 的第二种形态）。
    let merge = merge_of(
        vec![
            page_with("Template.A", &["tax", "city"]),
            page_with("Template.B", &["tax"]),
        ],
        Vec::new(),
    );
    assert_eq!(merge.templates.len(), 2);
    assert_eq!(merge.page(0).len(), 1);
    assert_eq!(merge.page(1).len(), 1);
    assert!(merge.imports("tax"));
    assert!(merge.imports("city"));
    assert!(!merge.imports("absent"));
    assert_eq!(
        merge.conflicts,
        vec![FdfFieldNameConflict {
            name: "tax".to_string(),
            sources: vec![(0, 0), (1, 0)],
            all_rename: true,
        }],
        "{:?}",
        merge.conflicts
    );
    // Annex J J.2 形态的前缀建议：`P<页下标>.<模板名>_<页内模板序>.`。
    // J.2 的示例把「fifth page」写成 `P5`（L37511–L37516），而 `/Pages`
    // 描述的是**加入目标文档的新页**（L25056）、最终序数不由 FDF 决定，
    // 故这里按声明序下标给出、属建议值（待核，见 merge.rs 模块文档）。
    assert_eq!(
        merge.templates[0].rename_prefix.as_deref(),
        Some("P0.Template.A_0.")
    );
    assert_eq!(
        merge.templates[1].rename_prefix.as_deref(),
        Some("P1.Template.B_0.")
    );
    assert_eq!(merge.templates[0].fields, vec!["tax", "city"]);
    // 跨模板同名冲突走**独立类型化列表**（不是 `FdfPagesIssue`），因为
    // L25096 明说「The Rename flag does not define a renaming algorithm」：
    // 只报告，不判定违例。
    let conflict = &merge.conflicts[0];
    assert_eq!(conflict.name, "tax");
    assert_eq!(conflict.sources, vec![(0, 0), (1, 0)], "按声明序");
    assert!(conflict.all_rename, "两个模板的 /Rename 都生效");
    // 报告里没有任何诊断：冲突不伪装成 `FdfPagesIssue`，结构本身合法。
    assert!(merge.issues.is_empty(), "{:?}", merge.issues);
    assert!(merge.advisories().is_empty());
    assert!(merge.violations().is_empty());
}

#[test]
fn fdf_import_merge_rename_false_keeps_duplicate_names() {
    // Table 249 `/Rename` false ⇒ J.2 说字段不改名，结果是目标文档里
    // 多个同名并存（L37509–L37510），且 `all_rename` 变为 false。
    let merge = merge_of(
        vec![
            page(&[(
                "Templates",
                array(vec![template(&[
                    ("TRef", named_page("A")),
                    ("Fields", array(vec![field("tax")])),
                    ("Rename", Value::Bool(false)),
                ])]),
            )]),
            page_with("B", &["tax"]),
        ],
        Vec::new(),
    );
    assert!(!merge.templates[0].rename, "/Rename false 生效");
    assert!(merge.templates[1].rename, "缺省即 true（不物化）");
    assert_eq!(merge.templates[0].rename_prefix, None);
    assert_eq!(
        merge.conflicts,
        vec![FdfFieldNameConflict {
            name: "tax".to_string(),
            sources: vec![(0, 0), (1, 0)],
            all_rename: false,
        }]
    );
}

#[test]
fn fdf_import_merge_conflicts_follow_declaration_order() {
    // 冲突按名字**首次出现**的声明序排列，模板位置 `(页, 页内模板)`。
    let two = |names: &[&str]| {
        let items: Vec<Value> = names.iter().map(|name| field(name)).collect();
        array(vec![template(&[
            ("TRef", named_page("T")),
            ("Fields", array(items)),
        ])])
    };
    let merge = merge_of(
        vec![
            page(&[("Templates", two(&["zeta", "alpha"]))]),
            page(&[("Templates", two(&["alpha", "zeta"]))]),
        ],
        Vec::new(),
    );
    let names: Vec<&str> = merge
        .conflicts
        .iter()
        .map(|conflict| conflict.name.as_str())
        .collect();
    assert_eq!(names, vec!["zeta", "alpha"], "按首次出现的声明序");
    assert_eq!(merge.conflicts[0].sources, vec![(0, 0), (1, 0)]);
    assert!(merge.conflicts.iter().all(|conflict| conflict.all_rename));
}

#[test]
fn fdf_import_merge_repeated_template_name_is_advisory() {
    // NOTE L25094「the same template page is imported more than once」：
    // 两个模板引用同一个命名页（字段树对象号不能重复引用，见 limits）。
    let merge = merge_of(
        vec![page(&[(
            "Templates",
            array(vec![
                template(&[
                    ("TRef", named_page("Dup")),
                    ("Fields", array(vec![field("a")])),
                ]),
                template(&[("TRef", named_page("Dup"))]),
            ]),
        )])],
        Vec::new(),
    );
    let repeat = FdfPagesIssue::TemplateFieldNameConflict {
        name: "template \"Dup\"".to_string(),
        sources: vec![(0, 0), (0, 1)],
    };
    assert!(
        merge.advisories().contains(&&repeat),
        "{:?}",
        merge.advisories()
    );
    assert!(
        !merge.violations().contains(&&repeat),
        "重名导入不是 shall 违例：{:?}",
        merge.violations()
    );
    assert_eq!(merge.templates.len(), 2);
    assert_eq!(merge.templates[1].name.as_deref(), Some("Dup"));
    assert!(merge.templates[1].fields.is_empty());
}

#[test]
fn fdf_import_merge_annotation_page_outside_pages_is_advisory_only() {
    // Table 251 `/Page` 指向**源文档**页（L25100–L25101），`/Pages` 描述
    // **目标文档**新页（L25056）⇒ 只作提示性交叉核对。
    let merge = merge_of(
        vec![page(&[("Templates", array(vec![]))])],
        vec![
            annotation(&[("Page", integer(3))]),
            annotation(&[("Page", integer(0))]),
        ],
    );
    let advisories = merge.advisories();
    assert!(
        advisories.contains(&&FdfPagesIssue::AnnotationPageNotInPagesArray {
            index: 0,
            page: 3,
            declared_pages: 1,
        }),
        "{advisories:?}"
    );
    assert!(
        merge
            .violations()
            .iter()
            .all(|issue| !matches!(issue, FdfPagesIssue::AnnotationPageNotInPagesArray { .. })),
        "{:?}",
        merge.violations()
    );
    assert!(
        advisories[0]
            .to_string()
            .contains("this is not a violation"),
        "{}",
        advisories[0]
    );
}

#[test]
fn fdf_import_merge_without_pages_never_claims_annotation_advisory() {
    // 只有 `/Annots` 时不评估交叉核对（否则任何 `/Page` 都是噪声）。
    let merge = merge_of(Vec::new(), vec![annotation(&[("Page", integer(9))])]);
    assert!(merge.advisories().is_empty(), "{:?}", merge.advisories());
    assert!(merge.violations().is_empty());
    assert!(merge.templates.is_empty());
}

#[test]
fn fdf_import_merge_collects_required_entry_violations() {
    // 报告里的 `violations()` 覆盖四张表全部的 Required 条目违例。
    let merge = merge_of(
        vec![page(&[])],
        vec![dict_value(&[("Subtype", name_entry("Text"))])],
    );
    let violations = merge.violations();
    assert!(
        violations
            .iter()
            .filter(|issue| issue.is_required_entry_missing())
            .count()
            >= 2,
        "{violations:?}"
    );
    assert!(
        violations.contains(&&FdfPagesIssue::PageTemplatesMissing { page: 0 }),
        "{violations:?}"
    );
    assert!(
        violations.contains(&&FdfPagesIssue::AnnotationPageMissing {
            index: 0,
            subtype: Some("Text".to_string()),
        }),
        "{violations:?}"
    );
}

#[test]
fn fdf_import_merge_children_names_are_listed_with_their_root() {
    // 模板导入的名包含 `/Kids` 后代（Table 246），一次导入里同名只算一个
    // 来源（J.2 L37509–L37511「all fields with that name shall be
    // updated」）。
    let root = Dict::new()
        .insert("T", text("person"))
        .insert("Kids", array(vec![field("name")]));
    let merge = merge_of_with(
        vec![page(&[(
            "Templates",
            array(vec![template(&[
                ("TRef", named_page("T")),
                ("Fields", array(vec![Value::Ref(3, 0)])),
            ])]),
        )])],
        Vec::new(),
        vec![Object::new(3, 0, Value::Dict(root))],
    );
    assert_eq!(merge.templates[0].fields, vec!["person", "person.name"]);
    assert!(merge.imports("person.name"));
}
