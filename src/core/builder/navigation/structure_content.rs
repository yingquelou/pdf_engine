//! 结构标记内容与逻辑结构树的构建期交叉校验（ISO 32000-1 14.7.4.2、14.7.4.4、
//! 14.8.2.2.1）。
//!
//! [`PdfBuilder::begin_structure_content`] 在页面内容流写入
//! `/<tag> << /MCID n >> BDC`，`builder/navigation/structure.rs` 只按结构树
//! 的 MCR 子项生成 `/ParentTree` 与页面 `/StructParents`；两条通道必须由
//! 同一 MCID 对齐，否则读端（`Document::page_struct_parents` 与 14.8.2 的
//! 归属诊断）无法把标记内容定位到结构元素。本模块在
//! `assemble_document` 里、任何对象号分配与写出之前做该一致性预检，失败
//! 保持零字节契约。
//!
//! 校验按页进行，且**页面以「至少标出一个 MCID」选择加入**：内容流里一个
//! `/MCID` 都没有的页面完全不参与比对。这是刻意的非对称——既有写端用例只
//! 声明结构树 MCR 而不写页面流，若反向检查（树声明的 MCID 必须在页上找到）
//! 覆盖到零 MCID 的页面，那些用例会被误判。选择加入后该页即要求双向一致：
//!
//! - 树上声明、页上未标记 → missing；
//! - 页上标记、树上无元素声明 → orphan。
//!
//! 其余规则：同页 MCID 必须唯一；页上有 MCID 时文档必须有结构树；标签为
//! `Artifact` 的标记内容带 `/MCID` 直接拒绝（14.8.2.3.1，与读端归属诊断的
//! `ArtifactMarkedAsStructure` 同一约束，写端更早失败）。

use std::collections::{BTreeMap, BTreeSet};

use super::super::types::{StructureElement, StructureKid, StructureTree};

/// 校验页面内容流的 `/MCID` 集合与结构树 MCR 子项是否逐页一致。
///
/// `page_mcids` 由调用点从内容流 BDC 的内联属性表提取（页 0 基索引 → 按
/// 出现顺序的 MCID 列表）；本函数只做集合比对，返回第一条不一致的人类可
/// 读信息，由调用点包装为 [`WriteError::InvalidObject`](crate::core::writer::WriteError)。
pub(in crate::core::builder) fn validate(
    page_mcids: &BTreeMap<usize, Vec<i64>>,
    tree: Option<&StructureTree>,
) -> Result<(), String> {
    let Some((page, mcids)) = page_mcids.first_key_value() else {
        return Ok(());
    };
    let Some(tree) = tree else {
        return Err(format!(
            "page {page} marks MCID {} but the document has no structure tree",
            mcids[0]
        ));
    };
    let mut claimed: BTreeMap<usize, BTreeSet<i64>> = BTreeMap::new();
    for element in &tree.elements {
        collect(element, &mut claimed);
    }
    for (page, mcids) in page_mcids {
        let content: BTreeSet<i64> = mcids.iter().copied().collect();
        if content.len() != mcids.len() {
            let mut seen = BTreeSet::new();
            let duplicate = mcids
                .iter()
                .copied()
                .find(|mcid| !seen.insert(*mcid))
                .unwrap_or_default();
            return Err(format!(
                "page {page} marks MCID {duplicate} more than once; MCIDs must be unique per page"
            ));
        }
        let empty = BTreeSet::new();
        let tree_mcids = claimed.get(page).unwrap_or(&empty);
        let missing: Vec<i64> = tree_mcids.difference(&content).copied().collect();
        let orphan: Vec<i64> = content.difference(tree_mcids).copied().collect();
        if missing.is_empty() && orphan.is_empty() {
            continue;
        }
        let mut message =
            format!("page {page} marked content does not agree with the structure tree");
        if !missing.is_empty() {
            message.push_str(&format!(
                "; structure elements claim MCID {missing:?} that the page never marks"
            ));
        }
        if !orphan.is_empty() {
            message.push_str(&format!(
                "; the page marks MCID {orphan:?} that no structure element claims"
            ));
        }
        return Err(message);
    }
    Ok(())
}

/// 递归收集一个元素及其子孙的 MCR 子项：页索引 → MCID 集合。
fn collect(element: &StructureElement, claimed: &mut BTreeMap<usize, BTreeSet<i64>>) {
    for kid in &element.kids {
        match kid {
            StructureKid::Element(child) => collect(child, claimed),
            StructureKid::MarkedContent { page, mcid } => {
                claimed.entry(*page).or_default().insert(*mcid);
            }
        }
    }
}
