//! FDF `/Pages` 展开的**不可信输入防护**：单数组条目上限、跨层共享的全局
//! 条目预算，以及模板 `/Fields` 复用字段树时的引用环与深度上限。
//!
//! 上限都是**库策略**（规范未给数组长度上限；数组长度、模板数与字段数都是
//! 文件提供的不可信数字），与 `embedded/limits.rs` 同一动机：单数组上限只
//! 约束一个数组，`页 × 模板 × 字段` 相乘仍是指数级，故再加一个跨所有
//! 嵌套层共享的全局预算。触顶一律降级为类型化诊断
//! （[`FdfPagesIssue::ArrayTruncated`] / [`FdfPagesIssue::BudgetExhausted`]）
//! 并**停止该数组的剩余项**，已类型化的层与原值都不受影响。

use super::*;
use crate::core::reader::fdf::pages::{MAX_FDF_PAGES_ARRAY_ENTRIES, MAX_FDF_PAGES_TOTAL_ENTRIES};

#[test]
fn fdf_pages_array_cap_truncates_tail_with_diagnostic() {
    let extra = 3;
    let items: Vec<Value> = (0..MAX_FDF_PAGES_ARRAY_ENTRIES + extra)
        .map(|_| page(&[("Templates", array(vec![]))]))
        .collect();
    let view = pages_view_of(array(items), Vec::new());
    assert_eq!(view.len(), MAX_FDF_PAGES_ARRAY_ENTRIES);
    let truncated: Vec<&FdfPagesIssue> = view
        .issues
        .iter()
        .filter(|issue| matches!(issue, FdfPagesIssue::ArrayTruncated { .. }))
        .collect();
    assert_eq!(
        truncated.len(),
        1,
        "触顶诊断只记一次，不逐项重复：{:?}",
        view.issues
    );
    assert_eq!(
        truncated[0],
        &FdfPagesIssue::ArrayTruncated {
            key: "Pages",
            limit: MAX_FDF_PAGES_ARRAY_ENTRIES,
        }
    );
    assert!(truncated[0].is_limit());
    assert!(
        truncated[0]
            .to_string()
            .contains("library guard, not a spec rule"),
        "{}",
        truncated[0]
    );
}

#[test]
fn fdf_templates_array_cap_is_checked_per_page_with_own_key() {
    // 页内 `/Templates` 数组同样受单数组上限约束，且诊断的 key 不同。
    let templates: Vec<Value> = (0..MAX_FDF_PAGES_ARRAY_ENTRIES + 1)
        .map(|index| template(&[("TRef", named_page(&format!("T{index}")))]))
        .collect();
    let view = pages_view_of(
        array(vec![
            page(&[("Templates", array(templates))]),
            page(&[("Templates", array(vec![]))]),
        ]),
        Vec::new(),
    );
    assert_eq!(view.entries[0].templates.len(), MAX_FDF_PAGES_ARRAY_ENTRIES);
    assert!(
        view.entries[0].issues.iter().any(|issue| matches!(
            issue,
            FdfPagesIssue::ArrayTruncated {
                key: "Templates",
                ..
            }
        )),
        "{:?}",
        view.entries[0].issues
    );
    // 第二页自己的数组不受影响。
    assert!(
        view.entries[1].issues.iter().all(|issue| !issue.is_limit()),
        "{:?}",
        view.entries[1].issues
    );
}

#[test]
fn fdf_pages_global_budget_stops_field_expansion() {
    // 6 页 × 1 模板 × 200 字段：先扣掉 6 个页名额与 6 个模板名额，
    // 剩下的全局预算只够类型化 `MAX_FDF_PAGES_TOTAL_ENTRIES - 12` 个字段。
    let fields: Vec<Value> = (0..200).map(|_| field("f")).collect();
    let pages: Vec<Value> = (0..6)
        .map(|_| {
            page(&[(
                "Templates",
                array(vec![template(&[
                    ("TRef", named_page("T")),
                    ("Fields", array(fields.clone())),
                ])]),
            )])
        })
        .collect();
    let view = pages_view_of(array(pages), Vec::new());
    assert_eq!(view.len(), 6, "页名额在触顶前已全部申请到");
    assert_eq!(
        view.template_fields().len(),
        MAX_FDF_PAGES_TOTAL_ENTRIES - 12,
        "全局预算要精确到剩余名额"
    );
    let issues = pages_issues(&view);
    assert!(
        issues.contains(&FdfPagesIssue::BudgetExhausted {
            key: "TemplateFields",
        }),
        "预算耗尽应有类型化诊断：{:?}",
        issues
            .iter()
            .filter(|issue| issue.is_limit())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        issues
            .iter()
            .filter(|issue| matches!(issue, FdfPagesIssue::BudgetExhausted { .. }))
            .count(),
        1,
        "每个数组只记一次"
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.to_string().contains("global budget")),
        "预算诊断要说明是全局预算：{issues:?}"
    );
}

#[test]
fn fdf_template_field_cycle_is_downgraded_to_diagnostic() {
    // Table 249 模板字段复用读端字段树的环检测（`parse::Walker`）。
    let looping = Dict::new()
        .insert("T", text("loop"))
        .insert("Kids", array(vec![Value::Ref(3, 0)]));
    let view = pages_view_of(
        array(vec![page(&[(
            "Templates",
            array(vec![template(&[
                ("TRef", named_page("T")),
                ("Fields", array(vec![Value::Ref(3, 0)])),
            ])]),
        )])]),
        vec![Object::new(3, 0, Value::Dict(looping))],
    );
    let item = &view.page(0).expect("页").templates[0];
    assert!(item.fields.is_empty(), "环上的字段不进入类型化列表");
    let skipped = item
        .issues
        .iter()
        .find(|issue| matches!(issue, FdfPagesIssue::TemplateFieldSkipped { .. }))
        .expect("环要降级为诊断而非报错");
    assert!(skipped.to_string().contains("cycle"), "{skipped}");
    // 数组原值仍在模板字典里，不丢弃。
    assert_eq!(
        item.entry("Fields"),
        Some(&array(vec![Value::Ref(3, 0)])),
        "{:?}",
        item.dict
    );
}

#[test]
fn fdf_template_field_depth_limit_is_reported_per_entry() {
    // 与 `/Fields`（Table 243）同一深度上限：超深字段项被跳过并记诊断，
    // 同一模板里的其它字段照常类型化。
    let mut current = field("leaf");
    for _ in 0..MAX_FDF_FIELD_DEPTH + 2 {
        current = Value::Dict(
            Dict::new()
                .insert("T", text("n"))
                .insert("Kids", array(vec![current])),
        );
    }
    let view = pages_view_of(
        array(vec![page(&[(
            "Templates",
            array(vec![template(&[
                ("TRef", named_page("T")),
                ("Fields", array(vec![current, field("ok")])),
            ])]),
        )])]),
        Vec::new(),
    );
    let item = &view.page(0).expect("页").templates[0];
    let names: Vec<&str> = item
        .fields
        .iter()
        .map(|field| field.fully_qualified_name.as_str())
        .collect();
    assert_eq!(names, vec!["ok"], "超深项被跳过、后续项不受牵连");
    assert!(
        item.issues.iter().any(|issue| matches!(
            issue,
            FdfPagesIssue::TemplateFieldSkipped {
                page: 0,
                template: 0,
                field: 0,
                ..
            }
        ) && issue.to_string().contains("depth")),
        "{:?}",
        item.issues
    );
}

#[test]
fn fdf_import_merge_shares_one_budget_across_pages_and_annots() {
    // 单独调用 `fdf_pages()` 与 `fdf_annots()` 各用一份预算；
    // `fdf_import_merge()` 把两个数组放在**同一次展开**里，共享一个全局
    // 预算（放大防护的口径与 `embedded::ExpansionBudget` 一致）。
    let fields: Vec<Value> = (0..200).map(|_| field("f")).collect();
    let pages: Vec<Value> = (0..4)
        .map(|_| {
            page(&[(
                "Templates",
                array(vec![template(&[
                    ("TRef", named_page("T")),
                    ("Fields", array(fields.clone())),
                ])]),
            )])
        })
        .collect();
    let annots: Vec<Value> = (0..500)
        .map(|index| annotation(&[("Page", integer(index as i64))]))
        .collect();
    let document = doc_with_fdf_entries(
        Dict::new()
            .insert("Pages", array(pages))
            .insert("Annots", array(annots)),
        Vec::new(),
    );
    // 单独调用互不占用：500 条注解全部类型化。
    let standalone = document.fdf_annots().expect("ok").expect("视图");
    assert_eq!(standalone.len(), 500);
    assert!(standalone.issues.iter().all(|issue| !issue.is_limit()));
    // 合并报告里 `/Pages` 先占用名额，`/Annots` 触顶并记诊断。
    let merge = document.fdf_import_merge().expect("ok").expect("报告");
    assert_eq!(merge.templates.len(), 4);
    assert_eq!(
        merge
            .issues
            .iter()
            .filter(|issue| matches!(issue, FdfPagesIssue::BudgetExhausted { key: "Annots" }))
            .count(),
        1,
        "{:?}",
        merge
            .issues
            .iter()
            .filter(|issue| issue.is_limit())
            .collect::<Vec<_>>()
    );
}
