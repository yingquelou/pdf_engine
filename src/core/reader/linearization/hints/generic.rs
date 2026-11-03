//! generic hint table（ISO 32000-1 F.4.4，Table F.9）。
//!
//! 本家族与前三家的最大差别：**一张表服务五类对象**。F.4.4 把 generic 形态
//! 指派给 Outline / Thread information / Named destination / Information
//! dictionary / Page label 五张 hint 表，即 Table F.2 的键 `O`/`A`/`E`/`I`/`L`
//! （名册 35320—35338 与续栏 35379—35382；名单 35752—35756），故解析结果
//! 自带身份（[`GenericHintTable::key`]），分发按 [`GENERIC_HINT_KEYS`] 逐键取
//! 位置，而不是像 `S`/`T` 那样一个键一张唯一的表。
//!
//! 原文要点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 的输出行）：
//! - 适用前提：「Categories of objects are associated with the document as a
//!   whole rather than with individual pages (see F.3.10, "Other Objects
//!   (Part 9)"), and hints should be provided for accessing those objects
//!   efficiently. For each category of hints, there shall be a separate entry
//!   in the primary hint stream giving the starting position of the table
//!   within the stream」（35745—35748）——每类一个独立键，位置由 F.3.6 的
//!   主 hint 流字典给出，即 [`super::LinearizationHints::position_of`]。
//! - 形态：「Such hints shall be represented by a generic hint table, which
//!   describes a single group of objects that are located together in the PDF
//!   file. The entries in this table are listed in Table F.9」（35749—35751）
//!   ——**一张 generic 表只描述一个对象组**，没有条目数、没有逐页序，也没有
//!   交错列；F.4.6 才为多组引入 Table F.11/F.12 的「头部＋逐组条目」两段式。
//! - Table F.9 四项，全部定宽 32 位（35801—35805）：①组内第一个对象的对象号
//!   ②组内第一个对象的位置 ③组内对象数 ④对象组的长度（字节），合计 128 位
//!   ＝16 字节整，故本表是唯一一张**长度与声明无关**的 hint 表。
//! - 「Generic hint tables may be used for product-specific objects accessed
//!   by conforming readers」（35757）：产品专有表须走 Annex E 注册键，而注册键
//!   的形态本库无从判定，故 [`super`] 只按标准名单里的五键分发，不猜未知键。
//! - 存在条件是**逐类**的：`O`「Present only if a document outline exists」、
//!   `A`「… article threads …」、`E`「… named destinations …」、`I`「…
//!   document information dictionary …」（35326—35336），`L` 标 PDF 1.3
//!   （35338）。五键全为**条件存在**，缺键即该类没有对象或没给 hint，是合法
//!   形态（与 Table F.2 里 Required 的 `S` 不同，与 `T` 同口径），故缺键
//!   **不给诊断**，[`super::LinearizationHints::generic_group_of`] 退回 `None`
//!   而非空组。
//! - 表内位置是文件偏移，按 F.4 前言 NOTE 3 修正（35500—35503）：
//!   [`super::LinearizationHints::adjust_position`]；组覆盖的字节范围即
//!   项 2 修正后的位置到该项 + 项 4（本库不折算，交给调用方组合）。
//!
//! **待核**（一律取保守行为：保留原值、不猜默认、不做除法折算）：
//! - 位数类条目的硬约束只写在 F.4.1 名下：「All the items in Table F.3 that
//!   specify a number of bits needed, such as item 3, have values in the range
//!   0 through 32. Although that range requires only 6 bits, 16-bit numbers
//!   shall be used」（35524—35525）。Table F.9 四项全是定宽、**没有**声明位数
//!   的条目，故本家族无从越界； [`GenericHintTable::parse_with_item_width`]
//!   把该 0—32 范围做成唯一的运行时位宽门（越界即类型化拒绝
//!   [`HintBitError::InvalidWidth`]，绝不退化成未定义行为），因为紧接着的
//!   F.4.5（Table F.10 项 6，标 16 位的「组内最大共享对象标识符位数」）与
//!   F.4.6（Table F.11 项 4—7）读的是**不可信声明宽度**，且都以本表四项开头
//!   （「An extended generic hint table shall begin with the same entries as in
//!   a generic hint table」35779—35780）。
//! - Table F.10 项 6 与项 7 的字面引用「See Table F.3, item 11」之间的组内／
//!   全页宽度歧义属**extended generic 切片**（F.4.5），本轮不解决。
//! - 项 3 声明 0 个对象（[`GenericHintTable::object_range`] 退回 `None`）：
//!   F.4.4 没有「组须至少含一个对象」的 shall（该措辞只出现在 F.4.6 的
//!   「A group shall contain no objects at all if it contains shared object
//!   references」35793—35794），故空组只按声明值原样保留、**不给诊断**。

use super::bits::{BitReader, HintBitError};
use super::page_offset::MAX_DECLARED_WIDTH;

/// Table F.9 四项的定宽（位，35801—35805）。
const TABLE_F9_ITEM_WIDTH_BITS: u32 = 32;

/// F.4.4 用 generic 形态承载的 Table F.2 键（35752—35756 的名单，按名册
/// 35320—35338 的键序）：`O` 大纲、`A` 文章线、`E` 命名目的地、`I` 信息字典、
/// `L` 页码标签。五键都是**条件存在**，缺键不报错。
pub(super) const GENERIC_HINT_KEYS: [&str; 5] = ["O", "A", "E", "I", "L"];

/// F.4.4 / Table F.9：一个对象组的 generic hint 表（4 项 × 32 位 = 128 位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericHintTable {
    /// 本组来自哪个 Table F.2 键（[`GENERIC_HINT_KEYS`] 之一）。同一张表形态
    /// 服务五类对象，故身份须跟着解析结果走，调用方才能按键取组。
    pub key: &'static str,
    /// 项 1（32 位）：组内第一个对象的对象号。
    pub first_object_number: u64,
    /// 项 2（32 位）：组内第一个对象的位置（文件偏移，须按 F.4 前言 NOTE 3
    /// 用 [`super::LinearizationHints::adjust_position`] 修正）。
    pub first_object_location: u64,
    /// 项 3（32 位）：组内对象数。
    pub object_count: u64,
    /// 项 4（32 位）：对象组的长度（字节）。
    pub group_length: u64,
    /// 本表在主 hint 流位流里的起始字节偏移，即 Table F.2 该键的值。
    pub start_byte: u64,
    /// 本表消费掉的位数（按 Table F.9 定宽读取时恒为 128）。
    pub bits_consumed: u64,
}

impl GenericHintTable {
    /// 从 `data`（主/溢出 hint 流解码拼接后的位流）的字节偏移 `start` 处按
    /// Table F.9 的定宽 32 位解析键 `key` 的对象组。
    ///
    /// `start` 由主 hint 流字典的 Table F.2 键给出（F.4.4：「there shall be a
    /// separate entry in the primary hint stream giving the starting position
    /// of the table」35747—35748），F.4 前言另要求表以字节边界开始
    /// （35482—35483）。
    pub fn parse(data: &[u8], start: usize, key: &'static str) -> Result<Self, HintBitError> {
        Self::parse_with_item_width(data, start, key, TABLE_F9_ITEM_WIDTH_BITS)
    }

    /// 按给定的**项宽**读取 Table F.9 的四项，同时是本家族唯一的位宽门。
    ///
    /// 位数类条目的取值范围是 0—32（35524—35525，原文字面约束 Table F.3，
    /// [`super::page_offset`] 与 [`super::thumbnail`] 已在同一措辞下沿用该
    /// 上限）：越界一律 [`HintBitError::InvalidWidth`] 类型化拒绝，不猜默认值、
    /// 不做未定义行为。Table F.9 自身四项定宽 32 位，[`Self::parse`] 恒以该
    /// 定宽进来，故规范文档里读到的 generic 表与本入口完全同形；后续 F.4.5
    /// （Table F.10 项 6）与 F.4.6（Table F.11 项 4—7）以**不可信声明宽度**
    /// 复用本门。
    pub fn parse_with_item_width(
        data: &[u8],
        start: usize,
        key: &'static str,
        item_bits: u32,
    ) -> Result<Self, HintBitError> {
        if item_bits > MAX_DECLARED_WIDTH {
            return Err(HintBitError::InvalidWidth {
                bits: u64::from(item_bits),
            });
        }
        if start > data.len() {
            return Err(HintBitError::ShortBuffer {
                needed_bits: 8,
                remaining_bits: 0,
                position_bits: start as u64 * 8,
            });
        }
        let mut reader = BitReader::new(&data[start..]);
        Ok(Self {
            key,
            first_object_number: reader.unsigned(item_bits)?,
            first_object_location: reader.unsigned(item_bits)?,
            object_count: reader.unsigned(item_bits)?,
            group_length: reader.unsigned(item_bits)?,
            start_byte: start as u64,
            bits_consumed: reader.position_bits(),
        })
    }

    /// 本表末尾之后的第一个字节（流内绝对位置，起点是 Table F.2 键的值）。
    /// F.4 前言要求各表以字节边界开始且互不重叠，故其他键的位置落进
    /// `[start_byte, 本值)` 即自相矛盾。
    pub fn end_byte(&self) -> u64 {
        self.start_byte
            .saturating_add(self.bits_consumed.div_ceil(8))
    }

    /// 项 1 + 项 3：组内对象号的**闭**区间 `(first, last)`。项 3 声明 0 个
    /// 对象时返回 `None`——组里没有对象，号区间无从给出（见模块头的**待核**：
    /// F.4.4 没有「组须至少一个对象」的 shall，故空组原样保留、不给诊断）。
    /// 由项 1/项 3 的字面定义**推出**的访问器，非规范条款；累加一律
    /// saturating，绝不因不可信位流溢出 panic。
    pub fn object_range(&self) -> Option<(u64, u64)> {
        if self.object_count == 0 {
            return None;
        }
        let last = self
            .first_object_number
            .saturating_add(self.object_count.saturating_sub(1));
        Some((self.first_object_number, last))
    }
}
