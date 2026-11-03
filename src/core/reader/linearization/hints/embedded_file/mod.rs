//! 内嵌文件流 hint 表（ISO 32000-1 F.4.6，Table F.11 头部 + Table F.12 每组
//! 条目）——**只读侧**。本目录只做类型化模型与派生访问器；位流读取（含 0—32
//! 位宽门与分配前的位预算封顶）与装配层接线在 [`parse`]。
//!
//! 键名自证（Table F.2 续栏 35381—35382，行号＝`pdftotext -raw
//! docs/PDF32000_2008.pdf` 的输出行）：「B (Present only if embedded file
//! streams exist; PDF 1.5) Embedded file stream hint table (see F.4.6,
//! "Embedded File Stream Hint Tables")」——本库先前的盘点记为 `B`，此处按原文
//! 确证一致（`EMBEDDED_FILE_HINT_KEY` 与 `STANDARD_HINT_KEYS` 里 `B` 的描述由
//! 单元测试钉住）。`B` 与 F.4.4 的 `O`/`A`/`E`/`I`/`L`、F.4.5 的 `V`/`C`/`R`
//! **互不相交**，且 F.4.5 另有一条 shall 把内嵌文件流划出 extended 形态
//! （35786—35788，见 [`super::extended`] 模块头）。`B` 是**条件存在**：缺键即
//! 文档没有内嵌文件流，属合法形态，**不给诊断**（与 `T` 及五键/三键同口径，与
//! Required 的 `S` 不同）。
//!
//! 原文要点（行号同上）：
//! - 用途：「The embedded file streams hint table allows a conforming reader to
//!   locate all byte ranges of a PDF file needed to access its embedded file
//!   streams」（35790—35791）。
//! - 组约定：「An embedded file stream may be grouped with other objects that it
//!   references; all objects in such a group shall have adjacent object numbers.
//!   (A group shall contain no objects at all if it contains shared object
//!   references.)」（35791—35794）——括号那句正是 Table F.12 项 2/项 3 那两条
//!   shall 的出处，见 [`EmbeddedFileGroupEntry::zero_item_problems`]；相邻那句
//!   只可对**第一组**判定，见 [`EmbeddedFileHintTable::first_group_anchor_problem`]
//!   与下面的**剩余缺口**。
//! - 结构：「This hint table shall have a header section (see Table F.11), which
//!   shall have general information about the embedded file stream groups. The
//!   header section shall be followed by the entries in Table F.12. Each of the
//!   items in Table F.12 shall be repeated for each embedded file stream group
//!   (the number of groups being represented by item 3 in Table F.11)」
//!   （35794—35797）。
//! - **条目序（本家族最易写错处）**：「the order of items in Table F.12 shall be
//!   item 1 for the first group, item 1 for the second group, and so on; item 2
//!   for the first group, item 2 for the second group, and so on; repeated for
//!   the 5 items」（35797—35799）——是**项主序（转置）**排列：先把**所有组**的
//!   项 1 读完，再读所有组的项 2……与 F.4.1/F.4.3 的交错列同形，**不是**逐组
//!   连续。[`parse`] 里的读取因此按列进行，再用同样的组项 4 计数把项 5 的扁平
//!   列表归位。
//! - Table F.11 头部 7 项，定宽 32/32/32/16/16/16/16 = 160 位（35822—35832）。
//! - Table F.12 每组 5 项，宽度依次取头部项 4/5/6/7，项 5 取**文档级**的
//!   Table F.3 项 11（35835—35850）。项 2 与项 3 都是**原值**而非增量（与
//!   Table F.6/F.8 不同：本表头没有「最小值」项，故没有基数可加）。
//! - 头部项 2 仍是文件偏移，按 F.4 前言 NOTE 3 修正
//!   （[`super::LinearizationHints::adjust_position`]）。
//!
//! **项 5 的宽度只有一个来源**：Table F.12 项 5 的 size 列字面写「See Table
//! F.3, item 11」（35847），即页偏移 hint 表头部的文档级
//! [`super::PageOffsetHeader::shared_object_id_bits`]（35546—35548）——本表
//! **没有** F.4.5 那种组内宽度可作第二种读法，故 [`super::extended`] 模块头那条
//! 组内／文档级**待核**歧义在本表不存在，也不做「猜宽度」的退路。由此得到一条
//! 硬依赖：文档级宽度不可得（页偏移表缺失或读坏）时项 5 的列无从解包，本表
//! **不类型化**，只落
//! [`LinearizationIssue::EmbeddedFileIdWidthUnavailable`](super::super::issues::LinearizationIssue::EmbeddedFileIdWidthUnavailable)，
//! 绝不拿 0 宽冒充（那会把所有标识符伪造成 0）。声明宽度仍走 0—32 门：F.4.1
//! 那句「All the items in Table F.3 that specify a number of bits needed … have
//! values in the range 0 through 32」（35524—35525）**字面就约束 Table F.3**，
//! 故越界直接由
//! [`HintBitError::InvalidWidth`](super::bits::HintBitError::InvalidWidth)
//! 类型化拒绝——保留原值毫无意义，项 5 的列长与列宽都由它决定（取舍同
//! [`super::extended`] 对项 6）。
//!
//! **不可信声明值**：头部项 3（组数，32 位）与项 4—7（位宽，16 位）、每组项 4
//! （共享对象引用数，宽度受项 7 约束）一律按不可信处理：位宽越出 0—32 即类型化
//! 拒绝（复用 [`super::generic`] 模块头预告的那道门；本表前三项定宽 32、后四项
//! 定宽 16、Table F.12 各列宽度**互不相同**，故只复用该门与
//! [`super::page_offset`] 的 `width`/`column` 原语，不复用
//! [`super::GenericHintTable::parse_with_item_width`] 的单项宽读取循环）；
//! `count × width` 一律**先按剩余位预算封顶、再进入任何分配**，绝不
//! `Vec::with_capacity(原始计数)`；位宽全为 0 时位预算给不出条数证据，改用
//! [`super::page_offset`] 的绝对上限（库策略，口径同页偏移/缩略图/extended 三家）。
//!
//! **剩余缺口（诚实标注，均非本增量能判定）**：
//! - **写端不存在**：本库不产出线性化布局、也不写 hint 表，故本家族只有读端；
//!   集成用例一律由 `tests/main/linearization_hints/support.rs` 手工拼装。
//! - **组内对象号相邻性只可核对第一组**（35792 那句 shall）：Table F.12 只给每
//!   个条目关联的**内嵌文件流对象号**（项 1）、组内对象数（项 2）与组长度
//!   （项 3），**不给**每组的第一个对象号/位置——只有头部项 1—2 描述第一组。故
//!   [`EmbeddedFileHintTable::first_group_anchor_problem`] 只做「第一组的号区间
//!   须覆盖本组内嵌文件流对象号」的**派生**核对（由 35792 的相邻约定与头部项 1
//!   的字面定义**推出**，**不是**原文写成判定式的 shall，口径同
//!   [`super::ExtendedHintTable::shared_id_width_problem`]），其余组的相邻性
//!   无从判定（需要 xref 与对象号全表），留给调用方。
//! - 头部项 1—2 的首组锚点不做**字节范围组装**：本表用途是「定位访问内嵌文件流
//!   所需的全部字节范围」，但 F.4.6 未规定其余组的定位链，故只原样保留声明值。
//! - 项 5 的标识符只与 F.4.2 做**下标越界**核对
//!   （[`super::SharedObjectHintTable`] 的条目数），不做「该共享对象是否真被
//!   内嵌文件流引用」之类语义核对。
//! - **版本门**：Table F.2 给 `B` 标了 PDF 1.5，但原文**没有**「1.5 之前 `B` 须
//!   按别的形态读」的条款（对比 35781 为 F.4.5 另写了一条名册 shall，故
//!   [`super::extended`] 才有门可设）。本库对 `B` **不设门**：声明版本 < 1.5 而
//!   带 `B` 的文档同样按 Table F.11/F.12 读（没有第二种可读形态，丢弃反而不如
//!   实），口径同不设门的 `S`/`T`。

mod parse;

/// 键 `B` 的分派入口对装配层（`hints::assemble`）开放：`parse` 模块本身不外露。
pub(super) use parse::parse_embedded_file;

use crate::core::reader::linearization::issues::LinearizationIssue;

/// Table F.2 键 `B`（名册 35381—35382：「Present only if embedded file streams
/// exist; PDF 1.5」）——**条件存在**，缺键不报错、不给诊断。
pub(super) const EMBEDDED_FILE_HINT_KEY: &str = "B";

/// Table F.11 项 1—3 的定宽（位；35822—35824 的「32」列）。
const HEADER_FIXED_ITEM_BITS: u32 = 32;

/// Table F.11 项 4—7 的存储宽度（位；35825—35831 的「16」列）。
const HEADER_WIDTH_FIELD_BITS: u32 = 16;

/// Table F.12 项 2 为 0 时的 shall（35837—35840）原文口径，直接进诊断文本。
const ITEM2_ZERO_RULE: &str =
    "Table F.12 item 2 is 0, so item 3 shall be 0 and item 4 shall be greater than zero";

/// Table F.12 项 3 为 0 时的 shall（35841—35844）原文口径，直接进诊断文本。
const ITEM3_ZERO_RULE: &str =
    "Table F.12 item 3 is 0, so item 2 shall be 0 and item 4 shall be greater than zero";

/// Table F.11：内嵌文件流 hint 表头部（7 项，32/32/32/16/16/16/16 = 160 位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedFileHeader {
    /// 项 1（32 位，35822）：**第一个**内嵌文件流组的第一个对象的对象号。
    pub first_object_number: u64,
    /// 项 2（32 位，35823）：该对象的位置（文件偏移，须按 F.4 前言 NOTE 3 用
    /// [`super::LinearizationHints::adjust_position`] 修正）。
    pub first_object_location: u64,
    /// 项 3（32 位，35824）：本 hint 表引用的**组数**，即 Table F.12 每一项的
    /// 重复次数（35795—35797）。成功解析的表其条目数恒等于本值
    /// （[`EmbeddedFileHintTable::groups_match_header`]）；声明 0 组是「键 `B`
    /// 声称有表、表里却没有任何组」的形态，由装配层落
    /// [`LinearizationIssue::EmbeddedFileGroupCountZero`]。
    pub group_count: u64,
    /// 项 4（16 位，35825—35826）：表示「对应内嵌文件流对象的最高对象号」所需的
    /// 位数（＝Table F.12 项 1 的宽度）。
    pub object_number_bits: u32,
    /// 项 5（16 位，35827—35828）：表示「一个组内最大对象数」所需的位数
    /// （＝Table F.12 项 2 的宽度）。
    pub group_object_bits: u32,
    /// 项 6（16 位，35829—35830）：表示「一个组最大长度（字节）」所需的位数
    /// （＝Table F.12 项 3 的宽度）。
    pub group_length_bits: u32,
    /// 项 7（16 位，35831—35832）：表示「任一组的最大共享对象引用数」所需的位数
    /// （＝Table F.12 项 4 的宽度，因而也是项 5 列表条数的上界）。
    pub shared_ref_count_bits: u32,
}

impl EmbeddedFileHeader {
    /// 头部 7 项的定宽合计（位）：3×32 + 4×16 = 160（35822—35832）。
    pub(super) const HEADER_BITS: u64 =
        (HEADER_FIXED_ITEM_BITS * 3 + HEADER_WIDTH_FIELD_BITS * 4) as u64;

    /// 每个 Table F.12 条目在项 1—4 上至少消费的位数：四项声明宽度之和。项 5 另
    /// 按本组项 4 追加，故本值是**下界**，[`parse`] 用它挡在任何分配之前。
    pub(super) fn bits_per_group(&self) -> u64 {
        [
            self.object_number_bits,
            self.group_object_bits,
            self.group_length_bits,
            self.shared_ref_count_bits,
        ]
        .into_iter()
        .fold(0u64, |sum, bits| sum.saturating_add(u64::from(bits)))
    }
}

/// Table F.12：一个内嵌文件流组的条目（5 项；项 5 是位打包列表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFileGroupEntry {
    /// 项 1（宽度＝头部项 4，35835—35836）：本条目**关联的内嵌文件流对象**的
    /// 对象号。注意语义与 Table F.9/F.11 的项 1 不同——那不是「组内第一个对象」
    /// 而是「本条目对应的内嵌文件流对象」，故本表没有号区间可给（见模块头的
    /// **剩余缺口**）。
    pub object_number: u64,
    /// 项 2（宽度＝头部项 5，35837—35840）：本组对象数（**原值**，非增量）。
    /// 可为 0，表示本组只有共享对象引用。
    pub object_count: u64,
    /// 项 3（宽度＝头部项 6，35841—35844）：本组长度（字节，**原值**）。可为 0，
    /// 含义同上。
    pub group_length: u64,
    /// 项 4（宽度＝头部项 7，35845—35846）：本组引用的共享对象数，即项 5 列表的
    /// 声明长度（35848—35850 的 shall）。
    pub shared_reference_count: u64,
    /// 项 5（宽度＝**文档级** Table F.3 项 11，35847—35850）：位打包的共享对象
    /// 标识符列表，即 F.4.2 共享对象 hint 表的顺序下标（自 0 起）。列位越过流尾
    /// 时整表按 [`super::bits::HintBitError`] 拒绝，故成功解析的条目其长度恒等于
    /// [`Self::shared_reference_count`]——不留「静默半截列」。
    pub shared_object_ids: Vec<u64>,
}

impl EmbeddedFileGroupEntry {
    /// 项 2/项 3 为 0 时的两条 shall（35837—35844）逐条核对：返回被违反的规则
    /// 文本（两项同时为 0 且项 4 也为 0 时两条都违反，故返回列表）。
    ///
    /// 原文「This item may be 0, meaning that there are only shared object
    /// references. In this case, item 4 for this group shall be greater than
    /// zero and item 3 shall be zero」——即**项 2 为 0 与项 3 为 0 互为一对**：
    /// 一项为 0 时另一项须同时为 0 且项 4 须大于 0。原值一律保留在 [`Self`] 里，
    /// 本方法只做判定、不改写、不硬失败。
    pub fn zero_item_problems(&self) -> Vec<&'static str> {
        let mut problems = Vec::new();
        if self.object_count == 0 && (self.group_length != 0 || self.shared_reference_count == 0) {
            problems.push(ITEM2_ZERO_RULE);
        }
        if self.group_length == 0 && (self.object_count != 0 || self.shared_reference_count == 0) {
            problems.push(ITEM3_ZERO_RULE);
        }
        problems
    }
}

/// F.4.6 内嵌文件流 hint 表：Table F.11 头部 + Table F.12 的每组条目（项主序
/// 读取后按组归位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFileHintTable {
    /// Table F.11 头部（7 项原样保留）。
    pub header: EmbeddedFileHeader,
    /// Table F.12 的每组条目，按组序（第 1 组在前）；长度恒等于头部项 3
    /// （[`Self::groups_match_header`]）。
    pub groups: Vec<EmbeddedFileGroupEntry>,
    /// 项 5 解包用的**文档级**位宽，即 Table F.3 项 11 的声明值（35847 的
    /// 「See Table F.3, item 11」）。原值保留（不折算、不用组内值替换），便于
    /// 溯源到页偏移 hint 表。
    pub shared_object_id_bits: u32,
    /// 本表在主 hint 流位流里的起始字节偏移，即 Table F.2 键 `B` 的值。
    pub start_byte: u64,
    /// 本表消费掉的位数（自表首字节起）：头部恒
    /// [`EmbeddedFileHeader::HEADER_BITS`] 位，条目按各组声明宽度与项 5 的列表
    /// 长度追加。
    pub bits_consumed: u64,
}

impl EmbeddedFileHintTable {
    /// 本表末尾之后的第一个字节（流内绝对位置，起点是 Table F.2 键 `B` 的值）。
    /// F.4 前言要求各表以字节边界开始且互不重叠，故其他键的位置落进
    /// `[start_byte, 本值)` 即自相矛盾；本表长度**不固定**（条目数与项 5 的列长
    /// 都由不可信声明值决定），只能按 [`Self::bits_consumed`] 折算。
    pub fn end_byte(&self) -> u64 {
        self.start_byte
            .saturating_add(self.bits_consumed.div_ceil(8))
    }

    /// **派生**不变式：成功解析的表其条目数恒等于头部项 3（35795—35797 规定条目
    /// 按组数重复；读不满即整表按 [`super::bits::HintBitError`] 拒绝，不留半截
    /// 表）。由字面定义**推出**的自检，非独立条款。
    pub fn groups_match_header(&self) -> bool {
        self.groups.len() as u64 == self.header.group_count
    }

    /// 逐组的 [`EmbeddedFileGroupEntry::zero_item_problems`] 汇总，一条对一组
    /// （同一组可贡献两条），携带组序与条目本身供诊断填字段。
    pub fn zero_item_problems(&self) -> Vec<(u64, &'static str, &EmbeddedFileGroupEntry)> {
        self.groups
            .iter()
            .enumerate()
            .flat_map(|(index, entry)| {
                entry
                    .zero_item_problems()
                    .into_iter()
                    .map(move |rule| (index as u64, rule, entry))
            })
            .collect()
    }

    /// 第一组的**派生**相邻性核对（由 35792「all objects in such a group shall
    /// have adjacent object numbers」＋头部项 1「第一个组的第一个对象的对象号」
    /// **推出**，不是原文写成判定式的 shall；见模块头的**剩余缺口**）。
    ///
    /// 相邻 ⇒ 第一组的对象号是闭区间 `[头部项 1, 头部项 1 + 项 2 - 1]`，本组
    /// 条目的项 1（内嵌文件流对象号）须落在其中。返回 `Some((对象号, 区间起,
    /// 区间止))` 表示矛盾。项 2 为 0（按 35838 的合法形态，本组没有对象）时无从
    /// 判定，退回头 `None`。累加一律 saturating，绝不因不可信位流溢出 panic。
    pub fn first_group_anchor_problem(&self) -> Option<(u64, u64, u64)> {
        let first = self.groups.first()?;
        if first.object_count == 0 {
            return None;
        }
        let start = self.header.first_object_number;
        let end = start.saturating_add(first.object_count).saturating_sub(1);
        if (start..=end).contains(&first.object_number) {
            return None;
        }
        Some((first.object_number, start, end))
    }

    /// 项 5 的标识符越出 F.4.2 共享对象 hint 表条目数（35847—35850 的「indices
    /// into the shared object hint table」＋35604 的「sequential position in the
    /// table, counting from 0」）：每组至多报一条，携带该组越界标识符的**最大值**
    /// （口径同 [`super::page_offset_derive`] 的页偏移表同名方法）。
    pub fn shared_index_problems(&self, shared_entries: u64) -> Vec<(u64, u64)> {
        self.groups
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let worst = entry
                    .shared_object_ids
                    .iter()
                    .filter(|id| **id >= shared_entries)
                    .max()?;
                Some((index as u64, *worst))
            })
            .collect()
    }
}

/// 装配层的语义核对：全部只落类型化诊断，**不硬失败**（原值一律保留在
/// [`EmbeddedFileHintTable`] 里，读者仍可尽力定位）。挂在 [`parse`] 的
/// `parse_embedded_file` 之后、由 `hints/assemble.rs` 调用。
pub(super) fn check_embedded_file_semantics(
    table: Option<&EmbeddedFileHintTable>,
    shared_object_entries: Option<u64>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(table) = table else { return };
    // 项 3 声明 0 组：键 `B` 的存在条件就是「有内嵌文件流」，而 35795—35797 规定
    // 条目按组数重复，故「声称有表、没有任何组」是自相矛盾（**派生**结论：F.4.6
    // 没有 F.4.3 那句「shall include one or more per-page entries」）。
    if table.groups.is_empty() {
        issues.push(LinearizationIssue::EmbeddedFileGroupCountZero);
    }
    for (group, rule, entry) in table.zero_item_problems() {
        issues.push(LinearizationIssue::EmbeddedFileSharedOnlyGroupViolation {
            group,
            rule,
            object_count: entry.object_count,
            group_length: entry.group_length,
            shared_reference_count: entry.shared_reference_count,
        });
    }
    if let Some(entries) = shared_object_entries {
        for (group, index) in table.shared_index_problems(entries) {
            issues.push(
                LinearizationIssue::EmbeddedFileSharedObjectIndexOutOfRange {
                    group,
                    index,
                    entries,
                },
            );
        }
    }
    if let Some((object_number, run_start, run_end)) = table.first_group_anchor_problem() {
        issues.push(LinearizationIssue::EmbeddedFileFirstGroupAnchorMismatch {
            object_number,
            run_start,
            run_end,
        });
    }
}
