//! 页偏移 hint 表（ISO 32000-1 F.4.1，Table F.3 头部 13 项 + Table F.4 逐页
//! 条目）的**写端**编码：把类型化输入模型 [`PageOffsetTableInput`] 逐位还原成
//! F.4 前言「高位在前」的字节流，与既有读端 [`PageOffsetHintTable::parse`] 精确对偶。
//!
//! 规范锚点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 输出行，沿用读端
//! `src/core/reader/linearization/hints/page_offset.rs` 已锚定的同一批）：
//! - 结构（35508—35509）：「This table shall begin with a header section,
//!   described in Table F.3, followed by one or more per-page entries, described
//!   in Table F.4」——页向量非空是 shall，空模型直接拒绝；
//! - 交错序 a)—g)（35514—35523）：项 1—3 与项 6—7 各是「all pages, in page
//!   order starting with the first page」的一整列；项 4/5 只对第二页起的共享
//!   对象逐页成列（「Item 4 for all shared objects in the second page, followed
//!   by item 4 for all shared objects in the third page, and so on」）——编码序
//!   严格镜像读端 `parse` 的列序：a→b→c→d→e→f→g；
//! - 位宽范围（35524—35525）：「All the items in Table F.3 that specify a number
//!   of bits needed, such as item 3, have values in the range 0 through 32.
//!   Although that range requires only 6 bits, 16-bit numbers shall be used」——
//!   七个声明宽度（项 3/5/7/9/10/11/12）越界即 [`HintEncodeError::
//!   DeclaredWidthOutOfRange`]（conforming writer 按 shall 拒绝；读端对越界值是
//!   宽容继续解析 + `width_problems` 上报，属**待核**取舍，本模块不跟随宽容）。
//!   声明值本身按 16 位写出；
//! - Table F.3 头部定宽 32/32/16/32/16/32/16/32/16/16/16/16/16（35526—35559），
//!   合计 5×32 + 8×16 = 288 位 = 36 字节整；项 13 是**分母值**（非位宽），受
//!   16 位字段约束，≥ 2^16 由 [`HintBitWriter::unsigned`] 拒绝；
//! - Table F.4 项 3 的 shall（35579—35581）：「For the first page, this number
//!   shall be 0; the next two items start with the second page」——首页声明非零
//!   或携带项 4/5 元素即 [`HintEncodeError::FirstPageSharedRefs`]；第二页起项
//!   4/5 的元素数须与本页项 3 严格相等，否则 [`HintEncodeError::
//!   SharedReferenceSpreadMismatch`]（读端按头部计数逐页重建向量，不符无从往返）；
//! - 增量语义：项 1/2/6/7 存**增量**（加头部项 1/4/6/8 得绝对值，见读端
//!   `PageOffsetEntry::object_count` 等访问器）——模型字段与读端类型化条目同形，
//!   本模块**只编码增量，不做累加**；跨页累加（对象号链、页位置链、分数位置
//!   类别）是读端 `page_offset_derive.rs` 的派生口径，写端不重复实现，由
//!   集成测试验证「编码→解码→派生值逐项等于模型」。
//!
//! **真实文件偏移属调用方**：Table F.3 项 2（首页页对象位置）与各页位置/长度
//! 增量都是**文件级**数字，本切片不做 F.3.1 分部布局、不算 `/L`/`/H`/`/T`/`/E`
//! 偏移——它们全部是模型里的 caller-supplied 字段，由后续装配切片给定。
//!
//! **读端预算门的对偶**：读端对「列宽度全 0、位预算无法背书计数」的形态设
//! `MAX_UNEVIDENCED_ITEMS`（65 536）绝对上限（页三项 3/5/10 全 0、或项 11/12
//! 全 0 时生效）。写出必可解是本模块的契约，故同口径提前拒绝
//! （[`HintEncodeError::UnevidencedItemsTooLarge`]，属库策略同形，非规范条款）。

use crate::core::reader::linearization::hints::{PageOffsetEntry, PageOffsetHeader};

use super::super::bits::{HintBitWriter, HintEncodeError};

/// Table F.3 声明「number of bits needed」项的原文上限（35524—35525 的 0—32）。
/// 与读端 `page_offset.rs::MAX_DECLARED_WIDTH` 同值。
const MAX_F3_DECLARED_WIDTH: u32 = 32;

/// 列宽度全 0 时读端对声明计数的绝对上限（读端 `MAX_UNEVIDENCED_ITEMS` 同值）。
const MAX_UNEVIDENCED_ITEMS: u64 = 65_536;

/// Table F.3 头部 13 项的总位数（5×32 + 8×16 = 288，35526—35559）。
const HEADER_BITS: u64 = 288;

/// F.4.1 页偏移 hint 表的写端类型化输入模型：只承载**进位流**的值。
///
/// - [`header`](Self::header)：Table F.3 十三项，定宽 32/32/16/32/16/32/16/32/
///   16/16/16/16/16；
/// - [`pages`](Self::pages)：Table F.4 逐页条目（复用读端 [`PageOffsetEntry`]，
///   字段即位流增量列与项 4/5 向量）。页数由 `pages.len()` 给出——读端按
///   Table F.1 `/N` 成列，真实装配切片须保证 `/N == pages.len()`，本模块无法
///   也不该猜字典内容。
///
/// 解析侧元数据（读端 `declared_pages`/`bits_consumed`）不进模型：前者即
/// `pages.len()`，后者由 [`Self::total_bits`] 派生。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOffsetTableInput {
    /// Table F.3 头部十三项。
    pub header: PageOffsetHeader,
    /// Table F.4 逐页条目（首页在前；至少一条，shall 35508—35509）。
    pub pages: Vec<PageOffsetEntry>,
}

impl PageOffsetTableInput {
    /// 交错列 a)/b)/c)/f)/g) 每页占用的位数（声明宽度之和，saturating）。
    pub fn bits_per_page(&self) -> u64 {
        [
            self.header.object_count_bits,
            self.header.page_length_bits,
            self.header.shared_ref_count_bits,
            self.header.content_offset_bits,
            self.header.content_length_bits,
        ]
        .into_iter()
        .fold(0u64, |sum, bits| sum.saturating_add(u64::from(bits)))
    }

    /// 读端 `fits_per_page` 的同一口径：a)/b)/c) 三列宽度之和（全 0 时页数
    /// 失去位预算背书，触发 [`MAX_UNEVIDENCED_ITEMS`] 上限）。
    fn budget_page_bits(&self) -> u64 {
        u64::from(self.header.object_count_bits)
            .saturating_add(u64::from(self.header.page_length_bits))
            .saturating_add(u64::from(self.header.shared_ref_count_bits))
    }

    /// 交错列 d)/e) 每个共享对象引用占用的位数（头部项 11 + 项 12）。
    pub fn bits_per_shared_reference(&self) -> u64 {
        u64::from(self.header.shared_object_id_bits)
            .saturating_add(u64::from(self.header.shared_position_numerator_bits))
    }

    /// 第二页起的共享对象引用总数（首页按 shall 是 0，不计数，35579—35581）。
    pub fn shared_reference_total(&self) -> u64 {
        self.pages.iter().skip(1).fold(0u64, |sum, page| {
            sum.saturating_add(page.shared_reference_count)
        })
    }

    /// 本表总位数（读端解析出的 `bits_consumed` 须等于它）。
    pub fn total_bits(&self) -> u64 {
        HEADER_BITS
            .saturating_add(self.bits_per_page().saturating_mul(self.pages.len() as u64))
            .saturating_add(
                self.bits_per_shared_reference()
                    .saturating_mul(self.shared_reference_total()),
            )
    }

    /// 本表末尾之后的第一个字节（F.4 前言「每表以字节边界开始」，写端补零后的
    /// 实际字节长；与读端 `PageOffsetHintTable::end_byte` 同口径 `div_ceil`）。
    pub fn end_byte(&self) -> u64 {
        self.total_bits().div_ceil(8)
    }
}

/// 把 [`PageOffsetTableInput`] 编码成从字节边界开始、可按 F.4.1 解析的位流字节。
///
/// 编码顺序严格镜像读端 [`PageOffsetHintTable::parse`](crate::core::reader::linearization::hints::PageOffsetHintTable::parse)：
/// Table F.3 头部 13 项，随后按 F.4.1 a)—g) 的项交错序写七列（项 4/5 自第二页
/// 起逐页成列）。所有自洽校验（声明宽度、首页 shall、计数与向量长度、位预算
/// 上限）都在写出任何位之前完成；给定宽度下的数值溢出由 [`HintBitWriter::
/// unsigned`] 逐字段拒绝，且 `Result::Err` 时不产出任何字节——绝不落半成品。
pub fn encode_page_offset_table(input: &PageOffsetTableInput) -> Result<Vec<u8>, HintEncodeError> {
    validate(input)?;
    let mut writer = HintBitWriter::new();
    write_header(&mut writer, &input.header)?;
    write_columns(&mut writer, &input.header, &input.pages)?;
    Ok(writer.finish())
}

/// 写出前的全量自洽校验：宽度范围、页向量非空、首页 shall、逐页计数与向量
/// 长度自洽、位预算上限。
fn validate(input: &PageOffsetTableInput) -> Result<(), HintEncodeError> {
    let header = &input.header;
    for (item, bits) in [
        ("item 3", header.object_count_bits),
        ("item 5", header.page_length_bits),
        ("item 7", header.content_offset_bits),
        ("item 9", header.content_length_bits),
        ("item 10", header.shared_ref_count_bits),
        ("item 11", header.shared_object_id_bits),
        ("item 12", header.shared_position_numerator_bits),
    ] {
        if bits > MAX_F3_DECLARED_WIDTH {
            return Err(HintEncodeError::DeclaredWidthOutOfRange {
                item,
                bits: u64::from(bits),
            });
        }
    }
    if input.pages.is_empty() {
        return Err(HintEncodeError::PageEntriesMissing);
    }
    let first = &input.pages[0];
    if first.shared_reference_count != 0
        || !first.shared_object_ids.is_empty()
        || !first.shared_position_numerators.is_empty()
    {
        return Err(HintEncodeError::FirstPageSharedRefs {
            count: first.shared_reference_count,
            ids: first.shared_object_ids.len(),
            numerators: first.shared_position_numerators.len(),
        });
    }
    for (page, entry) in input.pages.iter().enumerate().skip(1) {
        let ids = entry.shared_object_ids.len();
        let numerators = entry.shared_position_numerators.len();
        let consistent = usize::try_from(entry.shared_reference_count)
            .map(|declared| declared == ids && ids == numerators)
            .unwrap_or(false);
        if !consistent {
            return Err(HintEncodeError::SharedReferenceSpreadMismatch {
                page,
                declared: entry.shared_reference_count,
                ids,
                numerators,
            });
        }
    }
    // 读端位预算门的对偶：宽度全 0 时相应计数不得超过 65 536，否则读端拒绝、
    // 「写出必可解」的契约被破坏。
    if input.budget_page_bits() == 0 && input.pages.len() as u64 > MAX_UNEVIDENCED_ITEMS {
        return Err(HintEncodeError::UnevidencedItemsTooLarge {
            kind: "F.4.1 Table F.4 per-page entries",
            count: input.pages.len() as u64,
            limit: MAX_UNEVIDENCED_ITEMS,
        });
    }
    let references = input.shared_reference_total();
    if input.bits_per_shared_reference() == 0 && references > MAX_UNEVIDENCED_ITEMS {
        return Err(HintEncodeError::UnevidencedItemsTooLarge {
            kind: "F.4.1 Table F.4 items 4/5 shared references",
            count: references,
            limit: MAX_UNEVIDENCED_ITEMS,
        });
    }
    Ok(())
}

/// Table F.3 头部：32/32/16/32/16/32/16/32/16/16/16/16/16（35526—35559），
/// 项序与宽度严格镜像读端 `parse` 的结构体字面量。
fn write_header(w: &mut HintBitWriter, h: &PageOffsetHeader) -> Result<(), HintEncodeError> {
    w.unsigned(h.least_objects_per_page, 32)?;
    w.unsigned(h.first_page_object_location, 32)?;
    w.unsigned(u64::from(h.object_count_bits), 16)?;
    w.unsigned(h.least_page_length, 32)?;
    w.unsigned(u64::from(h.page_length_bits), 16)?;
    w.unsigned(h.least_content_offset, 32)?;
    w.unsigned(u64::from(h.content_offset_bits), 16)?;
    w.unsigned(h.least_content_length, 32)?;
    w.unsigned(u64::from(h.content_length_bits), 16)?;
    w.unsigned(u64::from(h.shared_ref_count_bits), 16)?;
    w.unsigned(u64::from(h.shared_object_id_bits), 16)?;
    w.unsigned(u64::from(h.shared_position_numerator_bits), 16)?;
    w.unsigned(h.shared_position_denominator, 16)?;
    Ok(())
}

/// Table F.4 的七列，按 F.4.1 a)—g) 的项交错序（列主序，页序在列内）：
/// a) 项 1 增量 → b) 项 2 增量 → c) 项 3 计数 → d) 项 4 标识扁平列（自第二页
/// 起逐页展开）→ e) 项 5 分子扁平列 → f) 项 6 增量 → g) 项 7 增量。
fn write_columns(
    w: &mut HintBitWriter,
    h: &PageOffsetHeader,
    pages: &[PageOffsetEntry],
) -> Result<(), HintEncodeError> {
    // a) 项 1：对象数增量列（宽度 = 头部项 3）。
    for page in pages {
        w.unsigned(page.object_count_delta, h.object_count_bits)?;
    }
    // b) 项 2：页长增量列（宽度 = 头部项 5）。
    for page in pages {
        w.unsigned(page.page_length_delta, h.page_length_bits)?;
    }
    // c) 项 3：共享对象引用数列（宽度 = 头部项 10；首页恒 0，已校验）。
    for page in pages {
        w.unsigned(page.shared_reference_count, h.shared_ref_count_bits)?;
    }
    // d) 项 4：逐页共享对象标识扁平列（宽度 = 头部项 11）。
    for page in pages.iter().skip(1) {
        for &id in &page.shared_object_ids {
            w.unsigned(id, h.shared_object_id_bits)?;
        }
    }
    // e) 项 5：与项 4 同序的分数位置分子扁平列（宽度 = 头部项 12）。
    for page in pages.iter().skip(1) {
        for &numerator in &page.shared_position_numerators {
            w.unsigned(numerator, h.shared_position_numerator_bits)?;
        }
    }
    // f) 项 6：内容流起点偏移增量列（宽度 = 头部项 7）。
    for page in pages {
        w.unsigned(page.content_offset_delta, h.content_offset_bits)?;
    }
    // g) 项 7：内容流长度增量列（宽度 = 头部项 9）。
    for page in pages {
        w.unsigned(page.content_length_delta, h.content_length_bits)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
