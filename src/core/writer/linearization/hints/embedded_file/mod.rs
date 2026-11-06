//! 内嵌文件流 hint 表（ISO 32000-1 F.4.6，Table F.11 头部 7 项 + Table F.12 每组
//! 条目）的**写端**编码：把类型化输入模型 [`EmbeddedFileTableInput`] 逐位还原成
//! F.4 前言「高位在前」的字节流，与既有读端
//! [`EmbeddedFileHintTable::parse`](crate::core::reader::linearization::hints::EmbeddedFileHintTable::parse)
//! 精确对偶。本模块落地后 F.4 六家的表级编码器全部齐备。
//!
//! 规范锚点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 输出行，沿用读端
//! `src/core/reader/linearization/hints/embedded_file/mod.rs` 已锚定的同一批）：
//! - 位流形态（35481—35483）：「shall be treated as a bit stream, high-order bit
//!   first … However, each hint table shall begin at a byte boundary」——按位追加、
//!   末尾补零，产物恒自字节边界开始，故可与相邻表字节相邻；
//! - 结构（35794—35797）：「shall have a header section (see Table F.11) … followed
//!   by the entries in Table F.12」，条目按头部项 3 的组数重复；
//! - **条目序（本家族最易写错处）**（35797—35799）：「the order of items in
//!   Table F.12 shall be item 1 for the first group, … repeated for the 5 items」
//!   ——**项主序（转置）**是 shall，故严格按五列铺开（所有组的项 1 → … → 项 5），
//!   绝不逐组连写；
//! - 组约定（35791—35794）：「all objects in such a group shall have adjacent
//!   object numbers. (A group shall contain no objects at all if it contains
//!   shared object references.)」——括号那句即下面互斥 shall 的出处；
//! - Table F.11 头部（35820—35832）：项 1—3 各 32 位（首组对象号、首组首位置、
//!   组数），项 4—7 各 16 位（Table F.12 四列的声明位宽），合计 3×32 + 4×16 ＝
//!   160 位；
//! - Table F.12 每组条目（35833—35850）：项 1 宽＝头部项 4（35835—35836）、项 2
//!   宽＝头部项 5（35837—35840）、项 3 宽＝头部项 6（35841—35844）、项 4 宽＝头部
//!   项 7（35845—35846）、**项 5 宽＝Table F.3 项 11**（35847—35850）；
//! - 项 2/项 3 的互斥 shall（35837—35844）：「This item may be 0 … item 4 for this
//!   group shall be greater than zero and item 3 shall be zero」及其对偶——一项为 0
//!   时另一项须同时为 0 且项 4 须大于 0。判定**直接调用读端**
//!   [`EmbeddedFileGroupEntry::zero_item_problems`]（两侧口径同源），违规即
//!   [`EmbeddedFileEncodeError::SharedOnlyGroupViolation`]；
//! - 项 5 的宽度只有一个来源（35847 的「See Table F.3, item 11」，定义见
//!   35546—35548）：它是**文档级**数字、不由本表自带，故
//!   [`EmbeddedFileTableInput::shared_object_id_bits`] 与读端 `parse` 的第三形参一样
//!   是**显式入参**，绝不从本表推导、也绝不拿 0 宽冒充（那会把标识符全伪造成 0）；
//!   项 5 每组的个数由该组项 4 给出，不符即
//!   [`EmbeddedFileEncodeError::ReferenceCountMismatch`]；
//! - 位宽范围（35524—35525）：「… in the range 0 through 32. Although that range
//!   requires only 6 bits, 16-bit numbers shall be used」——**字面只约束 Table F.3**，
//!   故 `shared_object_id_bits` 的 0—32 是规范字面门；Table F.11 项 4—7 按同一措辞与
//!   同一 16 位存储**类比**沿用读端 `page_offset` 的 `MAX_DECLARED_WIDTH` 施加同一
//!   上限（**库策略**，非规范字面），越界即
//!   [`EmbeddedFileEncodeError::DeclaredWidthOutOfRange`]。
//!
//! 文件划分（体量红线：每文件 ≤600 行且 ≤25 600 字节）：本文件承载**契约面**——
//! 常量、[`EmbeddedFileEncodeError`] 与 [`EmbeddedFileTableInput`] 模型；位装配
//! （`encode_embedded_file_table` 及其 `validate`/`write_header`/`write_columns`）
//! 住在 [`encode`] 子模块，经 [`encode::encode_embedded_file_table`] 原样再导出，
//! 公开面与同族的单文件形态逐字一致。
//!
//! **键 `B` 与表在流里的位置属调用方**：`B`「(Present only if embedded file
//! streams exist; PDF 1.5)」（35381—35382）**条件存在**，文档没有内嵌文件流时缺键
//! 合法，故本模块不判断「是否该写这张表」；产物起点即 `B` 的值（F.3.6 要求主流
//! 字典给出每张表的位置，35313—35316）。头部项 2（35823）是文件偏移，仍受 F.4
//! 前言 NOTE 3 约束，模型里存 hint 表空间的值。35786—35788 另把内嵌文件流划出
//! F.4.5，故本家族与 `V`/`C`/`R` 三键互斥；写端不判版本、不设 1.5 门。
//!
//! **模型复用读端类型、但不带读端专有字段**：输入复用 [`EmbeddedFileHeader`] 与
//! [`EmbeddedFileGroupEntry`]，**不**接受读端 `EmbeddedFileHintTable` 整体——它含
//! `start_byte`/`bits_consumed` 两个**解析侧游标**，对写端无意义：起点由键 `B` 的值
//! 给出（属装配切片），位消耗由 [`EmbeddedFileTableInput::total_bits`] 决定，测试用
//! 后者断言往返后读端的 `bits_consumed` 与模型一致。
//!
//! **校验先于任何位**：文档级项 5 宽度与头部项 4—7 的范围、组向量非空且与项 3
//! 等长、每组项 4 与项 5 列表等长、项 2/项 3 的互斥 shall、位宽全 0 时的读端
//! 计数上限，全部在写出第一位之前完成；数值溢出由
//! [`HintBitWriter::unsigned`](super::super::bits::HintBitWriter::unsigned) 逐字段
//! 拒绝，`Result::Err` 时不返回任何字节。读端位预算门的对偶：项 1—4 宽度全 0 时
//! 每组不占位、项 5 宽度为 0 时标识符不占位，位流给不出计数证据，读端对此类计数设
//! `MAX_UNEVIDENCED_ITEMS`（65 536）绝对上限并拒绝，本模块同口径提前拒绝（**库策略**，
//! 非规范条款），保证「写得出的一定读得回」。
//!
//! **剩余缺口**：Table F.12 **不给**每组的第一个对象号/位置，故 35792 那句相邻
//! shall 除第一组（由头部项 1—2 锚定，读端
//! [`first_group_anchor_problem`](crate::core::reader::linearization::hints::EmbeddedFileHintTable::first_group_anchor_problem)
//! 的**派生**核对）外无从判定；本模块不校验其余组的号区间相邻性（不发明 shall），
//! 也不校验项 5 标识符是否落进 F.4.2 共享对象表的条目范围（跨表核对属装配切片）。
//! 位级错误 [`HintEncodeError`] 住在写端
//! `bits.rs`（只读复用，不在本切片写集），故家族特有的拒绝住在
//! [`EmbeddedFileEncodeError`]，位级拒绝经 [`EmbeddedFileEncodeError::Bit`] 原值
//! 透传（与 `extended/`、`thumbnail/` 同一处理）。

use crate::core::reader::linearization::hints::{EmbeddedFileGroupEntry, EmbeddedFileHeader};

use super::super::bits::HintEncodeError;

mod encode;

pub use encode::encode_embedded_file_table;

/// Table F.11 项 1—3 的定宽（位；35822—35824 的「32」列）。
const TABLE_F11_FIXED_ITEM_WIDTH_BITS: u32 = 32;

/// Table F.11 项 4—7 的存储宽度（位；35825—35831 的「16」列，即 35525 的
/// 「16-bit numbers shall be used」）。
const TABLE_F11_WIDTH_FIELD_BITS: u32 = 16;

/// Table F.11 头部 7 项的定宽合计（位）：3×32 + 4×16 = 160（35820—35832）。读端
/// 同名常量 `EmbeddedFileHeader::HEADER_BITS` 是 `pub(super)`，故在此重述并单测钉住。
const HEADER_BITS: u64 = 160;

/// 声明位宽的期望上限（0—32）：文档级项 5 宽度是规范字面（35524—35525 约束
/// Table F.3），头部项 4—7 按同一措辞**类比**沿用（与读端 `MAX_DECLARED_WIDTH`
/// 同值，见模块头）。
const MAX_DECLARED_WIDTH: u32 = 32;

/// 位宽全为 0 时读端对组数／标识符总数的绝对上限（读端 `MAX_UNEVIDENCED_ITEMS`
/// 同值，属**库策略**而非条款：35824 只给组数，未给计数上限）。
const MAX_UNEVIDENCED_ITEMS: u64 = 65_536;

/// [`encode_embedded_file_table`] 的类型化拒绝，全部在写出第一位之前给出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddedFileEncodeError {
    /// **文档级**项 5 宽度（Table F.3 项 11，35546—35548）越出 0—32：35524—35525
    /// 的**字面**范围，项 5 的列宽与列长都由它决定，保留原值毫无意义。
    SharedIdWidthOutOfRange {
        /// 模型声明的文档级位宽。
        bits: u64,
    },
    /// Table F.11 项 4/5/6/7 之一越出 0—32（按 35524—35525 的同一措辞与同一
    /// 16 位存储**类比**施加，属库策略，见模块头）。
    DeclaredWidthOutOfRange {
        /// 越界的表内项名（与读端字段同名，便于对照）。
        item: &'static str,
        /// 模型声明的位宽。
        bits: u64,
    },
    /// 组向量为空：键 `B` 的条件是「有内嵌文件流」（35381），35795—35797 又规定条目
    /// 按组数重复，故「有表无组」自相矛盾（**派生**结论：F.4.6 未写 F.4.3 那句
    /// 「shall include one or more per-page entries」；口径同读端诊断
    /// [`EmbeddedFileGroupCountZero`](crate::core::reader::linearization::LinearizationIssue::EmbeddedFileGroupCountZero)）。
    GroupsMissing,
    /// Table F.11 项 3 的声明组数与 [`groups`](EmbeddedFileTableInput::groups)
    /// 的实际长度不符：读端按项 3 决定每列读几个值，不符的位流要么多读要么少读，
    /// 无从逐项往返。
    GroupCountMismatch {
        /// 项 3 的声明值。
        declared: u64,
        /// 组向量实际长度。
        actual: usize,
    },
    /// 项 4 的声明数与该组
    /// [`shared_object_ids`](EmbeddedFileGroupEntry::shared_object_ids) 的实际长度
    /// 不符（35848—35850：项 5 的个数就是项 4）——读端按项 4 把扁平的项 5 列切回
    /// 各组，不符要么串组要么留空。
    ReferenceCountMismatch {
        /// 违规的组序（自 0 起）。
        group: usize,
        /// 项 4 的声明值。
        declared: u64,
        /// 该组标识符向量的实际长度。
        actual: usize,
    },
    /// 项 2/项 3 的互斥 shall（35837—35844，另见 35791—35794「A group shall contain
    /// no objects at all if it contains shared object references」）被违反；规则文本
    /// 取读端 [`EmbeddedFileGroupEntry::zero_item_problems`]，两侧同源。
    SharedOnlyGroupViolation {
        /// 违规的组序（自 0 起）。
        group: usize,
        /// 被违反的规则原文口径（读端常量文本）。
        rule: &'static str,
        /// 该项的组数（项 2）。
        object_count: u64,
        /// 该项的组长度（项 3）。
        group_length: u64,
        /// 该项的共享引用数（项 4）。
        shared_reference_count: u64,
    },
    /// 头部项 4—7 全 0 时每组不占位、位流给不出组数证据，读端设绝对上限拒绝
    /// （`fits_groups`）——写端同口径拒绝（**库策略**，非规范条款）。
    UnevidencedGroupsTooLarge {
        /// 组向量的条数。
        count: u64,
        /// 与读端一致的绝对上限。
        limit: u64,
    },
    /// 文档级项 5 宽度为 0 时每个标识符不消费任何位，位流给不出项 4 总数的证据，
    /// 读端对此类计数设绝对上限并拒绝（`bounded_identifier_count`）——写端同口径
    /// 拒绝（**库策略**，非规范条款）。
    UnevidencedIdentifiersTooLarge {
        /// 各组项 4 之和（＝标识符总数）。
        count: u64,
        /// 与读端一致的绝对上限。
        limit: u64,
    },
    /// 位级拒绝（原值透传）：定宽字段装不下给定数值，如头部项 1—3 的某个值
    /// ≥ 2^32、项 4—7 的声明值 ≥ 2^16，或某个组值／标识符超出其声明列宽——读端
    /// 会截断高位，往返不再逐项相等。
    Bit(HintEncodeError),
}

impl std::fmt::Display for EmbeddedFileEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SharedIdWidthOutOfRange { bits } => write!(
                f,
                "F.4.6 Table F.12 item 5: the document-level width from Table F.3 item 11 \
                 (35546-35548) must be in 0..=32 by the literal wording of 35524-35525, got {bits}"
            ),
            Self::DeclaredWidthOutOfRange { item, bits } => write!(
                f,
                "F.4.6 Table F.11 {item}: a bits-needed width is capped at 0..=32 by the same \
                 wording and 16-bit storage as Table F.3 (35524-35525, applied here as library \
                 policy), got {bits}"
            ),
            Self::GroupsMissing => write!(
                f,
                "F.4.6: key B is present only if embedded file streams exist (35381) and the \
                 Table F.12 entries repeat per group (35795-35797), so a table with no group at \
                 all is self-contradictory; the model carries none"
            ),
            Self::GroupCountMismatch { declared, actual } => write!(
                f,
                "F.4.6 Table F.11 item 3 declares {declared} group(s) but the model carries \
                 {actual}; the reader consumes exactly item 3 values of every Table F.12 column, \
                 so an inconsistent model cannot round-trip"
            ),
            Self::ReferenceCountMismatch {
                group,
                declared,
                actual,
            } => write!(
                f,
                "F.4.6 Table F.12: group {group} declares item 4 = {declared} shared object \
                 reference(s) but carries {actual} identifier(s) for item 5; the reader slices \
                 the item 5 column by each group's item 4, so a mismatch cannot round-trip"
            ),
            Self::SharedOnlyGroupViolation {
                group,
                rule,
                object_count,
                group_length,
                shared_reference_count,
            } => write!(
                f,
                "F.4.6 Table F.12 group {group}: {rule} (35837-35844), but the model carries \
                 item 2 = {object_count}, item 3 = {group_length}, item 4 = {shared_reference_count}"
            ),
            Self::UnevidencedGroupsTooLarge { count, limit } => write!(
                f,
                "F.4.6 Table F.12: with Table F.11 items 4-7 all 0 the groups consume no bits and \
                 the bit budget cannot evidence the count, and the reader caps it at {limit}; \
                 refusing {count} keeps every written byte decodable"
            ),
            Self::UnevidencedIdentifiersTooLarge { count, limit } => write!(
                f,
                "F.4.6 Table F.12 item 5: with the document-level width 0 the identifiers \
                 consume no bits and the bit budget cannot evidence the count, and the reader \
                 caps it at {limit}; refusing {count} keeps every written byte decodable"
            ),
            Self::Bit(problem) => write!(f, "{problem}"),
        }
    }
}

impl std::error::Error for EmbeddedFileEncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bit(problem) => Some(problem),
            _ => None,
        }
    }
}

impl From<HintEncodeError> for EmbeddedFileEncodeError {
    fn from(problem: HintEncodeError) -> Self {
        Self::Bit(problem)
    }
}

/// F.4.6 内嵌文件流 hint 表的写端类型化输入模型：只承载**进位流**的值。
///
/// - [`header`](Self::header)：Table F.11 七项（复用读端 [`EmbeddedFileHeader`]），
///   3×32 + 4×16 = 160 位，项 3 的组数须与 [`groups`](Self::groups) 严格等长；
/// - [`groups`](Self::groups)：Table F.12 的每组条目（复用读端
///   [`EmbeddedFileGroupEntry`]），按组序排列；
/// - [`shared_object_id_bits`](Self::shared_object_id_bits)：**文档级**的 Table F.3
///   项 11（35847 的「See Table F.3, item 11」），项 5 列唯一的宽度来源，须与同一
///   文档页偏移表声明的值一致（跨表一致性属调用方：本入口只看单张表）。
///
/// 读端 `EmbeddedFileHintTable` 的 `start_byte`/`bits_consumed` 是解析侧游标：前者由
/// Table F.2 键 `B` 的值给出、后者由 [`Self::total_bits`] 决定，都不进模型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFileTableInput {
    /// Table F.11 头部七项；项 3 须等于 [`groups`](Self::groups) 的长度。
    pub header: EmbeddedFileHeader,
    /// Table F.12 的每组条目（组序；至少一组）。
    pub groups: Vec<EmbeddedFileGroupEntry>,
    /// 项 5 的**文档级**位宽（Table F.3 项 11，0—32），与读端 `parse` 的第三
    /// 形参同值同义。
    pub shared_object_id_bits: u32,
}

impl EmbeddedFileTableInput {
    /// Table F.12 一个条目在项 1—4 上消费的位数（四项声明宽度之和，saturating；
    /// 与读端同口径，项 5 另按各组项 4 追加）。
    pub fn bits_per_group(&self) -> u64 {
        [
            self.header.object_number_bits,
            self.header.group_object_bits,
            self.header.group_length_bits,
            self.header.shared_ref_count_bits,
        ]
        .into_iter()
        .fold(0u64, |sum, bits| sum.saturating_add(u64::from(bits)))
    }

    /// 各组项 4 之和（＝校验通过后的标识符总数）。
    pub fn shared_object_id_count(&self) -> u64 {
        self.groups.iter().fold(0u64, |sum, group| {
            sum.saturating_add(group.shared_object_ids.len() as u64)
        })
    }

    /// 项 5 整列（所有组扁平相加）占用的位数：标识符总数 × 文档级宽度。
    pub fn identifier_bits(&self) -> u64 {
        self.shared_object_id_count()
            .saturating_mul(u64::from(self.shared_object_id_bits))
    }

    /// 本表总位数：头部恒 160 位，五列按声明宽度与列长追加。读端解析成功时给出的
    /// `bits_consumed` 须等于它。
    pub fn total_bits(&self) -> u64 {
        HEADER_BITS
            .saturating_add(
                self.bits_per_group()
                    .saturating_mul(self.groups.len() as u64),
            )
            .saturating_add(self.identifier_bits())
    }

    /// 本表字节长度（读端
    /// [`end_byte`](crate::core::reader::linearization::hints::EmbeddedFileHintTable::end_byte)
    /// 减起点的同一 `div_ceil` 口径）：各表以字节边界开始（35481—35483），故写端
    /// 把尾部残位补零，本值即下一张表的最小合法位置。
    pub fn byte_len(&self) -> u64 {
        self.total_bits().div_ceil(8)
    }
}

#[cfg(test)]
mod tests;
