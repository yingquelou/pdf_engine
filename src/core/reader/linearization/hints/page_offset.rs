//! 页偏移 hint 表（ISO 32000-1 F.4.1，Table F.3 头部 + Table F.4 逐页条目）。
//!
//! 选择该家族的理由：F.4 前言规定「a conforming writer shall include a page
//! offset hint table, which shall be the first table in the stream and shall
//! start at offset 0」（35477—35478），它是**唯一没有 Table F.2 键、恒在流
//! 偏移 0** 的必选表，也是 F.4.2 共享对象 hint 表的前置依赖——Table F.6 项 1
//! 明说首页第一个对象的位置「shall be given in the page offset hint table,
//! header section (Table F.3, item 4)」（35678—35680），Table F.4 项 4 又按
//! 序号引用共享对象 hint 表的条目（35582—35584）。上一切片只把它当占位字节
//! 跳过，故它是依赖序上的下一步。
//!
//! 原文要点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 的输出行）：
//! - 结构：「This table shall begin with a header section, described in
//!   Table F.3, followed by one or more per-page entries, described in
//!   Table F.4」（35508—35509）。
//! - Table F.3 头部 13 项，定宽 32/32/16/32/16/32/16/32/16/16/16/16/16
//!   （35526—35559）。项 13 是**分母值**而非位宽，故不受 35524—35525 的
//!   「values in the range 0 through 32」约束。
//! - 位宽范围：「All the items in Table F.3 that specify a number of bits
//!   needed, such as item 3, have values in the range 0 through 32. Although
//!   that range requires only 6 bits, 16-bit numbers shall be used」
//!   （35524—35525）。原文未规定越界读法，故本模块照声明值继续读取、由
//!   [`PageOffsetHintTable::width_problems`] 如实上报（属**待核**取舍）；
//!   声明宽度大于 64 位原语上限时才由 [`BitReader`] 报
//!   [`HintBitError::InvalidWidth`]。
//! - 逐页条目**不连续**，按 F.4.1 a)—g) 的项交错序（35514—35523；NOTE
//!   35510—35511）：项 1—3 与
//!   项 6—7 各是「all pages, in page order starting with the first page」一列；
//!   项 4、项 5 只对第二页起的共享对象成列（「Item 4 for all shared objects in
//!   the second page, followed by item 4 for all shared objects in the third
//!   page, and so on」）。
//! - Table F.4 项 3：「For the first page, this number shall be 0; the next two
//!   items start with the second page」（35579—35581）。首页报非零即 shall
//!   违规：原值保留在 [`PageOffsetEntry::shared_reference_count`]，但项 4/5 的
//!   列里不含它，由 [`super`] 落类型化诊断。
//! - 页数取自 Table F.1 `/N`（35236「The number of pages in the document」），
//!   由 [`super`] 传入；`/N` 不可用时整个家族跳过，不叠加假阳性。
//!
//! 增量语义：项 1/2/6/7 存的是**增量**，须分别加 Table F.3 项 1/4/6/8 的最小值，
//! 见 [`PageOffsetEntry::object_count`] 等类型化访问器（一律 saturating，绝不因
//! 不可信位流溢出 panic）。表内位置若为文件绝对偏移，仍按 F.4 前言 NOTE 3 用
//! [`super::LinearizationHints::adjust_position`] 修正（Table F.4 项 2「A
//! conforming product shall skip over the primary hint stream」，35577—35578）。

use super::bits::{BitReader, HintBitError};

/// 原文允许声明的最大位宽（35524—35525 的 0—32）。
const MAX_DECLARED_WIDTH: u32 = 32;

/// 不可信计数在「该项不占位、位流里没有任何页数/引用数证据」时的分配上限。
///
/// 页宽三项（Table F.3 项 3/5/10）与共享对象两列位宽（项 11/12）全为 0 时，
/// 对应条目不消费任何位，位预算无法再为声明的条数背书（`/N` 与项 3 都来自
/// 不可信输入），故按本上限拒绝超出者——属库策略而非规范条款（**待核**：原文
/// 只规定页数取 `/N`、引用数取 Table F.4 项 3，未给上限）。上限之下的文档
/// （6.5 万页）按类型化错误处理，绝不无限分配或死循环。
const MAX_UNEVIDENCED_ITEMS: u64 = 65_536;

/// Table F.3：页偏移 hint 表头部（13 项，定宽）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageOffsetHeader {
    /// 项 1（32 位）：一页（含页对象自身）的最少对象数。
    pub least_objects_per_page: u64,
    /// 项 2（32 位）：首页页对象的位置。
    pub first_page_object_location: u64,
    /// 项 3（16 位）：表示每页对象数最大差值所需的位数（Table F.4 项 1 宽度）。
    pub object_count_bits: u32,
    /// 项 4（32 位）：一页的最小长度（字节）。
    pub least_page_length: u64,
    /// 项 5（16 位）：表示页长最大差值所需的位数（Table F.4 项 2 宽度）。
    pub page_length_bits: u32,
    /// 项 6（32 位）：内容流起点相对页起点的偏移最小值。
    pub least_content_offset: u64,
    /// 项 7（16 位）：表示内容流起点偏移最大差值所需的位数（Table F.4 项 6 宽度）。
    pub content_offset_bits: u32,
    /// 项 8（32 位）：内容流长度的最小值。
    pub least_content_length: u64,
    /// 项 9（16 位）：表示内容流长度最大差值所需的位数（Table F.4 项 7 宽度）。
    pub content_length_bits: u32,
    /// 项 10（16 位）：表示一页引用的共享对象数最大值所需的位数（Table F.4 项 3 宽度）。
    pub shared_ref_count_bits: u32,
    /// 项 11（16 位）：表示页面用到的最大共享对象标识所需的位数（Table F.4 项 4 宽度）。
    pub shared_object_id_bits: u32,
    /// 项 12（16 位）：共享对象引用分数位置的分子位宽（Table F.4 项 5 宽度）。
    pub shared_position_numerator_bits: u32,
    /// 项 13（16 位）：分数位置的分母（**值**，全篇文档共用；非位宽）。
    pub shared_position_denominator: u64,
}

impl PageOffsetHeader {
    /// Table F.3 里声明「number of bits needed」的 7 项（项 3/5/7/9/10/11/12）。
    /// 项 13 是分母值，不属于该范围。
    fn bit_widths(&self) -> [(&'static str, u32); 7] {
        [
            ("item 3", self.object_count_bits),
            ("item 5", self.page_length_bits),
            ("item 7", self.content_offset_bits),
            ("item 9", self.content_length_bits),
            ("item 10", self.shared_ref_count_bits),
            ("item 11", self.shared_object_id_bits),
            ("item 12", self.shared_position_numerator_bits),
        ]
    }

    /// 每页至少消费的位数：项 1 + 项 2 + 项 3 的宽度。
    fn fixed_bits_per_page(&self) -> u64 {
        (self.object_count_bits as u64)
            .saturating_add(self.page_length_bits as u64)
            .saturating_add(self.shared_ref_count_bits as u64)
    }

    /// 项 4 + 项 5 每个共享对象引用消费的位数。
    fn bits_per_shared_reference(&self) -> u64 {
        (self.shared_object_id_bits as u64)
            .saturating_add(self.shared_position_numerator_bits as u64)
    }
}

/// Table F.4：一页的条目（7 项；项 4/5 为变长列）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOffsetEntry {
    /// 项 1：对象数增量；页对象数 = 本值 + 头部项 1。
    pub object_count_delta: u64,
    /// 项 2：页长增量；页长 = 本值 + 头部项 4。
    pub page_length_delta: u64,
    /// 项 3：本页引用的共享对象数（首页按 shall 须为 0）。
    pub shared_reference_count: u64,
    /// 项 4：逐个共享对象标识（＝共享对象 hint 表的条目下标，自 0 起）。
    pub shared_object_ids: Vec<u64>,
    /// 项 5：与项 4 同序的分数位置分子；分母是头部项 13。
    pub shared_position_numerators: Vec<u64>,
    /// 项 6：内容流起点偏移增量；偏移 = 本值 + 头部项 6（相对页起点）。
    pub content_offset_delta: u64,
    /// 项 7：内容流长度增量；长度 = 本值 + 头部项 8。
    pub content_length_delta: u64,
}

impl PageOffsetEntry {
    /// Table F.4 项 1 + Table F.3 项 1：本页对象数（含页对象自身）。
    pub fn object_count(&self, header: &PageOffsetHeader) -> u64 {
        header
            .least_objects_per_page
            .saturating_add(self.object_count_delta)
    }

    /// Table F.4 项 2 + Table F.3 项 4：本页长度（字节）。
    pub fn page_length(&self, header: &PageOffsetHeader) -> u64 {
        header
            .least_page_length
            .saturating_add(self.page_length_delta)
    }

    /// Table F.4 项 6 + Table F.3 项 6：内容流起点相对页起点的偏移。
    pub fn content_offset(&self, header: &PageOffsetHeader) -> u64 {
        header
            .least_content_offset
            .saturating_add(self.content_offset_delta)
    }

    /// Table F.4 项 7 + Table F.3 项 8：内容流长度（含流对象开销）。
    pub fn content_length(&self, header: &PageOffsetHeader) -> u64 {
        header
            .least_content_length
            .saturating_add(self.content_length_delta)
    }
}

/// Table F.4 的逐页条目集合（交错项按 a)—g) 归位后的形态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOffsetHintTable {
    /// Table F.3 头部。
    pub header: PageOffsetHeader,
    /// 解析所用的页数，来自 Table F.1 `/N`（原样保留，便于溯源）。
    pub declared_pages: u64,
    /// 每页一条，按页序（首页在前）。
    pub pages: Vec<PageOffsetEntry>,
    /// 本表消费掉的位数（自表首字节起）；F.4 前言的「表不重叠」核对据此折算
    /// 下一张表的最小位置。
    pub bits_consumed: u64,
}

impl PageOffsetHintTable {
    /// 从 `data`（主/溢出 hint 流解码拼接后的位流）的字节偏移 `start` 处解析。
    ///
    /// `pages` 是 Table F.1 `/N` 的页数。F.4 前言把本表钉死在流偏移 0，故
    /// [`super`] 恒以 `start = 0` 调用；参数保留是为了与 [`SharedObjectHintTable`](super::SharedObjectHintTable)
    /// 同形，并便于单元测试直接给定起点。
    pub fn parse(data: &[u8], start: usize, pages: u64) -> Result<Self, HintBitError> {
        if start > data.len() {
            return Err(HintBitError::ShortBuffer {
                needed_bits: 8,
                remaining_bits: 0,
                position_bits: start as u64 * 8,
            });
        }
        let mut reader = BitReader::new(&data[start..]);
        let header = PageOffsetHeader {
            least_objects_per_page: reader.unsigned(32)?,
            first_page_object_location: reader.unsigned(32)?,
            object_count_bits: width(reader.unsigned(16)?),
            least_page_length: reader.unsigned(32)?,
            page_length_bits: width(reader.unsigned(16)?),
            least_content_offset: reader.unsigned(32)?,
            content_offset_bits: width(reader.unsigned(16)?),
            least_content_length: reader.unsigned(32)?,
            content_length_bits: width(reader.unsigned(16)?),
            shared_ref_count_bits: width(reader.unsigned(16)?),
            shared_object_id_bits: width(reader.unsigned(16)?),
            shared_position_numerator_bits: width(reader.unsigned(16)?),
            shared_position_denominator: reader.unsigned(16)?,
        };
        // 页数由不可信的 `/N` 给出：先按位预算封顶，再进入任何分配。
        let count = usize::try_from(pages).ok();
        let Some(count) =
            count.filter(|count| fits_per_page(&header, *count, reader.remaining_bits()))
        else {
            return Err(HintBitError::ShortBuffer {
                needed_bits: header.fixed_bits_per_page().saturating_mul(pages),
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        };
        // a) 项 1 → b) 项 2 → c) 项 3：三列都是「all pages, in page order」。
        let object_deltas = column(&mut reader, count, header.object_count_bits)?;
        let length_deltas = column(&mut reader, count, header.page_length_bits)?;
        let shared_counts = column(&mut reader, count, header.shared_ref_count_bits)?;
        // d) 项 4 → e) 项 5：自第二页起逐页成列（首页的项 3 按 shall 是 0，
        // 其声明值即便非零也不占位，故求和时 skip(1)）。
        let shared_total = shared_counts
            .iter()
            .skip(1)
            .fold(0u64, |sum, count| sum.saturating_add(*count));
        if !fits_shared(&header, shared_total, reader.remaining_bits()) {
            return Err(HintBitError::ShortBuffer {
                needed_bits: header
                    .bits_per_shared_reference()
                    .saturating_mul(shared_total),
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        }
        let ids = spread(&mut reader, &shared_counts, header.shared_object_id_bits)?;
        let numerators = spread(
            &mut reader,
            &shared_counts,
            header.shared_position_numerator_bits,
        )?;
        // f) 项 6 → g) 项 7。
        let content_offsets = column(&mut reader, count, header.content_offset_bits)?;
        let content_lengths = column(&mut reader, count, header.content_length_bits)?;
        let bits_consumed = reader.position_bits();
        let mut entries = Vec::with_capacity(count);
        let mut cursor = 0usize;
        for index in 0..count {
            // 首页不贡献项 4/5 的元素（F.4.1 d)/e) 从第二页起）。
            let take = match index {
                0 => 0,
                _ => usize::try_from(shared_counts[index]).unwrap_or(usize::MAX),
            };
            let page_ids = slice(&ids, cursor, take);
            let page_numerators = slice(&numerators, cursor, take);
            cursor = cursor.saturating_add(take).min(ids.len());
            entries.push(PageOffsetEntry {
                object_count_delta: object_deltas[index],
                page_length_delta: length_deltas[index],
                shared_reference_count: shared_counts[index],
                shared_object_ids: page_ids,
                shared_position_numerators: page_numerators,
                content_offset_delta: content_offsets[index],
                content_length_delta: content_lengths[index],
            });
        }
        Ok(Self {
            header,
            declared_pages: pages,
            pages: entries,
            bits_consumed,
        })
    }

    /// 本表末尾之后的第一个字节（F.4 前言：表以字节边界开始且互不重叠，故
    /// Table F.2 声明的位置不得低于此值）。位数按 `div_ceil` 折算。
    pub fn end_byte(&self) -> u64 {
        self.bits_consumed.div_ceil(8)
    }

    /// Table F.3 声明位宽越出原文 0—32 范围的项（项名, 声明值）。
    pub fn width_problems(&self) -> Vec<(&'static str, u64)> {
        self.header
            .bit_widths()
            .iter()
            .filter(|(_, bits)| *bits > MAX_DECLARED_WIDTH)
            .map(|(item, bits)| (*item, *bits as u64))
            .collect()
    }
}

/// Table F.3 位宽项：16 位原值最大 65535，转 `u32` 无信息损失。
fn width(raw: u64) -> u32 {
    raw.min(u32::MAX as u64) as u32
}

/// 页数是否落在剩余位预算内。
///
/// 项 3/5/10 全为 0 时每页不占位，位预算提供不了页数证据，故改用
/// [`MAX_UNEVIDENCED_ITEMS`] 绝对上限（宁给类型化错误，绝不做不可界分配或
/// 零消耗死循环）。
fn fits_per_page(header: &PageOffsetHeader, pages: usize, remaining_bits: u64) -> bool {
    let each = header.fixed_bits_per_page();
    let Ok(count) = u64::try_from(pages) else {
        return false;
    };
    if each == 0 {
        return count <= MAX_UNEVIDENCED_ITEMS;
    }
    each.saturating_mul(count) <= remaining_bits
}

/// 项 4/5 两列的位预算；两列宽度同为 0 时改用 [`MAX_UNEVIDENCED_ITEMS`] 上限。
fn fits_shared(header: &PageOffsetHeader, total: u64, remaining_bits: u64) -> bool {
    let each = header.bits_per_shared_reference();
    if each == 0 {
        return total <= MAX_UNEVIDENCED_ITEMS;
    }
    each.saturating_mul(total) <= remaining_bits
}

/// 读取一列 `count` 个 `bits` 位宽的值（F.4.1 交错序里的一列）。
fn column(reader: &mut BitReader<'_>, count: usize, bits: u32) -> Result<Vec<u64>, HintBitError> {
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(reader.unsigned(bits)?);
    }
    Ok(values)
}

/// 共享对象列：逐页（自第二页起）按该项的项 3 个数读取，返回**扁平**序列，
/// 页与列的边界由 [`slice`] 按同样的计数复原。
fn spread(
    reader: &mut BitReader<'_>,
    shared_counts: &[u64],
    bits: u32,
) -> Result<Vec<u64>, HintBitError> {
    let mut values = Vec::new();
    for (index, count) in shared_counts.iter().enumerate() {
        if index == 0 {
            continue;
        }
        // 总位数已由 [`fits_shared`] 校验，故 `count` 之和在界内。
        let take = usize::try_from(*count).unwrap_or(0);
        for _ in 0..take {
            values.push(reader.unsigned(bits)?);
        }
    }
    Ok(values)
}

/// 取扁平列的 `take` 个元素，越界即截到列尾（不 panic）。
fn slice(values: &[u64], from: usize, take: usize) -> Vec<u64> {
    let from = from.min(values.len());
    let to = from.saturating_add(take).min(values.len());
    values[from..to].to_vec()
}
