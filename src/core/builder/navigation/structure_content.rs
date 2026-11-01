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
//!
//! 14.7.4.4 的 MCID 编号与父树键策略补充：
//!
//! - `PdfBuilder::begin_auto_structure_content` 自动编号通道按页取
//!   「显式 ∪ 已自动分配」之外的最小非负整数（NOTE「their assigned values
//!   should be as small as possible」），纯自动编号的页面即从 0 起稠密递增，
//!   `/ParentTree` 数组无 `null` 空洞；[`McidRegistry`] 同时登记显式与自动
//!   两路来源，`begin_structure_content` 复用本页已自动分配的标识符时记入
//!   撞号信息，由 `PdfBuilder::build` 在任何对象号分配之前以
//!   [`WriteError::InvalidObject`](crate::core::writer::WriteError) 拒绝。
//! - 选择加入页面的 MCID 集合若不是 `0..n-1` 连续区间，父树数组按未声称
//!   槽位写出 `null`（既有显式稀疏形态受支持：读端按 MCID 定位数组元素，
//!   NOTE 只是 should 级）；空洞超过 [`MAX_PARENT_TREE_NULL_SLOTS`] 的极端
//!   稀疏在构建期报错，避免写出病态膨胀的数组。
//! - [`validate_next_key`] 复核 shall：「The ParentTreeNextKey entry …
//!   shall hold an integer value greater than any that is currently in use
//!   as a key」，并印证写端「新条目以当前 NextKey 为键、随后自增」的连续
//!   键策略（页面键 `0..p-1`、OBJR 键接在其后）。

use std::collections::{BTreeMap, BTreeSet};

use super::super::types::{StructureElement, StructureKid, StructureTree};

/// 自动 MCID 分配上限：与 `structure.rs` 的 `MAX_STRUCTURE_MCID` 同源——
/// 父树数组按 MCID 作零基下标，65535 为最大可用标识符。
const MAX_AUTO_MCID: i64 = super::structure::MAX_STRUCTURE_MCID;

/// 选择加入页面 `/ParentTree` 数组允许的空洞数上限。稀疏显式 MCID 是既有
/// 支持形态（14.7.4.4 NOTE 为 should 级，读端按 MCID 定位数组元素），故只
/// 对超出合理上限的极端稀疏报构建期错误；既有用例的最大显式稀疏（0 与 7）
/// 为 6 个空洞，远低于该阈值不受影响。
const MAX_PARENT_TREE_NULL_SLOTS: i64 = 64;

/// 自动编号通道的构建期 MCID 登记：`PdfBuilder::begin_structure_content`
/// 的显式传入与 `PdfBuilder::begin_auto_structure_content` 的自动分配按页
/// 记录，供最小未用分配与撞号拒绝。
#[derive(Debug, Default)]
pub(in crate::core::builder) struct McidRegistry {
    /// 页 0 基索引 → 显式传入过的 MCID 集合。
    explicit: BTreeMap<usize, BTreeSet<i64>>,
    /// 页 0 基索引 → 已自动分配的 MCID 集合。
    auto: BTreeMap<usize, BTreeSet<i64>>,
    /// 首次发现的自动/显式撞号信息；`build` 在对象号分配之前取出并拒绝。
    conflict: Option<String>,
}

impl McidRegistry {
    /// 登记一次显式传入的 MCID；若该值已被本页自动分配，记录撞号信息——
    /// 错误类型与既有交叉校验一致（[`WriteError::InvalidObject`](crate::core::writer::WriteError)），
    /// 由 `PdfBuilder::build` 在任何对象号分配之前消费，保持零字节契约。
    pub(in crate::core::builder) fn note_explicit(&mut self, page: usize, mcid: i64) {
        if self.conflict.is_none()
            && self
                .auto
                .get(&page)
                .is_some_and(|used| used.contains(&mcid))
        {
            self.conflict = Some(format!(
                "page {page} MCID {mcid} was allocated by begin_auto_structure_content and is re-marked by begin_structure_content; auto and explicit MCIDs must not collide"
            ));
        }
        self.explicit.entry(page).or_default().insert(mcid);
    }

    /// 为指定页分配最小未用 MCID（显式 ∪ 自动之外的最小非负整数，
    /// 14.7.4.4 NOTE「as small as possible」），登记为自动值后返回；
    /// 分配值超出上限时返回 `Err` 且不占用任何标识符。
    pub(in crate::core::builder) fn allocate_auto(&mut self, page: usize) -> Result<i64, String> {
        let empty = BTreeSet::new();
        let explicit = self.explicit.get(&page).unwrap_or(&empty);
        let auto = self.auto.get(&page).unwrap_or(&empty);
        let mut candidate = 0i64;
        while explicit.contains(&candidate) || auto.contains(&candidate) {
            candidate += 1;
        }
        if candidate > MAX_AUTO_MCID {
            return Err(format!(
                "page {page} has no free auto MCID at or below the limit {MAX_AUTO_MCID}"
            ));
        }
        self.auto.entry(page).or_default().insert(candidate);
        Ok(candidate)
    }

    /// 取出首次撞号信息（至多一条；后续撞号沿用首条，与「返回第一条
    /// 不一致信息」的既有校验风格一致）。
    pub(in crate::core::builder) fn take_conflict(&mut self) -> Option<String> {
        self.conflict.take()
    }
}

/// 复核 `/ParentTreeNextKey` 严格大于全部在用父树键（14.7.4.4 shall：
/// 「The ParentTreeNextKey entry … shall hold an integer value greater
/// than any that is currently in use as a key in the structural parent
/// tree」）。
///
/// 写端键策略本身满足「新条目以当前 NextKey 作键、随后自增」：页面键按
/// 页升序 `0..p-1`、OBJR 键接在其后连续分配，NextKey 写出为条目总数。
/// 本函数在结构树组装时、任何结构对象号分配之前复核该不变量；当前没有
/// 任何公共 API 允许调用方注入自定义 NextKey，因此该检查是防线而非日常
/// 路径，键策略一旦回归即零字节拒绝。
pub(in crate::core::builder) fn validate_next_key(
    keys: &[i64],
    next_key: i64,
) -> Result<(), String> {
    let Some(max) = keys.iter().copied().max() else {
        return Ok(());
    };
    if next_key <= max {
        return Err(format!(
            "/ParentTreeNextKey {next_key} must be greater than any key in use (largest in-use key {max}) (14.7.4.4)"
        ));
    }
    Ok(())
}

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
            // 页面已选择加入且双向一致：此时树上与页上的 MCID 集合相同，
            // 即写出父树数组的槽位集合。若该集合不是 0..n-1 连续区间，
            // 未声称的低位槽写出 null（合法但次优，14.7.4.4 NOTE 为
            // should 级）；只有超出合理上限的极端稀疏才构建期报错。
            // 未选择加入（页上零 MCID）的树声明不比对、也不做该检查，
            // 维持既有非对称豁免——其 MCID 形态门禁由 structure.rs 的
            // 计划阶段负责（负值/超上限报错信息保持既有文案）。
            let count = tree_mcids.len() as i64;
            if let Some(&max) = tree_mcids.iter().next_back() {
                let holes = max + 1 - count;
                if holes > MAX_PARENT_TREE_NULL_SLOTS {
                    return Err(format!(
                        "page {page} claims MCID {max} with only {count} identifiers, leaving {holes} null slots in its /ParentTree array (limit {MAX_PARENT_TREE_NULL_SLOTS}); prefer begin_auto_structure_content for dense numbering"
                    ));
                }
            }
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
            StructureKid::MarkedContent { page, mcid }
            | StructureKid::InlineMarkedContent { page, mcid } => {
                claimed.entry(*page).or_default().insert(*mcid);
            }
            // OBJR 内容项不占用 MCID（14.7.4.3，Table 325）。
            StructureKid::ObjectReference { .. } => {}
        }
    }
}
