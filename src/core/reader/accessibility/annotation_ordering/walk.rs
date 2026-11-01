//! 14.8.2.3.2 的遍历实现（内部模块，经 [`super`] 暴露给公开 API）。
//!
//! [`StructureWalk`] 把「逻辑结构序」落成三类可查表：
//!
//! - 每个结构元素在本页认领的标记内容区段（按 `/K` 子项位置升序）；
//! - 每个 OBJR 对本页 `/Annots` 的认领（同一注解被重复声称时取结构序中的
//!   第一次，后续忽略，保证输出确定）；
//! - 「认领了内容项的元素」集合与每棵子树的 arena 区间，用于判定 14.8.2.3.2
//!   的「紧接在前 / 紧接在后」。
//!
//! 结构元素 arena 按前序排列，故 arena 下标即逻辑结构序；子树在 arena 上是
//! 连续区间，由父链自后向前一趟即可回填。邻接判定只看「认领了内容项」的元素：
//! 跳过祖先（向前方向）/整棵自身子树（向后方向）与没有内容项的元素，遇到第一
//! 个认领了内容项的邻居即定论——它认领标记内容区段则邻接成立，只认领对象引用
//! 则邻接不成立。
//!
//! 生效页的解析（`/Pg` 为 Table 323 的可选条目，缺省语义规范未规定）：MCR 的
//! `/Pg` 覆盖元素的 `/Pg`，OBJR 的 `/Pg` 覆盖元素的 `/Pg`，整数 MCID 子项沿用
//! 元素的 `/Pg`；元素自身没有 `/Pg` 时沿父链取最近一次声明，环与超过
//! [`MAX_STRUCTURE_DEPTH`] 层视为未声明。未声明的一侧一律按「与本页相容」
//! 处理：MC 由同 MCID 在本页区段里落地，OBJR 由本页 `/Annots` 的成员关系
//! 落地；反过来，声明了别的页的元素不参与本页排序。MCR 带 `/Stm` 或
//! `/StmOwn` 时序列不在页面流中，整条子项忽略。

use std::collections::{BTreeMap, BTreeSet};

use crate::core::reader::content::{MarkedSection, Op};
use crate::core::reader::structure::{
    MAX_STRUCTURE_DEPTH, StandardStructureType, StructureKid, StructureTree,
};

/// 一条 OBJR 对本页注解的认领记录。
#[derive(Debug, Clone, Copy)]
pub(super) struct Claim {
    /// 声称元素的 arena 下标（前序序即逻辑结构序）。
    pub(super) element: usize,
    /// `/K` 中 OBJR 子项的位置。
    pub(super) kid: usize,
    /// 本页 `/Annots` 下标。
    pub(super) annotation: usize,
    /// 声称元素是否为 `Annot`/`Link`/`Form`（显式关联档）。
    pub(super) explicit: bool,
}

/// 邻接与关联求得的落点；`explicit` 记录该落点是否来自 `Annot`/`Link`/`Form`
/// 元素的显式关联（这类元素自身与子树都没有区段时会退回邻接档）。
#[derive(Debug, Clone, Copy)]
pub(super) enum Placement {
    /// 落在该区段之前。
    Before { section: usize, explicit: bool },
    /// 落在该区段之后。
    After { section: usize, explicit: bool },
    /// 位置不可由逻辑结构导出。
    Undetermined,
}

/// 一个标记内容区段在序列中的条目。
pub(super) struct SectionEntry {
    /// 区段下标（流序）。
    pub(super) section: usize,
    /// 声称该区段的元素 arena 下标。
    pub(super) element: Option<usize>,
    /// `/K` 子项位置。
    pub(super) kid: Option<usize>,
    /// 是否为规范 NOTE 说的定位锚点（空标记内容序列）。
    pub(super) anchor: bool,
}

/// 逻辑结构里与本页有关的内容项认领表。
pub(super) struct StructureWalk {
    /// 按（元素 arena 下标、`/K` 位置）升序的注解认领。
    pub(super) claims: Vec<Claim>,
    /// 区段下标 → 首个认领它的（元素、`/K` 位置）。
    section_claims: BTreeMap<usize, (usize, usize)>,
    /// 元素 arena 下标 → 本页区段（按 `/K` 位置升序）。
    element_sections: BTreeMap<usize, Vec<(usize, usize)>>,
    /// 认领了内容项（区段或本页注解）的元素集合。
    bearing: BTreeSet<usize>,
    /// 每个元素的子树区间右端（前序 arena 上的开区间）。
    subtree_end: Vec<usize>,
    parent: Vec<Option<usize>>,
    explicit: BTreeSet<usize>,
}

impl StructureWalk {
    /// 遍历结构树，收集本页的内容项认领。
    pub(super) fn collect(
        tree: &StructureTree,
        page: Option<i64>,
        sections_by_mcid: &BTreeMap<i64, usize>,
        annotations_by_object: &BTreeMap<i64, usize>,
    ) -> Self {
        let elements = tree.elements();
        let parent: Vec<Option<usize>> = elements.iter().map(|element| element.parent).collect();
        let mut subtree_end = vec![0usize; elements.len()];
        // 前序 arena：自后往前把子树右端并到父元素上，一趟即可（父下标必小于子）。
        for index in (0..elements.len()).rev() {
            subtree_end[index] = subtree_end[index].max(index + 1);
            if let Some(Some(ancestor)) = parent.get(index).copied()
                && ancestor < index
            {
                subtree_end[ancestor] = subtree_end[ancestor].max(subtree_end[index]);
            }
        }
        let mut explicit: BTreeSet<usize> = BTreeSet::new();
        for index in 0..elements.len() {
            if is_association_type(tree, index) {
                explicit.insert(index);
            }
        }
        let mut walk = Self {
            claims: Vec::new(),
            section_claims: BTreeMap::new(),
            element_sections: BTreeMap::new(),
            bearing: BTreeSet::new(),
            subtree_end,
            parent,
            explicit,
        };
        let mut claimed: BTreeSet<usize> = BTreeSet::new();
        for (index, element) in elements.iter().enumerate() {
            let declared = walk.declared_page(tree, index);
            for (kid, child) in element.kids.iter().enumerate() {
                match child {
                    StructureKid::Element(_) => {}
                    // 整数 MCID 简写沿用元素（含祖先）声明的页。
                    StructureKid::Mcid(mcid) => {
                        walk.record_section(index, kid, *mcid, declared, page, sections_by_mcid);
                    }
                    StructureKid::MarkedContent(reference) => {
                        // 序列不在页面流里时无从参与页序（与归属诊断一致）。
                        if reference.stream.is_some() || reference.stream_own.is_some() {
                            continue;
                        }
                        // MCR 的 `/Pg` 覆盖元素的 `/Pg`（Table 324）。
                        let item_page = reference.page.map(|(number, _)| number).or(declared);
                        walk.record_section(
                            index,
                            kid,
                            reference.mcid,
                            item_page,
                            page,
                            sections_by_mcid,
                        );
                    }
                    StructureKid::ObjectReference(reference) => {
                        // OBJR 的 `/Pg` 覆盖元素的 `/Pg`（Table 325）。
                        let item_page = reference.page.map(|(number, _)| number).or(declared);
                        if !page_compatible(item_page, page) {
                            continue;
                        }
                        let Some(&annotation) = annotations_by_object.get(&reference.object.0)
                        else {
                            // `/Obj` 不是本页注解（XObject 或他页注解）。
                            continue;
                        };
                        // 同一注解被重复声称：只认结构序中第一次。
                        if !claimed.insert(annotation) {
                            continue;
                        }
                        walk.bearing.insert(index);
                        walk.claims.push(Claim {
                            element: index,
                            kid,
                            annotation,
                            explicit: walk.explicit.contains(&index),
                        });
                    }
                }
            }
        }
        walk
    }

    /// 本页标记内容区段的序列条目，按 [`Document::page_marked_content`] 的流序。
    pub(super) fn sections(&self, sections: &[MarkedSection], ops: &[Op]) -> Vec<SectionEntry> {
        sections
            .iter()
            .enumerate()
            .map(|(section, entry)| {
                let claim = self.section_claims.get(&section).copied();
                SectionEntry {
                    section,
                    element: claim.map(|(element, _)| element),
                    kid: claim.map(|(_, kid)| kid),
                    // 规范 NOTE：空标记内容序列只用来定位相邻注解，仍要参与排序。
                    anchor: is_empty_sequence(entry, ops),
                }
            })
            .collect()
    }

    /// 求一条注解认领的落点：先显式关联，再邻接，都不成立即不可导出。
    pub(super) fn place(&self, claim: Claim) -> Placement {
        if claim.explicit
            && let Some(placement) = self.explicit_placement(claim)
        {
            return placement;
        }
        self.adjacency(claim.element)
            .unwrap_or(Placement::Undetermined)
    }

    /// 元素自身认领的区段里，按 `/K` 位置离 OBJR 最近的一侧落点；元素自身
    /// 没有区段时退到子树里第一个区段之前（`Annot`/`Link`/`Form` 常以子元素
    /// 携带内容）。
    fn explicit_placement(&self, claim: Claim) -> Option<Placement> {
        let mut preceding: Option<usize> = None;
        let mut following: Option<usize> = None;
        if let Some(entries) = self.element_sections.get(&claim.element) {
            for (kid, section) in entries {
                if *kid < claim.kid {
                    preceding = Some(*section);
                } else if following.is_none() {
                    following = Some(*section);
                }
            }
        }
        if let Some(section) = preceding {
            return Some(Placement::After {
                section,
                explicit: true,
            });
        }
        if let Some(section) = following {
            return Some(Placement::Before {
                section,
                explicit: true,
            });
        }
        for index in (claim.element + 1)..self.subtree_end[claim.element] {
            if let Some(entries) = self.element_sections.get(&index)
                && let Some((_, section)) = entries.first()
            {
                return Some(Placement::Before {
                    section: *section,
                    explicit: true,
                });
            }
        }
        None
    }

    /// 14.8.2.3.2 的邻接句：结构序中紧邻的、认领了内容项的元素。先向前
    /// （注解位于该序列之后），再向后（注解位于该序列之前）。
    fn adjacency(&self, element: usize) -> Option<Placement> {
        let ancestors = self.ancestors(element);
        let mut index = element;
        while index > 0 {
            index -= 1;
            if ancestors.contains(&index) || !self.bearing.contains(&index) {
                continue;
            }
            if let Some(entries) = self.element_sections.get(&index)
                && let Some((_, section)) = entries.last()
            {
                return Some(Placement::After {
                    section: *section,
                    explicit: false,
                });
            }
            // 紧邻的是只认领对象引用的元素：该方向的邻接不成立。
            break;
        }
        for index in self.subtree_end[element].max(element + 1)..self.parent.len() {
            if !self.bearing.contains(&index) {
                continue;
            }
            if let Some(entries) = self.element_sections.get(&index)
                && let Some((_, section)) = entries.first()
            {
                return Some(Placement::Before {
                    section: *section,
                    explicit: false,
                });
            }
            break;
        }
        None
    }

    /// 元素自身声明的页，缺省时沿父链取最近一次声明；环或超深视为未声明。
    fn declared_page(&self, tree: &StructureTree, element: usize) -> Option<i64> {
        let mut current = Some(element);
        let mut visited: BTreeSet<usize> = BTreeSet::new();
        for _ in 0..MAX_STRUCTURE_DEPTH {
            let index = current?;
            if !visited.insert(index) {
                return None;
            }
            if let Some((number, _)) = tree.element(index).and_then(|element| element.page) {
                return Some(number);
            }
            current = self.parent.get(index).copied().flatten();
        }
        None
    }

    /// 祖先集合（邻接扫描时跳过自身所在层级）。
    fn ancestors(&self, element: usize) -> BTreeSet<usize> {
        let mut ancestors = BTreeSet::new();
        let mut current = self.parent.get(element).copied().flatten();
        for _ in 0..MAX_STRUCTURE_DEPTH {
            let Some(index) = current else {
                break;
            };
            if !ancestors.insert(index) {
                break;
            }
            current = self.parent.get(index).copied().flatten();
        }
        ancestors
    }

    /// 记录一个 MC 子项认领的本页区段。
    fn record_section(
        &mut self,
        element: usize,
        kid: usize,
        mcid: i64,
        item_page: Option<i64>,
        page: Option<i64>,
        sections_by_mcid: &BTreeMap<i64, usize>,
    ) {
        if !page_compatible(item_page, page) {
            return;
        }
        let Some(&section) = sections_by_mcid.get(&mcid) else {
            return;
        };
        self.element_sections
            .entry(element)
            .or_default()
            .push((kid, section));
        self.bearing.insert(element);
        // 同一区段被多个元素认领（畸形写法）：取结构序中第一次。
        self.section_claims.entry(section).or_insert((element, kid));
    }
}

/// 生效页与目标页是否相容：任一侧未声明时按相容处理（见模块文档）。
fn page_compatible(item_page: Option<i64>, page: Option<i64>) -> bool {
    match (item_page, page) {
        (Some(item), Some(target)) => item == target,
        (None, _) => true,
        (Some(_), None) => false,
    }
}

/// 元素生效的标准类型是否为显式关联档的 `Annot`/`Link`/`Form`
/// （14.8.4.4 与 14.8.4.5 Table 338/340，按 `/RoleMap` 解析）。
fn is_association_type(tree: &StructureTree, index: usize) -> bool {
    matches!(
        tree.standard_type(index),
        Some(StandardStructureType::Annot)
            | Some(StandardStructureType::Link)
            | Some(StandardStructureType::Form)
    )
}

/// 是否为规范 NOTE 说的空标记内容序列：`MP`/`DP` 点（14.6.2 定义为空序列）
/// 与定界算子之间不含任何其它算子的区段。
fn is_empty_sequence(section: &MarkedSection, ops: &[Op]) -> bool {
    if section.point {
        return true;
    }
    let start = section.start.min(ops.len());
    let end = section.end.clamp(start, ops.len());
    !ops[start..end].iter().any(|op| {
        !matches!(
            op,
            Op::MarkedContentPoint(_)
                | Op::MarkedContentPointWithProperties(..)
                | Op::BeginMarkedContent(_)
                | Op::BeginMarkedContentWithProperties(..)
                | Op::EndMarkedContent
        )
    })
}
