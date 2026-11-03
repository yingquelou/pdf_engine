//! F.4.6 的位流读取与装配层接线（类型化模型在 [`super`]）。
//!
//! 本文件承担三件事，全部按「不可信输入只给类型化错误」的既有口径：
//! - [`EmbeddedFileHintTable::parse`]：Table F.11 头部（3×32 + 4×16 位）+
//!   Table F.12 的**项主序**五列（35797—35799），项 5 的位打包列表按组归位；
//! - 分配前的位预算封顶：组数（头部项 3）与项 5 的标识符总数都先过
//!   `count × width <= 剩余位`，宽度全为 0 时改用
//!   [`MAX_UNEVIDENCED_ITEMS`](super::super::page_offset::MAX_UNEVIDENCED_ITEMS)
//!   绝对上限——**绝不** `Vec::with_capacity(原始计数)`，也绝不零消耗死循环；
//! - [`parse_embedded_file`]：Table F.2 键 `B` 的分派入口（缺键静默、形态不符
//!   落类型化诊断），以及 F.4 前言「表以字节边界开始且互不重叠」的位置剔除
//!   （[`drop_positions_inside_embedded_file_table`]）。
//!
//! 位宽门与 `width`/`column` 原语一律复用 [`super::super::page_offset`]，位宽越出
//! 0—32 直接 [`HintBitError::InvalidWidth`]（裁决见 [`super`] 模块头）。

use crate::core::reader::linearization::issues::LinearizationIssue;

use super::super::bits::{BitReader, HintBitError};
use super::super::page_offset::{
    MAX_DECLARED_WIDTH, MAX_UNEVIDENCED_ITEMS, PageOffsetHintTable, column, width,
};
use super::super::{HintTablePosition, LinearizationHints};
use super::{
    EMBEDDED_FILE_HINT_KEY, EmbeddedFileGroupEntry, EmbeddedFileHeader, EmbeddedFileHintTable,
    HEADER_FIXED_ITEM_BITS, HEADER_WIDTH_FIELD_BITS,
};

/// 从 `data`（主/溢出 hint 流解码拼接后的位流）的字节偏移 `start` 处按
/// Table F.11/F.12 解析键 `B` 的内嵌文件流 hint 表。
///
/// `start` 由主 hint 流字典的 Table F.2 键 `B` 给出（F.3.6：「the dictionary of
/// the primary hint stream shall contain entries giving the position of the
/// beginning of each hint table in the stream」35313—35316，且**只**在主流字典
/// 里），F.4 前言另要求表以字节边界开始（35482—35483）。
///
/// `shared_object_id_bits` 是**文档级**的 Table F.3 项 11（Table F.12 项 5 的
/// 唯一宽度来源，见 [`super`] 模块头）；项 1—4 的宽度全部来自头部项 4—7。
impl EmbeddedFileHintTable {
    pub fn parse(
        data: &[u8],
        start: usize,
        shared_object_id_bits: u32,
    ) -> Result<Self, HintBitError> {
        if start > data.len() {
            return Err(HintBitError::ShortBuffer {
                needed_bits: 8,
                remaining_bits: 0,
                position_bits: start as u64 * 8,
            });
        }
        // 项 5 的宽度来自 Table F.3，而 35524—35525 的 0—32 **字面**就约束该表；
        // 列长与列宽都由它决定，故保留原值没有意义，越界即类型化拒绝。
        if shared_object_id_bits > MAX_DECLARED_WIDTH {
            return Err(HintBitError::InvalidWidth {
                bits: u64::from(shared_object_id_bits),
            });
        }
        let mut reader = BitReader::new(&data[start..]);
        // 头部恒 3×32 + 4×16 = 160 位：先按定宽给一个明确的 ShortBuffer，逐项读
        // 也会给，但这里把 Table F.11 的定宽写成可溯源的常量（0 组的最小合法表
        // 就是这 160 位）。
        if reader.remaining_bits() < EmbeddedFileHeader::HEADER_BITS {
            return Err(HintBitError::ShortBuffer {
                needed_bits: EmbeddedFileHeader::HEADER_BITS,
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        }
        let header = EmbeddedFileHeader {
            first_object_number: reader.unsigned(HEADER_FIXED_ITEM_BITS)?,
            first_object_location: reader.unsigned(HEADER_FIXED_ITEM_BITS)?,
            group_count: reader.unsigned(HEADER_FIXED_ITEM_BITS)?,
            object_number_bits: declared_width(reader.unsigned(HEADER_WIDTH_FIELD_BITS)?)?,
            group_object_bits: declared_width(reader.unsigned(HEADER_WIDTH_FIELD_BITS)?)?,
            group_length_bits: declared_width(reader.unsigned(HEADER_WIDTH_FIELD_BITS)?)?,
            shared_ref_count_bits: declared_width(reader.unsigned(HEADER_WIDTH_FIELD_BITS)?)?,
        };
        // 组数是 32 位不可信声明值：进入任何分配前先按位预算封顶（组数 0 是合法
        // 形态，只给空列表，不猜）。
        let declared_groups = header.group_count;
        let Some(count) = usize::try_from(declared_groups)
            .ok()
            .filter(|count| fits_groups(&header, *count, reader.remaining_bits()))
        else {
            return Err(HintBitError::ShortBuffer {
                needed_bits: header.bits_per_group().saturating_mul(declared_groups),
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        };
        // **项主序（转置）**：35797—35799 的「item 1 for the first group, item 1
        // for the second group, and so on …」——四列各是「所有组」的一整列，
        // 条目内部**不连续**（与 F.4.1/F.4.3 的交错列同形，与 F.4.2 的两序列
        // 交错同理）。
        let object_numbers = column(&mut reader, count, header.object_number_bits)?;
        let object_counts = column(&mut reader, count, header.group_object_bits)?;
        let group_lengths = column(&mut reader, count, header.group_length_bits)?;
        let shared_counts = column(&mut reader, count, header.shared_ref_count_bits)?;
        // 项 5（第 5 列）：逐组的位打包列表，总数＝各组项 4 之和；先按位预算封顶。
        let total_ids = shared_counts
            .iter()
            .fold(0u64, |sum, count| sum.saturating_add(*count));
        let Some(total) =
            bounded_identifier_count(total_ids, shared_object_id_bits, reader.remaining_bits())
        else {
            return Err(HintBitError::ShortBuffer {
                needed_bits: total_ids.saturating_mul(u64::from(shared_object_id_bits)),
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        };
        let identifiers = column(&mut reader, total, shared_object_id_bits)?;
        let bits_consumed = reader.position_bits();
        // 扁平列表按同样的组项 4 计数归位（口径同页偏移表的 `spread`/`slice`）。
        let mut groups = Vec::with_capacity(count);
        let mut cursor = 0usize;
        for index in 0..count {
            let take = usize::try_from(shared_counts[index]).unwrap_or(usize::MAX);
            let shared_object_ids = take_slice(&identifiers, cursor, take);
            groups.push(EmbeddedFileGroupEntry {
                object_number: object_numbers[index],
                object_count: object_counts[index],
                group_length: group_lengths[index],
                shared_reference_count: shared_counts[index],
                shared_object_ids,
            });
            cursor = cursor.saturating_add(take).min(identifiers.len());
        }
        Ok(Self {
            header,
            groups,
            shared_object_id_bits,
            start_byte: start as u64,
            bits_consumed,
        })
    }
}

/// Table F.11 的 16 位声明位宽：越出 0—32（35524—35525）即类型化拒绝。16 位
/// 原值最大 65535，转 `u32` 无信息损失（收敛复用 [`width`]，同
/// [`super::super::thumbnail`] 的 16 位「number of bits needed」字段）。
fn declared_width(raw: u64) -> Result<u32, HintBitError> {
    let bits = width(raw);
    if bits > MAX_DECLARED_WIDTH {
        return Err(HintBitError::InvalidWidth {
            bits: u64::from(bits),
        });
    }
    Ok(bits)
}

/// 组数是否落在剩余位预算内（按条目项 1—4 的**下界**宽度折算；项 5 的列长
/// 上限另由 [`bounded_identifier_count`] 把关）。
///
/// 头部项 4—7 全为 0 时条目 1—4 不占位，位预算提供不了组数证据，故改用
/// [`MAX_UNEVIDENCED_ITEMS`] 绝对上限（宁给类型化错误，绝不做不可界分配或
/// 零消耗死循环）。
fn fits_groups(header: &EmbeddedFileHeader, groups: usize, remaining_bits: u64) -> bool {
    let each = header.bits_per_group();
    let Ok(count) = u64::try_from(groups) else {
        return false;
    };
    if each == 0 {
        return count <= MAX_UNEVIDENCED_ITEMS;
    }
    each.saturating_mul(count) <= remaining_bits
}

/// 项 5 的标识符总数：先按剩余位数界定，再分配（口径同
/// [`super::super::extended`] 的 `bounded_identifier_count`，两者都是乘法比较，
/// 避免除法与宽度 0 的除零）。
///
/// 每组项 4 的宽度由头部项 7 给出（最大 32 位），故总数可达 ~2^48；文档级宽度
/// 为 0 时列不占位、位预算给不出条数证据，改用 [`MAX_UNEVIDENCED_ITEMS`] 上限。
fn bounded_identifier_count(declared: u64, bits: u32, remaining_bits: u64) -> Option<usize> {
    let count = usize::try_from(declared).ok()?;
    if bits == 0 {
        return (declared <= MAX_UNEVIDENCED_ITEMS).then_some(count);
    }
    (u64::from(bits).saturating_mul(count as u64) <= remaining_bits).then_some(count)
}

/// 取扁平列的 `take` 个元素，越界即截到列尾（不 panic；口径同页偏移表私有的
/// 同名 `slice`，那里未对兄弟模块开放，故本模块同形实现）。
fn take_slice(values: &[u64], from: usize, take: usize) -> Vec<u64> {
    let from = from.min(values.len());
    let to = from.saturating_add(take).min(values.len());
    values[from..to].to_vec()
}

/// Table F.2 键 `B`：F.4.6 的内嵌文件流 hint 表（Table F.11/F.12）。
///
/// 键**条件存在**（35381「Present only if embedded file streams exist」），缺键
/// 即文档没有内嵌文件流，属合法形态、**不给诊断**，故返回 `None` 且不留条目。
/// `B` 的位置若落进前一张已解析表（页偏移表、缩略图表、generic 表或 extended 表）
/// 的表体内，已由装配层的位置剔除给出
/// [`LinearizationIssue::HintTablePositionInvalid`]，这里直接跳过——起点都不可信，
/// 不猜。键在而形态不符（读不满头部、项主序的某列越过流尾、声明位宽越出 0—32）
/// 落 [`LinearizationIssue::EmbeddedFileHintTableCorrupt`]。
///
/// 唯一的外部依赖是**文档级**位宽（Table F.3 项 11）：页偏移表不在场或读坏时
/// 项 5 的列无从解包，本表**不类型化**，只落
/// [`LinearizationIssue::EmbeddedFileIdWidthUnavailable`]（模块头有述，绝不以 0 宽
/// 冒充标识符）。
pub(in super::super) fn parse_embedded_file(
    positions: &mut Vec<HintTablePosition>,
    data: &[u8],
    page_offset: Option<&PageOffsetHintTable>,
    issues: &mut Vec<LinearizationIssue>,
) -> Option<EmbeddedFileHintTable> {
    let position = positions
        .iter()
        .find(|entry| entry.key == EMBEDDED_FILE_HINT_KEY)
        .map(|entry| entry.position)?;
    // 位置已由装配层收敛到 `0..=stream_len`，故换算 `usize` 不会失败；
    // 失败时只跳过本键，不猜起点。
    let Ok(start) = usize::try_from(position) else {
        return None;
    };
    let Some(document_bits) = page_offset.map(|table| table.header.shared_object_id_bits) else {
        issues.push(LinearizationIssue::EmbeddedFileIdWidthUnavailable);
        return None;
    };
    match EmbeddedFileHintTable::parse(data, start, document_bits) {
        Ok(table) => {
            drop_positions_inside_embedded_file_table(&table, positions, issues);
            Some(table)
        }
        Err(problem) => {
            issues.push(LinearizationIssue::EmbeddedFileHintTableCorrupt {
                problem: problem.to_string(),
            });
            None
        }
    }
}

/// F.4 前言：「each hint table shall begin at a byte boundary」（35482—35483）
/// 且各表按 NOTE 2 依次排布、互不重叠。本表长度随组数与项 5 的列长变化，故按
/// [`EmbeddedFileHintTable::end_byte`] 折算；其他 Table F.2 位置落进
/// `[键 B, 本键末)` 即自相矛盾。本键自身的起点不在此列；与其他键共用同一位置
/// 不视为矛盾——规范没有禁止同一张表兼任两类 hint，此处不发明 shall。
fn drop_positions_inside_embedded_file_table(
    table: &EmbeddedFileHintTable,
    positions: &mut Vec<HintTablePosition>,
    issues: &mut Vec<LinearizationIssue>,
) {
    let start = table.start_byte;
    let end = table.end_byte();
    let inside = positions
        .iter()
        .filter(|entry| {
            entry.key != EMBEDDED_FILE_HINT_KEY && (start..end).contains(&entry.position)
        })
        .cloned()
        .collect::<Vec<_>>();
    for entry in inside {
        issues.push(LinearizationIssue::HintTablePositionInvalid {
            key: entry.key,
            problem: format!(
                "position {} lies inside the embedded file stream hint table, which occupies bytes {start}..{end}",
                entry.position
            ),
        });
    }
    positions.retain(|entry| {
        entry.key == EMBEDDED_FILE_HINT_KEY || !(start..end).contains(&entry.position)
    });
}

impl LinearizationHints {
    /// F.4.6 内嵌文件流 hint 表结束后的第一个字节（流内绝对位置，起点是 Table
    /// F.2 键 `B` 的值）；本表未解析成功（含键缺失）时 `None`。F.4 前言要求各表
    /// 以字节边界开始且互不重叠，故其他键的位置落进 `[该键, 本值)` 即自相矛盾。
    pub fn embedded_file_end_byte(&self) -> Option<u64> {
        self.embedded_file
            .as_ref()
            .map(EmbeddedFileHintTable::end_byte)
    }
}
