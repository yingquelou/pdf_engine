//! 缩略图 hint 表（ISO 32000-1 F.4.3，Table F.7 头部 12 项 + Table F.8 逐页
//! 条目）的**写端**编码：把类型化输入模型 [`ThumbnailTableInput`] 逐位还原成
//! F.4 前言「高位在前」的字节流，与既有读端
//! [`ThumbnailHintTable::parse`](crate::core::reader::linearization::hints::ThumbnailHintTable::parse)
//! 精确对偶。
//!
//! 规范锚点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 输出行，沿用读端
//! `src/core/reader/linearization/hints/thumbnail.rs` 已锚定的同一批）：
//! - 位流形态（35481—35483）：「this byte stream shall be treated as a bit
//!   stream, high-order bit first … without regard to byte boundaries. However,
//!   each hint table shall begin at a byte boundary」——本模块按位追加、末尾补零，
//!   产物恒自字节边界开始，故可与相邻表字节相邻；
//! - 结构（35693—35694）：「The thumbnail hint table shall consist of a header
//!   section (Table F.7) followed by the thumbnails section, which shall include
//!   one or more per-page entries (Table F.8)」——条目向量非空是 shall，空模型
//!   直接拒绝（[`ThumbnailEncodeError::EntriesMissing`]，与 F.4.1 的
//!   `PageEntriesMissing` 同口径）；
//! - Table F.7 头部 12 项定宽 32/32/32/16/32/16/32/16/32/32/32/32 ＝ 336 位整
//!   （项 1—9 见 35717—35735，项 10—12 见 35760—35762；原文按页跨栏续排，故
//!   两张「Table F.7 … (continued)」块之间夹着 Table F.6 的续条目），项序与宽度
//!   严格镜像读端 `parse` 的结构体字面量；
//! - Table F.8 的列序（35740—35743）：「The order of items in Table F.8 is as
//!   follows: a) Item 1 for all pages, in page order starting with the first
//!   page　b) Item 2 …　c) Item 3 …」——项 1/2/3 各是**一整列**，条目内部并不
//!   连续（与 F.4.1 同一形态），编码序 a→b→c；三项宽度依次取 Table F.7 项
//!   4/8/6（35765、35768、35771 各写「See Table F.7, item …」）；
//! - 列长取 Table F.7 项 3「The number of pages that have thumbnail images」
//!   （35720）：模型里
//!   [`pages_with_thumbnails`](ThumbnailHeader::pages_with_thumbnails) 须与
//!   [`entries`](ThumbnailTableInput::entries) 严格等长，否则
//!   [`ThumbnailEncodeError::EntryCountMismatch`]——读端按项 3 决定读几个条目，
//!   成功解析的表其条目数恒等于项 3，计数与向量不符的模型无从逐项往返；
//! - 页号序（35695—35697）：「The entries shall be in page number order
//!   starting with page 0 … Thumbnail images may exist for some pages and not
//!   for others」——[`entries`](ThumbnailTableInput::entries) 是**有序**向量，
//!   首条目对应页号最小的那张缩略图（即便目录有非 0 的 `/OpenAction`）；项 1 的
//!   定义（35765—35767「how many pages without a thumbnail image lie between
//!   the previous entry's page and this page」）没有给出可判定的上下界 shall，
//!   故本模块不校验项 1 的取值，只校它装得进项 4 声明的宽度；
//! - 增量语义（35768—35773）：Table F.8 项 2 加头部项 7 得该页缩略图的对象数、
//!   项 3 加头部项 5 得其长度（字节）——模型字段与读端类型化条目同形，本模块
//!   **只编增量、不做累加**；
//!   [`object_count`](crate::core::reader::linearization::hints::ThumbnailEntry::object_count)、
//!   [`image_length`](crate::core::reader::linearization::hints::ThumbnailEntry::image_length)
//!   与 [`page_of`](crate::core::reader::linearization::hints::ThumbnailHintTable::page_of)
//!   都是读端**派生**访问器，本模块不复制，由测试验证「编码→解码→派生值逐项
//!   等于模型」；
//! - 位宽范围（35524—35525）：「All the items in Table F.3 that specify a number
//!   of bits needed, such as item 3, have values in the range 0 through 32.
//!   Although that range requires only 6 bits, 16-bit numbers shall be used」——
//!   该句字面只约束 Table F.3；Table F.7 项 4/6/8（35721—35728）用的是**同一措辞**
//!   与同一 16 位存储约定，故与读端同一口径**类比**沿用 0—32 上限，越界即
//!   [`ThumbnailEncodeError::WidthOutOfRange`]（conforming writer 直接拒绝：三项
//!   宽度直接决定 Table F.8 三列的打包宽度，保留原值毫无意义）；声明值按 16 位
//!   写出。
//!
//! **待核**（一律沿用读端已落地的裁决，不发明第二种解读）：
//! - 0—32 的范围句字面只管 Table F.3，如上按类比外推到 Table F.7；读端对越界值
//!   是「原值保留 + [`width_problems`](crate::core::reader::linearization::hints::ThumbnailHintTable::width_problems)
//!   如实上报、仍按声明宽度继续读取」，写端按 shall 作 conforming writer 拒绝，
//!   故两侧只是**写端更严**：读端读得动的 33—64 位声明值写端不产出，读端读不动
//!   的 > 64 位（`HintBitError::InvalidWidth`）同样不产出——唯一共同约束是
//!   「写得出的一定读得回」；
//! - Table F.8 项 1 标注「(Optional)」（35765），原文未说明「省略」在位流里如何
//!   标示，而 35741 明确「Item 1 for all pages」且宽度由头部项 4 给出，读端按
//!   **每项恒在、声明宽度为 0 即消费 0 位**处理（与 F.4.1 的 0 宽字段同口径）——
//!   写端产出同一形态：项 4 为 0 时项 1 列一个位也不占、只能表示值 0；
//! - 项 4/6/8 全为 0 时条目不消费任何位、位预算给不出条目数的证据，读端对此类
//!   计数设 `MAX_UNEVIDENCED_ITEMS`（65 536）绝对上限并拒绝，本模块同口径提前
//!   拒绝（[`ThumbnailEncodeError::UnevidencedEntriesTooLarge`]，属**库策略**，
//!   非规范条款）。
//!
//! **键 `T` 与真实文件位置属调用方**：Table F.2 的 `T` 键「(Present only if
//! thumbnail images exist)」（35324—35325）是**条件存在**——文档没有缩略图时缺键
//! 合法，故本模块不承担「是否该写这张表」的判断，是否调用由调用方决定；产物在主
//! hint 流里的起点即 `T` 的值（F.4 前言 35476—35477），本切片不做多表装配、不算
//! `/L`/`/H`/`/T`/`/E` 的真实字节。项 2（35719）与项 10（35760）是文件偏移，仍受
//! NOTE 3（35490—35492「the resulting positions shall be interpreted as if the
//! primary hint stream itself were not present」）约束，模型里存的是 hint 表空间
//! 的值。项 9—12 声明的**缩略图共享对象区**（part 9 的子段，见 F.3.10
//! 「shall not be referenced from any other objects」35436—35437）与其余各项一样
//! **原样写出声明值**：F.4.3 没有规定这些值越界或自相矛盾时的写法，读端也只保留
//! 原值、不定位、不交叉核对，本模块同样不做。
//!
//! **校验先于任何位**：声明宽度范围、条目向量非空、项 3 与向量长度自洽、宽度全 0
//! 时的列长上限，全部在写出第一位之前完成；给定宽度下的数值溢出由
//! [`HintBitWriter::unsigned`] 逐字段拒绝，且 `Result::Err` 时不返回任何字节——
//! 绝不落半成品位流。
//!
//! **错误形态**：F.4 各家族共用的位级错误 [`HintEncodeError`] 住在
//! `src/core/writer/linearization/bits.rs`，本切片的写集不含该文件，故家族特有的
//! 拒绝（项 4/6/8 的范围、项 3 与向量自洽、条目缺失、宽度全 0 的列长上限）住在本
//! 模块的 [`ThumbnailEncodeError`]，位级拒绝原值经
//! [`ThumbnailEncodeError::Bit`] 透传（与 `extended/` 同一处理）。

use crate::core::reader::linearization::hints::{ThumbnailEntry, ThumbnailHeader};

use super::super::bits::{HintBitWriter, HintEncodeError};

/// Table F.7 里 9 个定宽项的宽度（位；35717—35735 与 35760—35762 的「32」列）。
const TABLE_F7_FIXED_ITEM_WIDTH_BITS: u32 = 32;

/// Table F.7 项 4/6/8 的存储宽度（位；35525 的「16-bit numbers shall be used」，
/// 与 Table F.7 三处「16」列同值）。
const TABLE_F7_WIDTH_FIELD_BITS: u32 = 16;

/// 头部 12 项的定宽合计（位）：9×32 + 3×16 = 336（35717—35735、35760—35762），
/// 即读端 `bits_consumed` 在项 4/6/8 全为 0 时的取值。
const HEADER_BITS: u64 = 336;

/// 声明位宽的期望上限（0—32，35524—35525 的类比外推；与读端
/// `MAX_DECLARED_WIDTH` 同值）。
const MAX_DECLARED_WIDTH: u32 = 32;

/// 项 4/6/8 全为 0 时读端对条目数的绝对上限（读端 `MAX_UNEVIDENCED_ITEMS` 同值，
/// 属**库策略**而非条款：原文只给「The number of pages that have thumbnail
/// images」35720，未给计数上限）。
const MAX_UNEVIDENCED_ITEMS: u64 = 65_536;

/// [`encode_thumbnail_table`] 的类型化拒绝，全部在写出第一位之前给出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbnailEncodeError {
    /// Table F.7 项 4/6/8 之一越出期望范围 0—32（35524—35525 的同一措辞类比，
    /// 35721—35728）。
    WidthOutOfRange {
        /// 越界的表内项名（与读端 `width_problems` 同名，便于对照）。
        item: &'static str,
        /// 模型声明的位宽。
        bits: u64,
    },
    /// [`entries`](ThumbnailTableInput::entries) 为空：35693—35694 的 shall 要
    /// 「one or more per-page entries」，只有头部的表是残缺表。
    EntriesMissing,
    /// Table F.7 项 3 的声明数与 [`entries`](ThumbnailTableInput::entries) 的
    /// 实际长度不符：读端按项 3（35720）决定 Table F.8 的列长，计数与向量不符
    /// 的模型写出的位流要么多读要么少读，无从逐项往返。
    EntryCountMismatch {
        /// 项 3 的声明值。
        declared: u64,
        /// 向量实际长度。
        actual: usize,
    },
    /// 项 4/6/8 全为 0 时每个条目不占位，位流给不出条目数的证据，读端对此类
    /// 计数设绝对上限并拒绝——写端同口径拒绝，保证写出的每个字节都解得开
    /// （**库策略**，非规范条款）。
    UnevidencedEntriesTooLarge {
        /// [`entries`](ThumbnailTableInput::entries) 的条数。
        count: u64,
        /// 与读端一致的绝对上限。
        limit: u64,
    },
    /// 位级拒绝（原值透传）：定宽字段装不下给定数值，如某个 32 位项 ≥ 2^32、
    /// 某个声明宽度 ≥ 2^16，或某个增量超出项 4/6/8 声明的列宽——读端会截断
    /// 高位，往返不再逐项相等。
    Bit(HintEncodeError),
}

impl std::fmt::Display for ThumbnailEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WidthOutOfRange { item, bits } => write!(
                f,
                "F.4.3 Table F.7 {item}: a bits-needed width must be in 0..=32 \
                 (35524-35525, applied to items 4/6/8 by the same wording and the same \
                 16-bit storage), got {bits}"
            ),
            Self::EntriesMissing => write!(
                f,
                "F.4.3: the thumbnails section shall include one or more per-page entries \
                 (35693-35694); the model carries none"
            ),
            Self::EntryCountMismatch { declared, actual } => write!(
                f,
                "F.4.3 Table F.7 item 3 declares {declared} page(s) with thumbnail images but \
                 the model carries {actual} per-page entr(y/ies); the reader consumes exactly \
                 item 3 entries of Table F.8, so an inconsistent model cannot round-trip"
            ),
            Self::UnevidencedEntriesTooLarge { count, limit } => write!(
                f,
                "F.4.3 Table F.8: with items 4/6/8 all 0 the entries consume no bits and the \
                 bit budget cannot evidence the count, and the reader caps it at {limit}; \
                 refusing {count} keeps every written byte decodable"
            ),
            Self::Bit(problem) => write!(f, "{problem}"),
        }
    }
}

impl std::error::Error for ThumbnailEncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bit(problem) => Some(problem),
            _ => None,
        }
    }
}

impl From<HintEncodeError> for ThumbnailEncodeError {
    fn from(problem: HintEncodeError) -> Self {
        Self::Bit(problem)
    }
}

/// F.4.3 缩略图 hint 表的写端类型化输入模型：只承载**进位流**的值。
///
/// - [`header`](Self::header)：Table F.7 十二项（复用读端类型），定宽
///   32/32/32/16/32/16/32/16/32/32/32/32；项 9—12 的缩略图共享对象区声明值原样
///   写出，本模块不定位、不核对；
/// - [`entries`](Self::entries)：Table F.8 逐页条目（复用读端
///   [`ThumbnailEntry`]，字段即三列增量），**按页号序**、首条目对应页号最小的
///   缩略图（35695—35697），至少一条（35693—35694 的 shall）。
///
/// 解析侧元数据（读端 `start_byte`/`declared_entries`/`bits_consumed`）不进模型：
/// 起点由 Table F.2 键 `T` 的值给出（属装配切片），项 3 即 `entries.len()`（须
/// 严格等长，见 [`ThumbnailEncodeError::EntryCountMismatch`]），位消耗由
/// [`Self::total_bits`] 决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailTableInput {
    /// Table F.7 头部十二项；项 3 须等于 [`entries`](Self::entries) 的长度。
    pub header: ThumbnailHeader,
    /// Table F.8 逐页条目（页号序；至少一条）。
    pub entries: Vec<ThumbnailEntry>,
}

impl ThumbnailTableInput {
    /// Table F.8 一个条目消费的位数（项 1/2/3 的声明宽度之和，saturating；
    /// 与读端 `ThumbnailHeader::bits_per_entry` 同口径）。
    pub fn bits_per_entry(&self) -> u64 {
        u64::from(self.header.gap_count_bits)
            .saturating_add(u64::from(self.header.object_count_bits))
            .saturating_add(u64::from(self.header.image_length_bits))
    }

    /// 本表总位数（读端解析成功时给出的 `bits_consumed` 须等于它）。
    pub fn total_bits(&self) -> u64 {
        HEADER_BITS.saturating_add(
            self.bits_per_entry()
                .saturating_mul(self.entries.len() as u64),
        )
    }

    /// 本表的字节长度：F.4 前言要求各表以字节边界开始（35481—35483），故写端把
    /// 尾部残位补零，本值即下一张表的最小合法位置（与读端
    /// [`end_byte`](crate::core::reader::linearization::hints::ThumbnailHintTable::end_byte)
    /// 同口径的 `div_ceil`）。
    pub fn byte_len(&self) -> u64 {
        self.total_bits().div_ceil(8)
    }
}

/// 把 [`ThumbnailTableInput`] 编码成从字节边界开始、可按 F.4.3（Table F.7/F.8）
/// 解析的位流字节。
///
/// 编码顺序严格镜像读端 `ThumbnailHintTable::parse`：Table F.7 头部 12 项（项序
/// 与宽度照表），随后按 F.4.3 a)—c) 的列序写 Table F.8 的三列（项 1 列 → 项 2 列
/// → 项 3 列，宽度依次取头部项 4/8/6）。所有自洽校验（声明宽度范围、条目非空、
/// 项 3 与向量长度、宽度全 0 的列长上限）都在写出任何位之前完成；给定宽度下的
/// 数值溢出由 [`HintBitWriter::unsigned`] 逐字段拒绝，且 `Result::Err` 时不返回
/// 任何字节——绝不落半成品。产物可直接交回
/// [`ThumbnailHintTable::parse`](crate::core::reader::linearization::hints::ThumbnailHintTable::parse)
/// 解出、逐项相等（含 `bits_consumed`）。
pub fn encode_thumbnail_table(
    input: &ThumbnailTableInput,
) -> Result<Vec<u8>, ThumbnailEncodeError> {
    validate(input)?;
    let mut writer = HintBitWriter::new();
    write_header(&mut writer, &input.header)?;
    write_columns(&mut writer, &input.header, &input.entries)?;
    Ok(writer.finish())
}

/// 写出前的全量自洽校验：项 4/6/8 的 0—32、条目向量非空、项 3 与向量长度、
/// 宽度全 0 时的读端列长上限。
fn validate(input: &ThumbnailTableInput) -> Result<(), ThumbnailEncodeError> {
    for (item, bits) in [
        ("item 4", input.header.gap_count_bits),
        ("item 6", input.header.image_length_bits),
        ("item 8", input.header.object_count_bits),
    ] {
        if bits > MAX_DECLARED_WIDTH {
            return Err(ThumbnailEncodeError::WidthOutOfRange {
                item,
                bits: u64::from(bits),
            });
        }
    }
    if input.entries.is_empty() {
        return Err(ThumbnailEncodeError::EntriesMissing);
    }
    // 项 3 是不可信形态的 32 位计数：模型里它必须已经与向量等长
    // （`usize::try_from` 失败即不等长，绝不按原值分配或写入）。
    let declared = usize::try_from(input.header.pages_with_thumbnails);
    if declared != Ok(input.entries.len()) {
        return Err(ThumbnailEncodeError::EntryCountMismatch {
            declared: input.header.pages_with_thumbnails,
            actual: input.entries.len(),
        });
    }
    // 读端位预算门的对偶：三项宽度全 0 时条目不占位，条数不得超过读端的绝对上限。
    if input.bits_per_entry() == 0 && input.entries.len() as u64 > MAX_UNEVIDENCED_ITEMS {
        return Err(ThumbnailEncodeError::UnevidencedEntriesTooLarge {
            count: input.entries.len() as u64,
            limit: MAX_UNEVIDENCED_ITEMS,
        });
    }
    Ok(())
}

/// Table F.7 头部 12 项：32/32/32/16/32/16/32/16/32/32/32/32（35717—35735、
/// 35760—35762），项序与宽度严格镜像读端 `parse` 的结构体字面量。
fn write_header(w: &mut HintBitWriter, h: &ThumbnailHeader) -> Result<(), ThumbnailEncodeError> {
    w.unsigned(
        h.first_thumbnail_object_number,
        TABLE_F7_FIXED_ITEM_WIDTH_BITS,
    )?;
    w.unsigned(
        h.first_thumbnail_object_location,
        TABLE_F7_FIXED_ITEM_WIDTH_BITS,
    )?;
    w.unsigned(h.pages_with_thumbnails, TABLE_F7_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(u64::from(h.gap_count_bits), TABLE_F7_WIDTH_FIELD_BITS)?;
    w.unsigned(h.least_image_length, TABLE_F7_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(u64::from(h.image_length_bits), TABLE_F7_WIDTH_FIELD_BITS)?;
    w.unsigned(h.least_object_count, TABLE_F7_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(u64::from(h.object_count_bits), TABLE_F7_WIDTH_FIELD_BITS)?;
    w.unsigned(h.first_shared_object_number, TABLE_F7_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(
        h.first_shared_object_location,
        TABLE_F7_FIXED_ITEM_WIDTH_BITS,
    )?;
    w.unsigned(h.shared_object_count, TABLE_F7_FIXED_ITEM_WIDTH_BITS)?;
    w.unsigned(h.shared_section_length, TABLE_F7_FIXED_ITEM_WIDTH_BITS)?;
    Ok(())
}

/// Table F.8 的三列，按 F.4.3 a)—c) 的列序（列主序，页序在列内）：
/// a) 项 1 前导无缩略图页数（宽度＝头部项 4）→ b) 项 2 对象数增量（头部项 8）
/// → c) 项 3 长度增量（头部项 6）。
fn write_columns(
    w: &mut HintBitWriter,
    h: &ThumbnailHeader,
    entries: &[ThumbnailEntry],
) -> Result<(), ThumbnailEncodeError> {
    // a) 项 1 列（宽度 = 头部项 4）。
    for entry in entries {
        w.unsigned(entry.preceding_pages_without_thumbnail, h.gap_count_bits)?;
    }
    // b) 项 2 列（宽度 = 头部项 8）。
    for entry in entries {
        w.unsigned(entry.object_count_delta, h.object_count_bits)?;
    }
    // c) 项 3 列（宽度 = 头部项 6）。
    for entry in entries {
        w.unsigned(entry.image_length_delta, h.image_length_bits)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
