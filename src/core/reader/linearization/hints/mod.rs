//! 附录 F.4 hint 表读端解码（已完整解析**页偏移**、**共享对象**、**缩略图**、
//! **generic** 与 **extended generic** 五家）。
//!
//! 消费上层已识别的 [`LinearizationInfo`]（Table F.1 `/H` 给出的 hint 流
//! 范围由 [`HintStreams`]/[`HintRange`] 承载，本模块不重复定位，只做：
//! 读流 → 过滤解码 → 溢出拼接 → 按主 hint 流字典的 Table F.2 键取表位置
//! → 位流解析）。
//!
//! **已实现**：
//! - [`bits`]：F.4 前言的位流原语（任意宽度、高位在前；带符号形式与链式
//!   增量表按 PDF Reference 1.4 F.2.2/F.2.3 字面实现，ISO 32000-1 的 F.4
//!   各表未引用，属**待核**预留）；
//! - [`page_offset`]：F.4.1 页偏移 hint 表（Table F.3 头部 13 项 + Table F.4
//!   逐页条目 7 项，含 a)—g) 的项交错序）；该表没有 Table F.2 键，按前言
//!   「shall be the first table in the stream and shall start at offset 0」
//!   恒从流偏移 0 起读，页数取 Table F.1 `/N`；
//! - [`shared`]：F.4.2 共享对象 hint 表（Table F.5 头部 + Table F.6 组
//!   条目，两条序列、项交错序与 128 位 MD5 签名）；
//! - [`thumbnail`]：F.4.3 缩略图 hint 表（Table F.7 头部 12 项 + Table F.8
//!   逐页条目 3 项，三列各自按页序）；本表由 Table F.2 键 `T` 定位，键
//!   「Present only if thumbnail images exist」，**缺失是合法形态**，故不
//!   因缺键给诊断；
//! - [`generic`]：F.4.4 generic hint 表（Table F.9 的 4 项定宽 32 位，一张表
//!   只描述**一个**对象组）；Outline/Thread/Named destination/Information
//!   dictionary/Page label 五家共用该形态，即 Table F.2 键 `O`/`A`/`E`/`I`/`L`，
//!   每类一个独立条目给出表起始位置；五键都是**条件存在**，缺键同样不给诊断；
//! - [`extended`]：F.4.5 extended generic hint 表（Table F.10：项 1—4 同
//!   Table F.9，另加项 5 的共享对象引用数、项 6 的组内标识符位宽与项 7 起的
//!   标识符列）；Interactive form/Logical structure/Renditions name tree 三家
//!   共用该形态，即 Table F.2 键 `V`/`C`/`R`，同样**条件存在**、缺键不给诊断；
//!   声明版本 < 1.5 时三键退回 [`generic`] 形态（F.4.5「As of PDF 1.5」35781
//!   的版本门，见 [`extended`] 模块头）；
//! - [`assemble`]：装配层——读主/溢出 hint 流、过滤解码、拼接、按 Table F.2 键
//!   分发各家族并互校表位置（零行为变更自本文件拆出）。
//!
//! **未解析（诚实缺口）**：F.4 其余家族——embedded file stream hint 表
//! （F.4.6，Table F.11/F.12，Table F.2 键 `B`；F.4.5 的 35786—35788 明确把
//! 嵌入式文件流划归该家族）。这些表的位置仍按 Table F.2 键在
//! [`LinearizationHints::table_positions`] 里如实列出，但内容为原样字节，
//! 未类型化。
//!
//! 诊断进入 [`LinearizationHints::issues`]（复用 [`LinearizationIssue`]
//! 的 hint 家族新增变体），不写 `Document::warnings`；[`Document::linearization`]
//! 的行为完全不受本模块影响。

use super::dict::HintRange;
use super::issues::LinearizationIssue;

mod assemble;
pub mod bits;
mod extended;
mod generic;
mod page_offset;
mod page_offset_derive;
mod shared;
mod thumbnail;

#[cfg(test)]
mod tests;

pub use extended::ExtendedHintTable;
pub use generic::GenericHintTable;
pub use page_offset::{PageOffsetEntry, PageOffsetHeader, PageOffsetHintTable};
pub use page_offset_derive::SharedObjectPosition;
pub use shared::{SharedObjectGroupEntry, SharedObjectHeader, SharedObjectHintTable};
pub use thumbnail::{ThumbnailEntry, ThumbnailHeader, ThumbnailHintTable};

/// Table F.2 列出的标准 hint 表键（`S` 为 Required，其余按存在条件）。
/// 注册键（Annex E）不在标准集合内，按未知键跳过、不报错。
const STANDARD_HINT_KEYS: [(&str, &str); 11] = [
    ("S", "shared object hint table (F.4.2, Table F.5/F.6)"),
    ("T", "thumbnail hint table (F.4.3, Table F.7/F.8)"),
    ("O", "outline hint table (F.4.4 generic)"),
    ("A", "thread information hint table (F.4.4 generic)"),
    ("E", "named destination hint table (F.4.4 generic)"),
    ("V", "interactive form hint table (F.4.5 extended generic)"),
    ("I", "information dictionary hint table (F.4.4 generic)"),
    (
        "C",
        "logical structure hint table (F.4.5 extended generic, PDF 1.3)",
    ),
    ("L", "page label hint table (F.4.4 generic, PDF 1.3)"),
    (
        "R",
        "renditions name tree hint table (F.4.5 extended generic, PDF 1.5)",
    ),
    ("B", "embedded file stream hint table (F.4.6, PDF 1.5)"),
];

/// F.4 前言：页偏移 hint 表「shall be the first table in the stream and shall
/// start at offset 0」，没有 Table F.2 键，故位置常量而非字典条目。
pub const PAGE_OFFSET_TABLE_POSITION: u64 = 0;

/// 主 hint 流字典给出的一张表的位置条目（F.3.6：字节偏移，相对解码后的
/// 流数据起点，溢出流已拼接）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintTablePosition {
    /// Table F.2 键名。
    pub key: String,
    /// 表在（解码拼接后的）hint 流位流中的字节偏移。
    pub position: u64,
}

/// 类型化后的 hint 表数据与一致性诊断。
#[derive(Debug, Clone, PartialEq)]
pub struct LinearizationHints {
    /// 主 hint 流的对象范围（Table F.1 `/H` 第一对；复制自
    /// [`LinearizationInfo::hint_streams`]，供位置修正与溯源）。
    pub primary: HintRange,
    /// Table F.1 `/O`（复制自 [`LinearizationInfo::first_page_object`]）：
    /// Table F.4 项 1 的首页首对象号；负值/非整数为 `None`。
    pub first_page_object: Option<u64>,
    /// 解码并拼接溢出流后的位流总字节数。
    pub stream_bytes: usize,
    /// 按 Table F.2 标准键列出的表位置（缺失键不列）。
    pub table_positions: Vec<HintTablePosition>,
    /// 已完整解析的页偏移 hint 表（F.4.1）；该表无 Table F.2 键，恒在流偏移
    /// 0（[`PAGE_OFFSET_TABLE_POSITION`]）。Table F.1 `/N` 不可用时为 `None`
    /// 且不报错（结构层已就 `/N` 给诊断）；解析失败亦为 `None` 并在
    /// [`Self::issues`] 给出类型化诊断。
    pub page_offset: Option<PageOffsetHintTable>,
    /// 已完整解析的共享对象 hint 表（F.4.2）；解析失败为 `None` 并在
    /// [`Self::issues`] 给出类型化诊断。
    pub shared_object: Option<SharedObjectHintTable>,
    /// 已完整解析的缩略图 hint 表（F.4.3）；Table F.2 键 `T` 缺失（文档没有
    /// 缩略图）是合法形态，此时为 `None` 且**不给诊断**；键在而解析失败亦为
    /// `None`，并在 [`Self::issues`] 给出类型化诊断。
    pub thumbnail: Option<ThumbnailHintTable>,
    /// 已按 F.4.4 形态解析的 generic 对象组：Table F.2 键 `O`/`A`/`E`/`I`/`L`
    /// 每出现一个键给一条（一张 generic 表只描述**一个**对象组，35749—35751）。
    /// 五键都是**条件存在**，缺键即该类对象没给 hint，属合法形态，故列表可以
    /// 短于五且**不给诊断**；键在而解析失败则不列条目，并在 [`Self::issues`]
    /// 给出类型化诊断。
    pub generic: Vec<GenericHintTable>,
    /// 已按 F.4.5 形态解析的 extended 对象组：Table F.2 键 `V`/`C`/`R` 每出现
    /// 一个键给一条（一张表只描述**一个**对象组，项 1—4 与 [`GenericHintTable`]
    /// 同形，另加项 5—7 的共享对象引用信息）。三键都是**条件存在**，缺键属
    /// 合法形态、**不给诊断**；键在而解析失败则不列条目，并在 [`Self::issues`]
    /// 给出类型化诊断。声明版本 < 1.5 时三键走 F.4.4 形态、列在
    /// [`Self::generic`] 里，本列表为空（版本门见 [`extended`] 模块头）。
    pub extended: Vec<ExtendedHintTable>,
    /// hint 层一致性诊断；`LinearizationInfo::issues` 的文档结构诊断
    /// 不在此列（由 [`Document::linearization`] 报告）。
    pub issues: Vec<LinearizationIssue>,
}

impl LinearizationHints {
    /// hint 层是否一致（无诊断）。
    pub fn is_consistent(&self) -> bool {
        self.issues.is_empty()
    }

    /// 按 Table F.2 键查表位置。
    pub fn position_of(&self, key: &str) -> Option<u64> {
        self.table_positions
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.position)
    }

    /// F.4 前言 NOTE 3：hint 表内给出的位置「shall be interpreted as if
    /// the primary hint stream itself were not present」——大于主 hint 流
    /// 起点的位置须补上主 hint 流长度才是文件绝对偏移；溢出流的偏移与
    /// 长度按 F.4 前言末段不计。
    pub fn adjust_position(&self, position: u64) -> u64 {
        if position > self.primary.offset {
            position.saturating_add(self.primary.length)
        } else {
            position
        }
    }

    /// F.4.1 页偏移 hint 表结束后的第一个字节，即后续 Table F.2 各表位置的
    /// 下界；未解析成功时 `None`（F.4 前言：各表以字节边界开始且互不重叠）。
    pub fn page_offset_end_byte(&self) -> Option<u64> {
        self.page_offset.as_ref().map(PageOffsetHintTable::end_byte)
    }

    /// F.4.3 缩略图 hint 表结束后的第一个字节（流内绝对位置，起点是 Table
    /// F.2 键 `T` 的值）；本表未解析成功时 `None`。F.4 前言要求各表以字节边界
    /// 开始且互不重叠，故其他键的位置落进 `[T, 本值)` 即自相矛盾。
    pub fn thumbnail_end_byte(&self) -> Option<u64> {
        self.thumbnail.as_ref().map(ThumbnailHintTable::end_byte)
    }

    /// 按 Table F.2 键取 F.4.4 的 generic 对象组（`O`/`A`/`E`/`I`/`L`）：
    /// 键缺失（该类对象没给 hint，合法形态）与键在而解析失败都返回 `None`，
    /// **不是**空组——空组只可能是项 3 声明 0 个对象的表，见
    /// [`GenericHintTable::object_range`]。
    pub fn generic_group_of(&self, key: &str) -> Option<&GenericHintTable> {
        self.generic.iter().find(|table| table.key == key)
    }

    /// 键 `key` 的 generic 表结束后的第一个字节（流内绝对位置，起点是该键的
    /// 值）；本键没有可用表时 `None`。F.4 前言要求各表以字节边界开始且互不
    /// 重叠，故其他键的位置落进 `[该键, 本值)` 即自相矛盾。
    pub fn generic_end_byte(&self, key: &str) -> Option<u64> {
        self.generic_group_of(key).map(GenericHintTable::end_byte)
    }
}
