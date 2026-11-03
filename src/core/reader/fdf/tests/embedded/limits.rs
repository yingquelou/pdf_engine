//! `/EmbeddedFDFs` 递归的**放大防护**与子文档 `/Differences` 就地类型化。
//!
//! 单数组条目上限（[`MAX_FDF_EMBEDDED_ENTRIES`]）与层数上限
//! （[`MAX_FDF_EMBEDDED_DEPTH`]）各自只约束「一个数组」和「一条链」，扇出
//! 相乘仍是 `breadth^depth` 量级，故这里覆盖跨层共享的全局条目预算
//! [`MAX_FDF_EMBEDDED_TOTAL_ENTRIES`]；另外补齐递归子树里**每一层**
//! `/Differences` 的类型化（子 [`Document`] 不落进视图，调用方无法自行
//! 解引用那个对象号）。

use super::*;
use crate::core::reader::fdf::embedded::MAX_FDF_EMBEDDED_TOTAL_ENTRIES;

/// 逐层扇出的内层 FDF 字节：`level` 是剩余层数，每层 `breadth` 个分支，
/// 每个分支都是**独立**的文件规范与载体流（同一数组内的重复对象号会被
/// 环检测拦下，故这里必须逐项新建对象）。`level == 0` 是叶子（无
/// `/EmbeddedFDFs`）。
fn branching_fdf_bytes(level: usize, breadth: usize) -> Vec<u8> {
    if level == 0 {
        return b"1 0 obj\n<< /FDF << /Status (leaf) >> >>\nendobj\n%%EOF\n".to_vec();
    }
    let inner = branching_fdf_bytes(level - 1, breadth);
    let refs: Vec<String> = (0..breadth)
        .map(|index| format!("{} 0 R", 2 + index * 2))
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "1 0 obj\n<< /FDF << /Status (branch{level}) /EmbeddedFDFs [ {} ] >> >>\nendobj\n",
            refs.join(" ")
        )
        .as_bytes(),
    );
    for index in 0..breadth {
        let spec = 2 + index * 2;
        let carrier = spec + 1;
        out.extend_from_slice(
            format!("{spec} 0 obj\n<< /F (b{level}-{index}.fdf) /EF {carrier} 0 R >>\nendobj\n")
                .as_bytes(),
        );
        out.extend_from_slice(
            format!("{carrier} 0 obj\n<< /Length {} >>\nstream\n", inner.len()).as_bytes(),
        );
        out.extend_from_slice(&inner);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    }
    out.extend_from_slice(b"\n%%EOF\n");
    out
}

/// 整棵类型化视图树里的条目总数（含所有递归层）。
fn count_typed(view: &FdfEmbeddedFdfs) -> usize {
    view.entries
        .iter()
        .map(|entry| {
            1 + entry
                .nested
                .as_ref()
                .map(|nested| count_typed(nested))
                .unwrap_or(0)
        })
        .sum()
}

/// 收集整棵视图树的诊断（数组级 + 条目级）。
fn collect_issues(view: &FdfEmbeddedFdfs, out: &mut Vec<String>) {
    out.extend(view.issues.iter().cloned());
    for entry in &view.entries {
        out.extend(entry.issues.iter().cloned());
        if let Some(nested) = entry.nested.as_deref() {
            collect_issues(nested, out);
        }
    }
}

#[test]
fn embedded_fdfs_global_budget_caps_the_whole_expansion_tree() {
    // 5 个顶层条目 × 4 层扇出 = 5 + 25 + 125 + 625 = 780 个可展开条目；
    // 全局预算把整棵树的类型化条目数**精确**截在
    // MAX_FDF_EMBEDDED_TOTAL_ENTRIES，且已类型化的浅层一条诊断都不带。
    const BREADTH: usize = 5;
    let carrier = branching_fdf_bytes(3, BREADTH);
    let mut objects = Vec::new();
    let mut items = Vec::new();
    for index in 0..BREADTH {
        let spec = 6 + index as i64 * 2;
        objects.extend(referenced_filespec(
            spec,
            spec + 1,
            &format!("branch{index}.fdf"),
            carrier.clone(),
        ));
        items.push(Value::Ref(spec, 0));
    }
    let document = with_embedded_fdfs(field_array(items), objects);
    let view = embedded_of(&document);
    assert_eq!(
        count_typed(&view),
        MAX_FDF_EMBEDDED_TOTAL_ENTRIES,
        "全局预算必须精确截断整棵树的条目总数"
    );
    // 展开是按声明序深度优先的：预算耗在第一条链上时，连顶层数组都会被
    // 截断——这正是「跨层共享」而非「逐层各自 256」的可观察证据。
    assert!(
        view.len() < BREADTH,
        "顶层数组本应被全局预算截断，实际类型化 {}",
        view.len()
    );
    for entry in &view.entries {
        let child = entry
            .document
            .as_ref()
            .expect("已类型化的浅层条目应有子文档");
        assert_eq!(child.fdf.status.as_deref(), Some("branch3"));
        assert!(entry.issues.is_empty(), "got {:?}", entry.issues);
        assert!(entry.nested.is_some(), "浅层应带出下一层视图");
    }
    let mut issues = Vec::new();
    collect_issues(&view, &mut issues);
    assert!(
        issues.iter().any(|issue| issue.contains("global budget")),
        "截断必须留下可诊断的记录：{issues:?}"
    );
    assert!(
        issues
            .iter()
            .all(|issue| !issue.contains("recursion limit") && !issue.contains("tail was skipped")),
        "全局预算截断不该伪装成层数或单数组上限：{issues:?}"
    );
}

#[test]
fn nested_child_document_differences_is_typed_during_recursion() {
    // 子文档的 `/Differences` 在递归时就地类型化：[`FdfDocument`] 里保留
    // 的原值只是一个对象号（子 `Document` 不落进视图，调用方无处解引用）。
    let inner = b"1 0 obj\n<< /FDF << /Status (kid) /Differences 2 0 R >> >>\nendobj\n\
                  2 0 obj\n<< /Length 5 /Custom (kept) >>\nstream\ndelta\nendstream\nendobj\n%%EOF\n";
    let spec = Dict::new()
        .insert("F", Value::String("kid.fdf".to_string()))
        .insert("EF", carrier_stream(inner.to_vec(), Dict::new()));
    let document = with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new());
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    let child = entry.document.as_ref().expect("子文档应被递归解析");
    assert_eq!(
        child.fdf.differences,
        Some(Value::Ref(2, 0)),
        "子模型里的原值不丢"
    );
    let differences = entry
        .differences
        .as_ref()
        .expect("子文档 /Differences 应就地类型化");
    assert!(differences.is_stream(), "引用应被解引用到子文档的流对象");
    assert_eq!(differences.reference, Some((2, 0)));
    assert_eq!(differences.declared_length(), Some(5));
    assert_eq!(
        differences.entry("Custom"),
        Some(&Value::String("kept".to_string())),
        "子流字典的未建模条目原值保留"
    );
    let joined = differences.issues.join("|");
    assert!(
        !joined.contains("declares a stream") && !joined.contains("could not be resolved"),
        "got {joined}"
    );
    // 内层流字节随载体解码内联，故过滤链入口照常可用（以调用方手里的
    // 外层 `Document` 为上下文；FDF 侧 RC4 一律不解）。
    assert_eq!(
        differences
            .decoded_bytes(&document)
            .expect("内联字节可直接解码"),
        b"delta".to_vec()
    );
    // 外层自己没有 `/Differences`：顶层视图安静 None。
    assert_eq!(document.fdf_differences().expect("ok"), None);
}

#[test]
fn nested_child_without_differences_leaves_the_view_none() {
    // 子文档未声明 `/Differences` 时不物化视图（与顶层「条目缺失返回
    // None」同一口径）。
    let spec = Dict::new()
        .insert("F", Value::String("plain-kid.fdf".to_string()))
        .insert("EF", carrier_stream(inner_fdf_bytes("plain"), Dict::new()));
    let document = with_embedded_fdfs(field_array(vec![Value::Dict(spec)]), Vec::new());
    let view = embedded_of(&document);
    let entry = first_entry(&view);
    assert!(entry.document.is_some(), "子文档本身仍应解析");
    assert!(entry.differences.is_none());
    assert!(entry.nested.is_none());
    assert!(entry.issues.is_empty(), "got {:?}", entry.issues);
}
