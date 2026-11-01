//! 注解在页面内容序列中的位置（ISO 32000-1 14.8.2.3.2「Sequencing of
//! Annotations」）。
//!
//! 规范原文（14.8.2.3.2，逐句对应本模块的建模）：
//!
//! > Annotations associated with a page are not interleaved within the page's
//! > content stream but shall be placed in the Annots array in its page object
//! > (see 7.7.3.3, "Page Objects"). Consequently, the correct position of an
//! > annotation in the page content order is not readily apparent but shall be
//! > determined from the document's logical structure.
//! > Both page content (marked-content sequences) and annotations may be
//! > treated as content items that are referenced from structure elements (see
//! > 14.7.4, "Structure Content"). Structure elements of type Annot (PDF 1.5),
//! > Link, or Form (see 14.8.4.4, "Inline-Level Structure Elements," and
//! > 14.8.4.5, "Illustration Elements") explicitly specify the association
//! > between a marked-content sequence and a corresponding annotation. In
//! > other cases, if the structure element corresponding to an annotation
//! > immediately precedes or follows (in the logical structure order) a
//! > structure element corresponding to a marked-content sequence, the
//! > annotation is considered to precede or follow the marked-content
//! > sequence, respectively, in the page content order.
//! > NOTE If necessary, a conforming writer may introduce an empty
//! > marked-content sequence solely to serve as a structure element for the
//! > purpose of positioning adjacent annotations in the page content order.
//!
//! 由此得到的 shall / 实现选择划分：
//!
//! - **shall**：注解只在页面 `/Annots` 数组里、不在内容流里（故其页序必须由
//!   逻辑结构导出）；标注内容序列与注解都可作为 14.7.4 的内容项被结构元素
//!   引用；`Annot`（PDF 1.5，14.8.4.4.3 与 Table 338 均已核实为 1.5 引入）、
//!   `Link`、`Form` 三种结构类型**显式**给出「一个标记内容序列 ↔ 一个注解」
//!   的关联；邻接规则作为其余情形的 fallback。
//! - **实现选择**（规范未规定，本模块取定并保证确定性）：显式关联时注解落在
//!   关联序列的哪一侧，按该元素 `/K` 里 OBJR 与 MCID/MCR 子项的**先后位置**
//!   决定；元素自身没有 MC 子项时，取其子树中第一个 MC 内容项之前；邻接规则
//!   同时成立时优先「注解元素在前的邻接元素」；同一侧挂多个注解时按本页
//!   `/Annots` 下标升序；无法判定的注解统一追加在序列末尾。
//!
//! 输出 [`ContentOrderItem`] 序列即该页的**页面内容序列**（14.8.2.3.1：页内
//! 图形对象的先后由内容流的排列决定，故标记内容区段按
//! [`Document::page_marked_content`] 的流序给出，含 `Artifact` 等未挂结构的
//! 区段；只有注解的位置取自逻辑结构）。
//!
//! 判定强度由 [`AnnotationOrderBasis`] 表达：
//! [`AnnotationOrderBasis::ExplicitAssociation`]（Annot/Link/Form 元素，规范
//! 的强关联）、[`AnnotationOrderBasis::AdjacentPrecedes`] 与
//! [`AnnotationOrderBasis::AdjacentFollows`]（14.8.2.3.2 的邻接句）、
//! [`AnnotationOrderBasis::Undetermined`]（既无关联也无邻接，或本文档没有可
//! 导出的结构；此时注解仍按 `/Annots` 下标出现在序列末尾，仅表示输出顺序
//! 约定，不表示规范给出的位置）。
//!
//! 生效页的解析沿用 [`artifact_attribution`](super::artifact_attribution) 的
//! 约定并做一处补强：MCR 的 `/Pg` 覆盖元素的 `/Pg`，整数 MCID 子项沿用元素
//! 的 `/Pg`，OBJR 的 `/Pg` 覆盖元素的 `/Pg`；元素自身没有 `/Pg` 时沿父链取
//! 最近一次声明（Table 323 的 `/Pg` 是可选条目，规范未规定缺省语义，本模块
//! 按「就近祖先声明」实现）。整棵元素链都没有 `/Pg` 时视为未指明：MC 子项按
//! 本页内容流里的同 MCID 区段认领，OBJR 按本页 `/Annots` 的成员关系认领，
//! 因此只写了 `/Annots` 与 OBJR、不写 `/Pg` 的文档同样可导出顺序。MCR 带
//! `/Stm` 或 `/StmOwn` 时序列不在页面流中，不参与本页排序（与
//! `ArtifactInLogicalStructure` 同一处理）。
//!
//! 规范 NOTE 的空标记内容序列（写端只为定位相邻注解而放的一段空序列）由
//! [`MarkedContentPosition::positioning_anchor`] 识别：`MP`/`DP` 点（14.6.2
//! 定义其本身就是空标记内容序列）与 `BMC`/`BDC … EMC` 之间不含任何算子的
//! 区段；这类区段照常参与排序，不会因为「没有内容」而被丢弃或报错。
//!
//! 版本边界：`Annot` 结构与 `Link`/`Form` 的显式关联机制建立在 14.7 逻辑结构
//! 之上，其中 `Annot` 结构类型自 PDF 1.5 引入。该边界已由特性扫描侧登记
//! （`/S` 或 `/RoleMap` 目标命中 1.5 扩展名即
//! [`PdfFeature::StructureTypeExtensions`](crate::core::types::PdfFeature)），
//! 本增量不新增读端登记：本模块只从已有对象派生顺序，不引入任何新的磁盘标记。
//!
//! 容错原则（原则 6）：读取输入失败（页码越界、内容流损坏、结构树损坏、
//! `/Annots` 损坏）返回类型化 [`AnnotationOrderingError`]；文档**没有**结构树
//! 不是错误，返回空序列。结构树里的环、越界引用、重复认领同一注解等畸形写法
//! 一律降级为「少一个可判定项」或 [`AnnotationOrderBasis::Undetermined`]，
//! 不 panic。
//!
//! 遍历与落点求解在子模块 [`walk`](walk)（约定 5：单文件不越 600 行）。

mod walk;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::span::mcid_value;
use crate::core::reader::annotation::{Annotation, AnnotationError};
use crate::core::reader::content::{MarkedContentError, MarkedSection, Op};
use crate::core::reader::page_content::PageContentError;
use crate::core::reader::structure::StructureError;
use crate::core::types::Document;
use walk::{Placement, StructureWalk};

/// 注解位置的依据（14.8.2.3.2 的三句话对应前三个变体，最后一档是规范承认
/// 「无从判定」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationOrderBasis {
    /// 声称该注解的结构元素是 `Annot`（PDF 1.5）/`Link`/`Form`，且经
    /// `/RoleMap` 解析后仍为这三个标准类型之一：规范明示的强关联。
    ExplicitAssociation,
    /// 注解元素在逻辑结构序中**紧接在**某个对应标记内容序列的元素之前，
    /// 故注解在页面内容序列中位于该序列之前。
    AdjacentPrecedes,
    /// 注解元素在逻辑结构序中**紧接在**某个对应标记内容序列的元素之后，
    /// 故注解在页面内容序列中位于该序列之后。
    AdjacentFollows,
    /// 既没有显式关联也没有可用邻接（含注解未被任何结构元素声称），
    /// 位置不可由逻辑结构导出。
    Undetermined,
}

impl AnnotationOrderBasis {
    /// 该依据是否给出了规范承认的位置（即非 `Undetermined`）。
    pub fn is_deterministic(self) -> bool {
        !matches!(self, Self::Undetermined)
    }

    /// 该依据是否来自 14.8.2.3.2 的显式关联句（Annot/Link/Form 元素）。
    pub fn is_explicit(self) -> bool {
        matches!(self, Self::ExplicitAssociation)
    }
}

impl fmt::Display for AnnotationOrderBasis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::ExplicitAssociation => "explicit Annot/Link/Form association",
            Self::AdjacentPrecedes => "structure adjacency (annotation precedes)",
            Self::AdjacentFollows => "structure adjacency (annotation follows)",
            Self::Undetermined => "position undeterminable from logical structure",
        };
        f.write_str(label)
    }
}

/// 页面内容序列中的一个标记内容区段条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkedContentPosition {
    /// [`Document::page_marked_content`] 返回序列中的区段下标（即流序）。
    pub section: usize,
    /// 声称该区段的结构元素 arena 下标；未被结构声称时为 `None`。
    pub element: Option<usize>,
    /// 该元素 `/K` 中的子项位置；[`Self::element`] 为 `None` 时为 `None`。
    pub kid: Option<usize>,
    /// 规范 NOTE 的空标记内容序列：只用于定位相邻注解的定位锚点。
    pub positioning_anchor: bool,
}

/// 页面内容序列中的一个注解条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationPosition {
    /// [`Document::page_annotations`] 返回序列中的下标（即本页 `/Annots` 序）。
    pub annotation: usize,
    /// 声称该注解的结构元素 arena 下标；未被声称时为 `None`。
    pub element: Option<usize>,
    /// 该元素 `/K` 中的 OBJR 子项位置；[`Self::element`] 为 `None` 时为 `None`。
    pub kid: Option<usize>,
    /// 位置的依据。
    pub basis: AnnotationOrderBasis,
    /// 决定该位置的标记内容区段下标；[`Self::basis`] 为
    /// [`AnnotationOrderBasis::Undetermined`] 时为 `None`。
    pub associated_sequence: Option<usize>,
}

/// 页面内容序列的一个条目：标记内容区段或注解。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentOrderItem {
    /// 内容流里的标记内容区段（位置由流序直接给出，14.8.2.3.1）。
    MarkedContent(MarkedContentPosition),
    /// 只存在于页面 `/Annots` 的注解（位置由逻辑结构导出，14.8.2.3.2）。
    Annotation(AnnotationPosition),
}

impl ContentOrderItem {
    /// 该条目对应的标记内容区段下标；注解条目为 `None`。
    pub fn section(&self) -> Option<usize> {
        match self {
            Self::MarkedContent(position) => Some(position.section),
            Self::Annotation(_) => None,
        }
    }

    /// 该条目对应的本页 `/Annots` 下标；区段条目为 `None`。
    pub fn annotation(&self) -> Option<usize> {
        match self {
            Self::MarkedContent(_) => None,
            Self::Annotation(position) => Some(position.annotation),
        }
    }

    /// 该条目的位置依据；区段条目按其是否被结构声称返回。
    pub fn basis(&self) -> Option<AnnotationOrderBasis> {
        match self {
            Self::MarkedContent(_) => None,
            Self::Annotation(position) => Some(position.basis),
        }
    }
}

/// 读取本模块所需输入时的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationOrderingError {
    /// 页码越界或页面树损坏。
    Page(PageContentError),
    /// 页面标记内容无法配对或内容流无法解析。
    MarkedContent(MarkedContentError),
    /// 逻辑结构树损坏。
    Structure(StructureError),
    /// 页面 `/Annots` 或注解字典损坏，无法枚举本页注解。
    Annotations(AnnotationError),
}

impl fmt::Display for AnnotationOrderingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(error) => write!(f, "page content order needs its page: {error}"),
            Self::MarkedContent(error) => {
                write!(f, "page content order needs its marked content: {error}")
            }
            Self::Structure(error) => {
                write!(f, "page content order needs the logical structure: {error}")
            }
            Self::Annotations(error) => {
                write!(
                    f,
                    "page content order needs the page /Annots array: {error}"
                )
            }
        }
    }
}

impl std::error::Error for AnnotationOrderingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Page(error) => Some(error),
            Self::MarkedContent(error) => Some(error),
            Self::Structure(error) => Some(error),
            Self::Annotations(error) => Some(error),
        }
    }
}

impl From<PageContentError> for AnnotationOrderingError {
    fn from(error: PageContentError) -> Self {
        Self::Page(error)
    }
}

impl From<MarkedContentError> for AnnotationOrderingError {
    fn from(error: MarkedContentError) -> Self {
        Self::MarkedContent(error)
    }
}

impl From<StructureError> for AnnotationOrderingError {
    fn from(error: StructureError) -> Self {
        Self::Structure(error)
    }
}

impl From<AnnotationError> for AnnotationOrderingError {
    fn from(error: AnnotationError) -> Self {
        Self::Annotations(error)
    }
}

/// 落点到序列位置用 `(区段下标, 槽位, 次序)` 三元组排序：槽位 0 表示区段之前、
/// 1 表示区段本身、2 表示区段之后、3 表示末尾（不可导出档）；次序取本页
/// `/Annots` 下标，保证同一槽位挂多个注解时输出确定。
const SLOT_BEFORE: u8 = 0;
const SLOT_SECTION: u8 = 1;
const SLOT_AFTER: u8 = 2;
const SLOT_UNDETERMINED: u8 = 3;

impl Document {
    /// 按 14.8.2.3.2 从文档的逻辑结构导出注解在页面内容序列中的位置。
    ///
    /// 返回序列即该页的页面内容序列：标记内容区段按内容流序（
    /// [`Document::page_marked_content`]）排列，注解按逻辑结构插在其中，
    /// 位置依据见 [`AnnotationOrderBasis`]。文档没有 `/StructTreeRoot` 时返回
    /// 空序列（不报错）：没有逻辑结构就没有可导出的位置。页码越界、内容流
    /// 损坏、结构树损坏或 `/Annots` 损坏分别返回
    /// [`AnnotationOrderingError`] 的对应变体。
    ///
    /// 输出顺序确定：区段按流序；同一侧挂多个注解时按本页 `/Annots` 下标
    /// 升序；`Undetermined` 的注解统一追加在末尾（同样按 `/Annots` 升序）。
    pub fn annotation_content_order(
        &self,
        page_index: usize,
    ) -> Result<Vec<ContentOrderItem>, AnnotationOrderingError> {
        let page_number = self
            .locate_page_node_with_ref(page_index)?
            .1
            .map(|(number, _)| number);
        // 没有结构树＝没有可导出的位置，返回空序列而不是错误。
        let Some(tree) = self.structure_tree()? else {
            return Ok(Vec::new());
        };
        let sections = self.page_marked_content(page_index)?;
        let ops = self.page_content_ops(page_index)?;
        let annotations = self.page_annotations(page_index)?;
        let walk = StructureWalk::collect(
            &tree,
            page_number,
            &sections_by_mcid(&sections),
            &annotations_by_object(&annotations),
        );
        Ok(assemble(&walk, &sections, &ops, annotations.len()))
    }
}

/// MCID → 本页区段下标（同一 MCID 命中多个区段时取流中最小的下标）。
fn sections_by_mcid(sections: &[MarkedSection]) -> BTreeMap<i64, usize> {
    let mut index_by_mcid: BTreeMap<i64, usize> = BTreeMap::new();
    for (index, section) in sections.iter().enumerate() {
        if let Some(properties) = section.properties.as_ref()
            && let Some(mcid) = mcid_value(properties)
        {
            index_by_mcid.entry(mcid).or_insert(index);
        }
    }
    index_by_mcid
}

/// 注解对象号 → 本页 `/Annots` 下标（重复对象号取最小下标）；直接嵌入
/// `/Annots` 的非间接注解不可能被 Table 325 要求的间接 `/Obj` 声称。
fn annotations_by_object(annotations: &[Annotation]) -> BTreeMap<i64, usize> {
    let mut index_by_object: BTreeMap<i64, usize> = BTreeMap::new();
    for (index, annotation) in annotations.iter().enumerate() {
        if let Some((number, _)) = annotation.reference {
            index_by_object.entry(number).or_insert(index);
        }
    }
    index_by_object
}

/// 把区段骨架与注解落点合成确定顺序的序列。
fn assemble(
    walk: &StructureWalk,
    sections: &[MarkedSection],
    ops: &[Op],
    annotation_count: usize,
) -> Vec<ContentOrderItem> {
    let tail = sections.len() as i64;
    let mut slots: Vec<(i64, u8, u64, ContentOrderItem)> = Vec::new();
    for entry in walk.sections(sections, ops) {
        slots.push((
            entry.section as i64,
            SLOT_SECTION,
            0,
            ContentOrderItem::MarkedContent(MarkedContentPosition {
                section: entry.section,
                element: entry.element,
                kid: entry.kid,
                positioning_anchor: entry.anchor,
            }),
        ));
    }
    let mut claimed: BTreeSet<usize> = BTreeSet::new();
    for claim in &walk.claims {
        claimed.insert(claim.annotation);
        let placement = walk.place(*claim);
        let (basis, associated_sequence, slot) = match placement {
            Placement::Before { section, explicit } => (
                if explicit {
                    AnnotationOrderBasis::ExplicitAssociation
                } else {
                    AnnotationOrderBasis::AdjacentPrecedes
                },
                Some(section),
                (section as i64, SLOT_BEFORE),
            ),
            Placement::After { section, explicit } => (
                if explicit {
                    AnnotationOrderBasis::ExplicitAssociation
                } else {
                    AnnotationOrderBasis::AdjacentFollows
                },
                Some(section),
                (section as i64, SLOT_AFTER),
            ),
            // 位置不可导出：追加在末尾只是本输出的确定性约定，不是规范位置。
            Placement::Undetermined => (
                AnnotationOrderBasis::Undetermined,
                None,
                (tail, SLOT_UNDETERMINED),
            ),
        };
        slots.push((
            slot.0,
            slot.1,
            claim.annotation as u64,
            ContentOrderItem::Annotation(AnnotationPosition {
                annotation: claim.annotation,
                element: Some(claim.element),
                kid: Some(claim.kid),
                basis,
                associated_sequence,
            }),
        ));
    }
    // 未被任何 OBJR 声称的注解：只有 `Undetermined` 一档，同样落在末尾。
    for index in 0..annotation_count {
        if !claimed.insert(index) {
            continue;
        }
        slots.push((
            tail,
            SLOT_UNDETERMINED,
            index as u64,
            ContentOrderItem::Annotation(AnnotationPosition {
                annotation: index,
                element: None,
                kid: None,
                basis: AnnotationOrderBasis::Undetermined,
                associated_sequence: None,
            }),
        ));
    }
    slots.sort_by_key(|item| (item.0, item.1, item.2));
    slots.into_iter().map(|item| item.3).collect()
}
