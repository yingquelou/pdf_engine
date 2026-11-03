//! Table F.4 项 1/2/4/5 的**跨页派生**（ISO 32000-1 F.4.1）。解析层只给出逐页
//! 增量（见 [`super::page_offset`]），而规范要求读者自己累加，本模块补的正是
//! 这一步：派生值一律**不落地存储**，按需从 [`PageOffsetHintTable`] 算出。
//!
//! 原文要点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 的输出行）：
//! - 项 1（35562—35571）：「The first object of the first page shall have an
//!   object number that is the value of the O entry in the linearization
//!   parameter dictionary」；「The first object of the second page shall have an
//!   object number of 1」；「Object numbers for subsequent pages shall be
//!   determined by accumulating the number of objects in all previous pages」。
//!   首页在**另一个对象组**（F.3.1，其对象号自 `/O` 起），故累加链自第二页起、
//!   不含首页对象数——否则第二页就取不到规定的 1。
//! - 项 2（35572—35578）：「The locations of subsequent pages shall be
//!   determined by accumulating the lengths of all previous pages」，且「A
//!   conforming product shall skip over the primary hint stream, wherever it is
//!   located」。跳过主 hint 流＝F.4 前言 NOTE 3（35488—35491）「a position
//!   greater than the hint stream offset shall have the hint stream length
//!   added to it」，故本模块只在 **hint 表空间**里累加（锚点取 Table F.3 项 2
//!   的首页页对象位置，35529），修正一律走既有
//!   [`super::LinearizationHints::adjust_position`] 这一条路径，**不另起第二条**。
//!   NOTE 3 末段（35498—35499）另规定该规则**不适用**于交叉引用表与参数字典里的
//!   偏移：项 2 也允许以 `/O` 对象的 xref 位置为锚（该位置已是文件绝对偏移），
//!   故 [`PageOffsetHintTable::page_start_byte`] 把锚点留给调用方而不猜。
//! - 项 5（35608—35628）＋分母（Table F.3 项 13，35558）：分子 `0..d-1` 选内容流
//!   的第 k 份，`== d` 表示该共享对象不由内容流引用、需在页末图像 XObject 与
//!   非共享对象**之前**就绪，`>= d+1` 表示在其**之后**。NOTE（35630—35634）
//!   「This method of dividing the page into fractions is only approximate …
//!   The relationship between positions in the unencoded and encoded streams is
//!   not necessarily linear」⇒ 本模块**刻意不把分数折算成字节偏移**（按字面折算
//!   反而违规），只给类型化类别 [`SharedObjectPosition`]。分母为 0 时类别无从
//!   判定，一律退回头 [`None`]：**本模块没有任何除法**。
//! - 项 4 与 F.4.2（35604）：「shall refer to an entry in the shared object hint
//!   table by a simple index that is its sequential position in the table,
//!   counting from 0」——标识符是共享对象 hint 表的**条目下标**，与对象号无关
//!   （项 4 原文「shared object identifiers shall not be directly related to
//!   object numbers」，35588—35589）。
//!
//! 累加一律 `saturating_add`：不可信的位流与头部值绝不 panic、绝不回绕。跨页
//! 核对——项 1 的对象号相撞、项 13 分母为 0、项 4 的下标越界——同型地只落
//! [`LinearizationIssue`] 类型化诊断，绝不硬失败；三者在装配期一并汇入
//! [`LinearizationHints::issues`]（项 4 走 [`check_shared_object_indices`]），
//! 后者另有 [`LinearizationHints::shared_object_index_issues`] 视图可单独取用。

use crate::core::reader::linearization::issues::LinearizationIssue;

use super::LinearizationHints;
use super::page_offset::PageOffsetHintTable;
use super::shared::SharedObjectHintTable;

/// Table F.4 项 5 ＋ Table F.3 项 13：共享对象引用的**分数位置类别**。
///
/// 这不是字节偏移，也**不得**按 `numerator × 页长 / denominator` 折算：项 5 的
/// NOTE 明说分数只近似到内容流的「份」，且编码前后流的位置关系未必线性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedObjectPosition {
    /// 分子 `0..d-1`：首次引用落在页内容流的第 `numerator` 份（共 `denominator` 份）。
    ContentStreamFraction { numerator: u64, denominator: u64 },
    /// 分子 `== d`：不由内容流引用（注解等），且需在页末图像 XObject 与
    /// 非共享对象**之前**就绪。
    BeforeTrailingObjects { numerator: u64, denominator: u64 },
    /// 分子 `>= d+1`：不由内容流引用，且在页末那些对象**之后**才需要；
    /// 原值（自 `d+1` 起）保留在 `numerator`，规范未给超出 `d+1` 的进一步语义。
    AfterTrailingObjects { numerator: u64, denominator: u64 },
}

impl PageOffsetHintTable {
    /// Table F.4 项 1 – cumulative first object number of the page at `page_index`
    /// (0-based)：首页取参数字典 `/O`（`first_page_object`，缺失即 `None`），
    /// 第二页恒 1，其后各页累加**第二页起**各页的对象数。
    pub fn page_first_object_number(
        &self,
        first_page_object: Option<u64>,
        page_index: usize,
    ) -> Option<u64> {
        self.pages.get(page_index)?;
        if page_index == 0 {
            return first_page_object;
        }
        Some(
            self.pages[1..page_index]
                .iter()
                .fold(1u64, |acc, previous| {
                    acc.saturating_add(previous.object_count(&self.header))
                }),
        )
    }

    /// Table F.4 项 2 – 页起点在 **hint 表空间**的字节位置（锚点由调用方给出：
    /// Table F.3 项 2 的首页页对象位置，或 `/O` 对象的 xref 位置），其后各页累加
    /// 页长；页区间是 `[start, start + page_length)`。
    pub fn page_start_byte(&self, first_page_anchor: u64, page_index: usize) -> Option<u64> {
        self.pages.get(page_index)?;
        Some(
            self.pages[..page_index]
                .iter()
                .fold(first_page_anchor, |acc, previous| {
                    acc.saturating_add(previous.page_length(&self.header))
                }),
        )
    }

    /// Table F.4 项 2 的页区间上界：`page_start_byte` 加本页页长（saturating）。
    pub fn page_end_byte(&self, first_page_anchor: u64, page_index: usize) -> Option<u64> {
        let start = self.page_start_byte(first_page_anchor, page_index)?;
        let length = self.pages[page_index].page_length(&self.header);
        Some(start.saturating_add(length))
    }

    /// Table F.4 项 2 的页区间 `[start, start + page_length)`（hint 表空间，
    /// 不做主 hint 流修正）；`page_index` 越界即 `None`。
    pub fn page_span(&self, first_page_anchor: u64, page_index: usize) -> Option<(u64, u64)> {
        let start = self.page_start_byte(first_page_anchor, page_index)?;
        Some((start, self.pages[page_index].page_length(&self.header)))
    }

    /// Table F.4 项 5 ＋ Table F.3 项 13：第 `reference_index` 个共享对象引用的
    /// 分数位置类别。分母 0（无从分类）、页/序号越界一律 `None`；**不做除法**。
    pub fn shared_object_position(
        &self,
        page_index: usize,
        reference_index: usize,
    ) -> Option<SharedObjectPosition> {
        let numerator = *self
            .pages
            .get(page_index)?
            .shared_position_numerators
            .get(reference_index)?;
        let denominator = self.header.shared_position_denominator;
        if denominator == 0 {
            return None;
        }
        let position = if numerator < denominator {
            SharedObjectPosition::ContentStreamFraction {
                numerator,
                denominator,
            }
        } else if numerator == denominator {
            SharedObjectPosition::BeforeTrailingObjects {
                numerator,
                denominator,
            }
        } else {
            SharedObjectPosition::AfterTrailingObjects {
                numerator,
                denominator,
            }
        };
        Some(position)
    }

    /// Table F.4 项 1 的累加链上**第二对象组用到的最大对象号**（第二页起各页
    /// 对象数之和，起点为 1）；只有首页时为 `None`。
    pub fn second_group_max_object(&self) -> Option<u64> {
        if self.pages.len() <= 1 {
            return None;
        }
        Some(self.pages[1..].iter().fold(0u64, |acc, page| {
            acc.saturating_add(page.object_count(&self.header))
        }))
    }

    /// Table F.4 项 3 的共享对象引用总数（自第二页起；首页那一项按 shall 是 0）。
    pub fn shared_reference_total(&self) -> u64 {
        self.pages.iter().skip(1).fold(0u64, |acc, page| {
            acc.saturating_add(page.shared_reference_count)
        })
    }

    /// Table F.4 项 4 的下标越界（F.4.2 35604：下标是共享对象 hint 表的顺序位置，
    /// 自 0 起）：每页至多报一条，携带该页越界标识符的**最大值**。
    pub fn shared_index_problems(&self, shared_entries: u64) -> Vec<(u64, u64)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(index, page)| {
                let worst = page
                    .shared_object_ids
                    .iter()
                    .filter(|id| **id >= shared_entries)
                    .max()?;
                Some((index as u64, *worst))
            })
            .collect()
    }
}

impl LinearizationHints {
    /// Table F.4 item 1 – cumulative first object number of page `page_index`.
    pub fn page_first_object_number(&self, page_index: usize) -> Option<u64> {
        self.page_offset
            .as_ref()?
            .page_first_object_number(self.first_page_object, page_index)
    }

    /// Table F.4 item 2 – file offset of page `page_index`'s page object (anchor:
    /// Table F.3 item 2; shifted past the primary hint stream by
    /// [`LinearizationHints::adjust_position`]).
    pub fn page_start_byte(&self, page_index: usize) -> Option<u64> {
        let table = self.page_offset.as_ref()?;
        let position =
            table.page_start_byte(table.header.first_page_object_location, page_index)?;
        Some(self.adjust_position(position))
    }

    /// Table F.4 item 2 – one past the last byte of page `page_index` (the span is
    /// `[page_start_byte, page_end_byte)`).
    pub fn page_end_byte(&self, page_index: usize) -> Option<u64> {
        let table = self.page_offset.as_ref()?;
        let end = table.page_end_byte(table.header.first_page_object_location, page_index)?;
        Some(self.adjust_position(end))
    }

    /// Table F.4 item 5 – position class of the `reference_index`-th shared
    /// reference of page `page_index`; **not** a byte offset (see module header).
    pub fn shared_object_position(
        &self,
        page_index: usize,
        reference_index: usize,
    ) -> Option<SharedObjectPosition> {
        self.page_offset
            .as_ref()?
            .shared_object_position(page_index, reference_index)
    }

    /// Table F.4 项 4 ＋ F.4.2 – 标识符不是共享对象 hint 表顺序位置时的类型化
    /// 诊断（不硬失败）。装配期已由 [`check_shared_object_indices`] 把同一批
    /// 诊断并入 [`LinearizationHints::issues`]；本方法是**只列这一类诊断**的
    /// 独立视图，与装配结果同源（同一实现）。
    pub fn shared_object_index_issues(&self) -> Vec<LinearizationIssue> {
        let mut issues = Vec::new();
        check_shared_object_indices(
            self.page_offset.as_ref(),
            self.shared_object.as_ref(),
            &mut issues,
        );
        issues
    }
}

/// Table F.4 项 4 ＋ F.4.2（35604）——装配期的共享对象下标越界核对：标识符
/// 须是共享对象 hint 表的顺序下标（自 0 起），越过条目数即逐页落一条
/// [`LinearizationIssue::PageOffsetSharedObjectIndexOutOfRange`]（携带该页
/// 越界标识符的最大值），不硬失败。
///
/// 由装配层在两张表都就绪后调用，结果并入 [`LinearizationHints::issues`]；
/// [`LinearizationHints::shared_object_index_issues`] 与本函数同源。任一张表
/// 缺席时不核对（不叠加假阳性）。
pub(super) fn check_shared_object_indices(
    page_offset: Option<&PageOffsetHintTable>,
    shared_object: Option<&SharedObjectHintTable>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let (Some(table), Some(shared)) = (page_offset, shared_object) else {
        return;
    };
    let entries = shared.first_page_groups.len() as u64 + shared.shared_section_groups.len() as u64;
    issues.extend(
        table
            .shared_index_problems(entries)
            .into_iter()
            .map(
                |(page, index)| LinearizationIssue::PageOffsetSharedObjectIndexOutOfRange {
                    page,
                    index,
                    entries,
                },
            ),
    );
}

/// 项 1/5 的跨页核对：只落类型化诊断，派生值本身仍照常给出（原值一律保留）。
///
/// 两条都不硬失败：读者仍可拿派生结果尽力定位，规范层面对照 [`super::LinearizationHints`]
/// 的其他页偏移诊断一样处理。项 4 的下标越界是同类核对，由
/// [`check_shared_object_indices`] 在装配期单独执行（见其文档）。
pub(super) fn check_page_offset_derivations(
    table: Option<&PageOffsetHintTable>,
    first_page_object: Option<u64>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(table) = table else { return };
    // 项 1 的 shall：第二页首对象号恒为 1，故首页的 `/O` 必须落在第二对象组之后
    // （F.3.1 的「第一组紧接第二组末尾」在表内派生值上的体现）。只报**确凿相撞**
    // （`/O` 不大于第二组最大号），偏大不报——hint 表只按页计对象数，组内还含
    // 目录等非页对象。
    if let (Some(first_page_object), Some(max)) =
        (first_page_object, table.second_group_max_object())
        && first_page_object <= max
    {
        issues.push(LinearizationIssue::PageOffsetObjectNumberConflict {
            first_page_object,
            second_group_max: max,
        });
    }
    // 项 5 与 Table F.3 项 13：分母为 0 时分数类别无从判定（不做除法，见模块头）。
    if table.header.shared_position_denominator == 0 {
        let references = table.shared_reference_total();
        if references > 0 {
            issues.push(LinearizationIssue::PageOffsetSharedFractionDenominatorZero { references });
        }
    }
}
