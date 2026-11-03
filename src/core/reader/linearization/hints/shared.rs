//! 共享对象 hint 表（ISO 32000-1 F.4.2，Table F.5 头部 + Table F.6 组条目）。
//!
//! 选择该家族的理由：F.4 六个家族里它字段最少、条目序规则最简（表头 7 项
//! 全定宽，组条目 4 项，其中两项宽度由表头给出、一项为 1 位标志 + 可选
//! 128 位 MD5）。原文要点：
//!
//! - F.4.2：表由头部（Table F.5）后跟「一个或多个共享对象组条目」
//!   （Table F.6）组成；「There shall be two sequences of shared object
//!   group entries: the ones for objects located in the first page,
//!   followed by the ones for objects located in the shared objects
//!   section」；每序列内条目不连续，按项交错：先全部组的项目 1，再全部组
//!   的项目 2（1 位标志），再按标志为 1 的组顺序写项目 3（128 位 MD5），
//!   最后全部组的项目 4。
//! - Table F.5 项 4 是**含首页组数（项 3）在内**的总条目数，故项 3 大于
//!   项 4 即自相矛盾（在读端 [`super`] 层落类型化诊断）。
//! - Table F.6 项 4：「A number equal to 1 less than the number of
//!   objects in the group」——存的是对象数减一。
//! - 位置是增量：组长度 = 长度增量 + Table F.5 项 6（最小长度）；首页第一
//!   个对象的位置取自 page offset hint 表头部（Table F.3 项 4），共享对象
//!   区第一个对象的位置取自 Table F.5 项 2，其余按前组长度累加；hint 流
//!   本身的位置修正规则见 F.4 前言 NOTE 3（[`super::LinearizationHints::adjust_position`]）。

use super::bits::{BitReader, HintBitError};

/// Table F.5：共享对象 hint 表头部（7 项，位宽 32/32/32/32/16/32/16）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedObjectHeader {
    /// 项 1（32 位）：共享对象区（part 8）第一个对象的对象号。
    pub first_object_number: u64,
    /// 项 2（32 位）：共享对象区第一个对象的位置。
    pub first_object_location: u64,
    /// 项 3（32 位）：首页（part 6）的共享对象条目数（含非共享对象）。
    pub first_page_entries: u64,
    /// 项 4（32 位）：共享对象区条目总数，**含**项 3 的首页条目数。
    pub total_entries: u64,
    /// 项 5（16 位）：表示组内最大对象数所需的位数（Table F.6 项 4 宽度）。
    pub group_object_bits: u32,
    /// 项 6（32 位）：共享对象组的最小长度（字节）。
    pub least_group_length: u64,
    /// 项 7（16 位）：表示组长度最大差值所需的位数（Table F.6 项 1 宽度）。
    pub group_length_bits: u32,
}

/// Table F.6：一个共享对象组条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedObjectGroupEntry {
    /// 项 1：长度增量；组长度 = 本值 + 头部项 6（Table F.5）。
    pub length_delta: u64,
    /// 项 2：1 位标志，指示项目 3 的签名是否存在。
    pub signature_present: bool,
    /// 项 3：仅当标志为 1 时存在，16 字节 MD5 签名（与表单签名域无关）。
    pub signature: Option<[u8; 16]>,
    /// 项 4：组内对象数减一。
    pub object_count_minus_one: u64,
}

/// Table F.6 的两条序列：首页序列与共享对象区序列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedObjectHintTable {
    /// Table F.5 头部。
    pub header: SharedObjectHeader,
    /// 首页（part 6）的组条目；首页所有对象都有条目，无论是否真的共享。
    pub first_page_groups: Vec<SharedObjectGroupEntry>,
    /// 共享对象区（part 8）的组条目，条数为总条目数减首页条目数。
    pub shared_section_groups: Vec<SharedObjectGroupEntry>,
}

impl SharedObjectHintTable {
    /// 从 `data`（主/溢出 hint 流解码拼接后的完整位流）的字节偏移
    /// `start` 处解析（F.4.2：表自字节边界开始，偏移由主 hint 流字典的
    /// Table F.2 键 `S` 给出）。
    pub fn parse(data: &[u8], start: usize) -> Result<Self, HintBitError> {
        if start > data.len() {
            return Err(HintBitError::ShortBuffer {
                needed_bits: 8,
                remaining_bits: 0,
                position_bits: start as u64 * 8,
            });
        }
        let mut reader = BitReader::new(&data[start..]);
        let header = SharedObjectHeader {
            first_object_number: reader.unsigned(32)?,
            first_object_location: reader.unsigned(32)?,
            first_page_entries: reader.unsigned(32)?,
            total_entries: reader.unsigned(32)?,
            group_object_bits: clamp_width(reader.unsigned(16)?),
            least_group_length: reader.unsigned(32)?,
            group_length_bits: clamp_width(reader.unsigned(16)?),
        };
        // 每组至少消费 1 位（项 2 标志），据此在进入循环前挡住不可信计数。
        if header.total_entries > reader.remaining_bits() {
            return Err(HintBitError::ShortBuffer {
                needed_bits: header.total_entries,
                remaining_bits: reader.remaining_bits(),
                position_bits: reader.position_bits(),
            });
        }
        // 项 3 大于项 4 属自相矛盾：截断到总数继续解析，让诊断在
        // [`super`] 层与部分结果一起给出。
        let first_page = header.first_page_entries.min(header.total_entries);
        let shared_section = header.total_entries - first_page;
        let first_page_groups = parse_groups(&mut reader, first_page as usize, &header)?;
        let shared_section_groups = parse_groups(&mut reader, shared_section as usize, &header)?;
        Ok(Self {
            header,
            first_page_groups,
            shared_section_groups,
        })
    }
}

/// Table F.5 项 5/7 声明的是位宽，超过 64 位原语上限即不可用。
fn clamp_width(raw: u64) -> u32 {
    raw.min(u32::MAX as u64) as u32
}

/// 按 Table F.6 的项交错序解析 `count` 个组条目。
fn parse_groups(
    reader: &mut BitReader<'_>,
    count: usize,
    header: &SharedObjectHeader,
) -> Result<Vec<SharedObjectGroupEntry>, HintBitError> {
    // 项 1：长度增量（宽度 = 头部项 7）。
    let mut length_deltas = Vec::with_capacity(count);
    for _ in 0..count {
        length_deltas.push(reader.unsigned(header.group_length_bits)?);
    }
    // 项 2：1 位签名标志。
    let mut flags = Vec::with_capacity(count);
    for _ in 0..count {
        flags.push(reader.flag()?);
    }
    // 项 3：按组顺序为标志置位的组读 128 位 MD5。
    let mut signatures: Vec<[u8; 16]> = Vec::new();
    for present in flags.iter().copied() {
        if !present {
            continue;
        }
        let raw = reader.bytes(16)?;
        let fixed: [u8; 16] = raw.try_into().expect("16 bytes requested");
        signatures.push(fixed);
    }
    // 项 4：对象数减一（宽度 = 头部项 5）。
    let mut object_counts = Vec::with_capacity(count);
    for _ in 0..count {
        object_counts.push(reader.unsigned(header.group_object_bits)?);
    }
    // 项 4 在交错序里独立成列；按「长度增量 + 标志 + 弹出式对象数」配对，
    // 签名按组顺序从收集队列里逐个取用（标志置位才有，且第 k 个置位恰好
    // 对应队列第 k 个元素，故 remove(0) 不会越界）。
    let mut entries = Vec::with_capacity(count);
    let mut counts = object_counts.into_iter();
    for (length_delta, signature_present) in length_deltas.into_iter().zip(flags) {
        let signature = match signature_present {
            true => Some(signatures.remove(0)),
            false => None,
        };
        entries.push(SharedObjectGroupEntry {
            length_delta,
            signature_present,
            signature,
            object_count_minus_one: counts.next().expect("one count per group"),
        });
    }
    Ok(entries)
}
