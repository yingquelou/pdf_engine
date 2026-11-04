//! extended generic hint table（ISO 32000-1 F.4.5，Table F.10）的**写端**编码：
//! 把类型化输入模型 [`ExtendedHintTableInput`] 逐位还原成 F.4 前言「高位在前」的
//! 字节流，与既有读端 [`ExtendedHintTable::parse`](crate::core::reader::linearization::hints::ExtendedHintTable::parse)
//! 精确对偶。
//!
//! 规范锚点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 输出行，沿用读端
//! `src/core/reader/linearization/hints/extended.rs` 已锚定的同一批）：
//! - F.4.5（35779—35780）：「An extended generic hint table shall begin with the
//!   same entries as in a generic hint table, and shall be followed by three
//!   additional entries, as shown in Table F.10」——项 1—4 与 Table F.9（35802—35805）
//!   逐字同形，故本模块写出的**前 16 字节恒等于**同四项的
//!   [`encode_generic_table`](super::generic::encode_generic_table) 产物（单元
//!   测试 `first_sixteen_bytes_equal_the_generic_table_encoding` 钉住该 shall）；
//!   多出的三项是项 5（32 位引用数）、项 6（16 位组内位宽）与项 7 起的标识符列；
//! - Table F.10 项 1—7（35808—35817）：①组首对象号 ②组首位置 ③组内对象数
//!   ④组长度（字节）⑤共享对象引用数 ⑥「表示本组用到的最大共享对象标识符所需的
//!   位数」⑦起「each of the remaining items in this table shall be a shared object
//!   identifier—that is, an index into the shared object hint table」，其 size 列
//!   字面写「See Table F.3, item 11」；
//! - 名册（35781—35785）：「As of PDF 1.5, the following hint tables, if needed,
//!   shall use the extended generic format: Interactive form / Logical structure /
//!   Renditions name tree hint table」，即 Table F.2 的 `V`（35332—35333）、`C`
//!   （35336—35337）、`R`（35379—35380）三键——[`ExtendedHintTableInput::key`]
//!   只接受这三键，其余键不属本家族（`O`/`A`/`E`/`I`/`L` 属 F.4.4，`B` 由
//!   35786—35788 明确划归 F.4.6：「Embedded file streams shall not be referred to
//!   by this hint table … instead they shall use the hint table described in
//!   F.4.6」）；
//! - 位宽范围（35524—35525）：「All the items in Table F.3 that specify a number
//!   of bits needed … have values in the range 0 through 32. Although that range
//!   requires only 6 bits, 16-bit numbers shall be used」——该句字面只约束 Table F.3；
//!   项 6 用同一措辞、同一 16 位存储，故与读端同一口径**类比**沿用 0—32 上限，
//!   越界即 [`ExtendedHintEncodeError::IdWidthOutOfRange`]（conforming writer 直接
//!   拒绝：项 7 的列长直接由它决定，保留原值毫无意义）；声明值按 16 位写出。
//!
//! **项 7 的打包宽度取组内项 6**（与读端同一裁决，见该文件模块头的**待核**）：
//! 项 7 的 size 列指向**文档级** Table F.3 项 11「The number of bits needed to
//! represent the numerically greatest shared object identifier used by the
//! pages」（35546—35548），而项 6 声明的是**本组**的同一量（35813—35814），原文
//! 没有任何一句裁定谁优先。本模块按读端的读法用**项 6** 打包项 7，理由与读端相同
//! （项 6 装在本表里、本表因此自描述）。跨表一致性不是本模块的职责：文档级宽度是
//! **另一张表**（页偏移表 Table F.3 项 11）的字段，本入口只看单张表，故**不**做
//! 跨表拒绝——读端在两张表都在场时给派生的
//! [`ExtendedHintIdWidthExceedsDocument`](crate::core::reader::linearization::LinearizationIssue::ExtendedHintIdWidthExceedsDocument)
//! 诊断（该核对由两处字段定义**推出**，非原文 shall）。
//!
//! **绝对值，无增量、无累加**：与 F.4.1 的交错增量列不同，Table F.10 项 1—4 与
//! 项 7 都是绝对值，本家族在表内**没有**需要跨项或跨组累加的量，故写端不存在
//! 「只编增量、累加留给读端」的分工；读端的
//! [`object_range`](crate::core::reader::linearization::hints::ExtendedHintTable::object_range)
//! 与
//! [`shared_id_width_problem`](crate::core::reader::linearization::hints::ExtendedHintTable::shared_id_width_problem)
//! 都是**派生**访问器，本模块不复制。项 2 是文件偏移，仍受 F.4 前言 NOTE 3
//! （35500—35503「the resulting positions shall be interpreted as if the primary
//! hint stream itself were not present」）约束——模型里存的是 hint 表空间的值，
//! 真实文件偏移与主 hint 流长度属后续装配切片，本切片不做 F.3.1 分部布局。
//!
//! **版本门属读端**：35781 只说「as of PDF 1.5 … shall use the extended generic
//! format」，没有裁定 < 1.5 时这三键的形态；读端据此把 < 1.5 的 `V`/`C`/`R` 交给
//! Table F.9 形态解析（**库策略**，见读端模块头）。写端不判版本，只产出 Table F.10
//! 形态，**调用方须保证文档声明版本 ≥ 1.5**：否则读端只消费前 16 字节（项 1—4）
//! 而把项 5—7 留给下一张表的起点，写出的表会被当成 generic 表。
//!
//! **校验先于任何位**：名册、项 6 的 0—32、项 5 与标识符向量长度自洽、宽度 0 时
//! 的列长上限，全部在写出第一位之前完成；给定宽度下的数值溢出由
//! [`HintBitWriter::unsigned`] 逐项拒绝，且 `Result::Err` 时不返回任何字节——
//! 绝不落半成品位流。
//! 读端位预算门的对偶：项 6 为 0 时项 7 不占位、位流给不出引用数的证据，读端对
//! 该类计数设 `MAX_UNEVIDENCED_ITEMS`（65 536）绝对上限并拒绝，本模块同口径提前
//! 拒绝（[`ExtendedHintEncodeError::UnevidencedReferencesTooLarge`]），保证「写得出
//! 的一定读得回」。
//!
//! **错误形态**：F.4 各家族共用的位级错误 [`HintEncodeError`] 住在
//! `src/core/writer/linearization/bits.rs`，本切片的写集不含该文件，故家族特有的
//! 拒绝（名册键、项 6 范围、项 5 与向量自洽、宽度 0 的列长上限）住在本模块的
//! [`ExtendedHintEncodeError`]，位级拒绝原值经 [`ExtendedHintEncodeError::Bit`]
//! 透传，消息不与 F.4.1/F.4.2 的表号混淆。

use super::super::bits::{HintBitWriter, HintEncodeError};

/// Table F.10 项 1—5 的定宽（位；35808—35812 的「32」列）。与读端
/// `extended.rs::TABLE_F10_FIXED_ITEM_WIDTH_BITS` 同值。
const TABLE_F10_FIXED_ITEM_WIDTH_BITS: u32 = 32;

/// Table F.10 项 6 的存储宽度（位；35813 的「16」列）。与读端
/// `extended.rs::TABLE_F10_WIDTH_FIELD_BITS` 同值。
const TABLE_F10_WIDTH_FIELD_BITS: u32 = 16;

/// 项 1—6 的定宽合计（位）：4×32 + 32 + 16 = 176（35808—35814），
/// 即读端 `bits_consumed` 在项 5 为 0 时的取值。
const HEADER_BITS: u64 = 176;

/// 项 6 的上限（0—32，35524—35525 类比；与读端 `MAX_DECLARED_WIDTH` 同值）。
const MAX_DECLARED_WIDTH: u32 = 32;

/// 项 6 为 0 时读端对项 7 列长的绝对上限（读端 `MAX_UNEVIDENCED_ITEMS` 同值，
/// 属**库策略**而非条款：原文只给「See Table F.3, item 11」，未给计数上限）。
const MAX_UNEVIDENCED_ITEMS: u64 = 65_536;

/// F.4.5 用 extended 形态承载的 Table F.2 键（名册 35332—35333 的 `V`、
/// 35336—35337 的 `C`、35379—35380 的 `R`，按名册序；与读端
/// `extended.rs::EXTENDED_HINT_KEYS` 同值）。
const EXTENDED_HINT_KEYS: [&str; 3] = ["V", "C", "R"];

/// [`encode_extended_table`] 的类型化拒绝，全部在写出第一位之前给出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtendedHintEncodeError {
    /// [`key`](ExtendedHintTableInput::key) 不在 F.4.5 的名册（35781—35785 的
    /// `V`/`C`/`R`）里：`B` 由 35786—35788 明确划归 F.4.6，`O`/`A`/`E`/`I`/`L`
    /// 属 F.4.4 的 Table F.9，都不该由本编码器产出。
    KeyNotInExtendedRoster {
        /// 被拒绝的 Table F.2 键。
        key: &'static str,
    },
    /// 项 6 越出 0—32（35524—35525 的同一措辞类比，35813—35814）。
    IdWidthOutOfRange {
        /// 模型声明的位宽。
        bits: u64,
    },
    /// 项 5 的声明数与 [`shared_object_ids`](ExtendedHintTableInput::shared_object_ids)
    /// 的实际长度不符：读端按项 5 决定项 7 读几个（成功解析的表其列长恒等于项 5），
    /// 计数与向量不符的模型写出的位流要么多读要么少读，无从逐项往返。
    ReferenceCountMismatch {
        /// 项 5 的声明值。
        declared: u64,
        /// 向量实际长度。
        actual: usize,
    },
    /// 项 6 为 0 时项 7 的每个标识符不占位，位流给不出引用数的证据，读端对此类
    /// 计数设绝对上限并拒绝（`bounded_identifier_count`）——写端同口径拒绝，
    /// 保证写出的每个字节都解得开（**库策略**，非规范条款）。
    UnevidencedReferencesTooLarge {
        /// 项 5 声明（＝向量长度）的条数。
        count: u64,
        /// 与读端一致的绝对上限。
        limit: u64,
    },
    /// 位级拒绝（原值透传）：定宽字段装不下给定数值，如项 1—5 的某个值 ≥ 2^32、
    /// 项 6 的声明值 ≥ 2^16，或某个标识符超出项 6 声明的宽度——读端会截断高位，
    /// 往返不再逐项相等。
    Bit(HintEncodeError),
}

impl std::fmt::Display for ExtendedHintEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyNotInExtendedRoster { key } => write!(
                f,
                "F.4.5 Table F.10: key {key:?} is not one of the three hint tables the extended \
                 generic format covers (V interactive form / C logical structure / R renditions \
                 name tree; 35781-35785)"
            ),
            Self::IdWidthOutOfRange { bits } => write!(
                f,
                "F.4.5 Table F.10 item 6: a bits-needed width must be in 0..=32 \
                 (35524-35525, applied to item 6 by the same wording and the same 16-bit \
                 storage), got {bits}"
            ),
            Self::ReferenceCountMismatch { declared, actual } => write!(
                f,
                "F.4.5 Table F.10: item 5 declares {declared} shared object reference(s) but the \
                 model carries {actual} identifier(s); the reader consumes exactly item 5 entries \
                 of item 7, so an inconsistent model cannot round-trip"
            ),
            Self::UnevidencedReferencesTooLarge { count, limit } => write!(
                f,
                "F.4.5 Table F.10 item 7: with item 6 = 0 the identifiers consume no bits and the \
                 bit budget cannot evidence the count, and the reader caps it at {limit}; \
                 refusing {count} keeps every written byte decodable"
            ),
            Self::Bit(problem) => write!(f, "{problem}"),
        }
    }
}

impl std::error::Error for ExtendedHintEncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bit(problem) => Some(problem),
            _ => None,
        }
    }
}

impl From<HintEncodeError> for ExtendedHintEncodeError {
    fn from(problem: HintEncodeError) -> Self {
        Self::Bit(problem)
    }
}

/// F.4.5 单个对象组的写端类型化输入模型：只承载**进位流**的七项。
///
/// - 项 1—4（各 32 位）与 F.4.4 的 Table F.9 逐字同形（35779—35780 的 shall）；
/// - 项 5（32 位）是项 7 列的声明长度，须与
///   [`shared_object_ids`](Self::shared_object_ids) 严格等长；
/// - 项 6（16 位）是项 7 每项的打包宽度，取 0—32；
/// - 项 7 起是逐个共享对象标识符，即 F.4.2 共享对象 hint 表的顺序下标（自 0 起，
///   35815—35817）——**是否真落进那张表的条目范围属调用方**：本表只有组内位宽，
///   没有任何一项声明共享对象表的条目数，故本模块无法（也不该猜）做该跨表校验，
///   读端在解码后按 [`SharedObjectHintTable`](crate::core::reader::linearization::hints::SharedObjectHintTable)
///   给诊断。
///
/// [`key`](Self::key) 是本组来自哪个 Table F.2 键的身份标记（**不进位流**，只让
/// 往返断言与调用方的按键取组对上号）；读端的 `start_byte`/`bits_consumed` 是解析
/// 侧游标，同理不进模型——前者由 Table F.2 键的值给出，后者由 [`Self::total_bits`]
/// 决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedHintTableInput {
    /// 本组来自哪个 Table F.2 键：`V`（交互表单）/`C`（逻辑结构）/`R`（影片名称树）。
    pub key: &'static str,
    /// 项 1（32 位，35808）：组内第一个对象的对象号。
    pub first_object_number: u64,
    /// 项 2（32 位，35809）：组内第一个对象的位置（hint 表空间的文件偏移，
    /// 按 F.4 前言 NOTE 3 解读）。
    pub first_object_location: u64,
    /// 项 3（32 位，35810）：组内对象数（0 合法：F.4.5 无「组须至少一个对象」的
    /// shall，该措辞只在 F.4.6 的 35792—35793 出现）。
    pub object_count: u64,
    /// 项 4（32 位，35811）：对象组的长度（字节）。
    pub group_length: u64,
    /// 项 5（32 位，35812）：本组引用的共享对象数，须等于
    /// [`shared_object_ids`](Self::shared_object_ids) 的长度。
    pub shared_reference_count: u64,
    /// 项 6（16 位，35813—35814）：表示本组最大共享对象标识符所需的位数（0—32），
    /// 同时是项 7 每项的打包宽度。
    pub shared_object_id_bits: u32,
    /// 项 7 起（35815—35817）：逐个共享对象标识符，每项按
    /// [`shared_object_id_bits`](Self::shared_object_id_bits) 位打包。
    pub shared_object_ids: Vec<u64>,
}

impl ExtendedHintTableInput {
    /// 项 7 整列占用的位数（列长 × 项 6，saturating；校验通过后列长即项 5）。
    pub fn identifier_bits(&self) -> u64 {
        u64::from(self.shared_object_id_bits).saturating_mul(self.shared_object_ids.len() as u64)
    }

    /// 本表的总位数：项 1—6 恒 176 位，项 7 起按列长追加。读端解析成功时给出的
    /// `bits_consumed` 须等于它。
    pub fn total_bits(&self) -> u64 {
        HEADER_BITS.saturating_add(self.identifier_bits())
    }

    /// 本表的字节长度（读端 [`end_byte`](crate::core::reader::linearization::hints::ExtendedHintTable::end_byte)
    /// 减去起点后的同一 `div_ceil` 口径）：F.4 前言要求各表以字节边界开始（35482—35483），
    /// 故写端把尾部残位补零，本值即下一张表的最小合法位置。
    pub fn byte_len(&self) -> u64 {
        self.total_bits().div_ceil(8)
    }
}

/// 把 [`ExtendedHintTableInput`] 编码成从字节边界开始、可按 F.4.5（Table F.10）
/// 解析的位流字节。
///
/// 编码顺序严格镜像读端 `ExtendedHintTable::parse`：项 1 → 项 2 → 项 3 → 项 4 →
/// 项 5（各 32 位）→ 项 6（16 位）→ 项 7 起逐项按项 6 的宽度连续成列。所有自洽
/// 校验（名册键、项 6 的 0—32、项 5 与向量长度、宽度 0 的列长上限）都在写出任何
/// 位之前完成；给定宽度下的数值溢出由 [`HintBitWriter::unsigned`] 逐字段拒绝，
/// 且 `Result::Err` 时不返回任何字节——绝不落半成品。产物可直接交回
/// `ExtendedHintTable::parse(&bytes, 0, input.key)` 解出、七项逐一相等（含
/// `bits_consumed`）。
pub fn encode_extended_table(
    input: &ExtendedHintTableInput,
) -> Result<Vec<u8>, ExtendedHintEncodeError> {
    validate(input)?;
    let mut writer = HintBitWriter::new();
    write_header(&mut writer, input)?;
    write_identifier_column(&mut writer, input)?;
    Ok(writer.finish())
}

/// 写出前的全量自洽校验：名册键、项 6 的 0—32、项 5 与向量长度、宽度 0 时的
/// 读端列长上限。
fn validate(input: &ExtendedHintTableInput) -> Result<(), ExtendedHintEncodeError> {
    if !EXTENDED_HINT_KEYS.contains(&input.key) {
        return Err(ExtendedHintEncodeError::KeyNotInExtendedRoster { key: input.key });
    }
    if input.shared_object_id_bits > MAX_DECLARED_WIDTH {
        return Err(ExtendedHintEncodeError::IdWidthOutOfRange {
            bits: u64::from(input.shared_object_id_bits),
        });
    }
    // 项 5 是不可信形态的 32 位计数：模型里它必须已经与向量等长（`usize::try_from`
    // 失败即不等长，绝不按原值分配或写入）。
    let declared = usize::try_from(input.shared_reference_count);
    if declared != Ok(input.shared_object_ids.len()) {
        return Err(ExtendedHintEncodeError::ReferenceCountMismatch {
            declared: input.shared_reference_count,
            actual: input.shared_object_ids.len(),
        });
    }
    // 读端位预算门的对偶：宽度 0 时列不占位，条数不得超过读端的绝对上限。
    if input.shared_object_id_bits == 0
        && input.shared_object_ids.len() as u64 > MAX_UNEVIDENCED_ITEMS
    {
        return Err(ExtendedHintEncodeError::UnevidencedReferencesTooLarge {
            count: input.shared_object_ids.len() as u64,
            limit: MAX_UNEVIDENCED_ITEMS,
        });
    }
    Ok(())
}

/// Table F.10 项 1—6 的定宽头部：32/32/32/32/32/16（35808—35814），项序与宽度
/// 严格镜像读端 `parse` 的结构体字面量。
fn write_header(
    w: &mut HintBitWriter,
    input: &ExtendedHintTableInput,
) -> Result<(), ExtendedHintEncodeError> {
    w.unsigned(input.first_object_number, TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(input.first_object_location, TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(input.object_count, TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(input.group_length, TABLE_F10_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(
        input.shared_reference_count,
        TABLE_F10_FIXED_ITEM_WIDTH_BITS,
    )?;
    w.unsigned(
        u64::from(input.shared_object_id_bits),
        TABLE_F10_WIDTH_FIELD_BITS,
    )?;
    Ok(())
}

/// 项 7 起的标识符列：一项接一项、每项按项 6 声明的宽度连续写出（35815—35817
/// 「Starting with item 7, each of the remaining items in this table shall be a
/// shared object identifier」——原文没有将该项分组或对齐字节，故列就是列）。
fn write_identifier_column(
    w: &mut HintBitWriter,
    input: &ExtendedHintTableInput,
) -> Result<(), ExtendedHintEncodeError> {
    for &id in &input.shared_object_ids {
        w.unsigned(id, input.shared_object_id_bits)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
