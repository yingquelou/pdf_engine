//! 共享对象 hint 表（ISO 32000-1 F.4.2，Table F.5 头部 + Table F.6 组条目）的
//! **写端**编码：把读端已定义的类型化模型 [`SharedObjectHintTable`] 逐位还原成
//! F.4 前言「高位在前」的字节流，与既有读端 [`SharedObjectHintTable::parse`] 精确对偶。
//!
//! 规范锚点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 输出行，沿用读端
//! `src/core/reader/linearization/hints/shared.rs` 已锚定的同一批）：
//! - F.4.2（35605—35610）：「The shared object hint table shall consist of a
//!   header section (Table F.5) followed by one or more shared object group
//!   entries (Table F.6). There shall be two sequences of shared object group
//!   entries: the ones for objects located in the first page, followed by the
//!   ones for objects located in the shared objects section」；
//! - F.4.2（35651—35660）：条目**不连续**、按项交错（转置，项主序）：
//!   「a) Item 1 for the first group, item 1 for the second group, and so on
//!   b) Item 2 … c) Item 3 … d) Item 4 …」——先全部组的项目 1（长度增量），
//!   再全部组的项目 2（1 位签名标志），再按组顺序写标志置位者的项目 3（128 位
//!   MD5），最后全部组的项目 4（对象数减一）。
//! - Table F.5 头部 7 项定宽 32/32/32/32/16/32/16；项 5 是组对象数（Table F.6
//!   项 4）的位宽、项 7 是组长度增量（Table F.6 项 1）的位宽。
//! - Table F.6 项 4「A number equal to 1 less than the number of objects in the
//!   group」——存对象数减一。
//!
//! **宽度来源与读端一致**：表头项 5/项 7 声明的位宽既写进 16 位头部、又用作
//! 组条目项目 4/项目 1 的字段宽度（文档级标识符宽度不在此表，属页偏移表/F.4.6
//! 的硬依赖，本家族自足）。头部项 5/7 超过 64 位原语上限即 [`HintEncodeError::
//! WidthTooWide`]——与读端 [`BitReader::unsigned`](crate::core::reader::linearization::hints::bits::BitReader)
//! 的 `MAX_UNSIGNED_BITS` 门同口径，保证写出的字节读端一定解得开。
//!
//! **条目数自洽**：Table F.5 项 3（首页条目数）须等于 `first_page_groups.len()`、
//! 项 4（含项 3 的总条目数）须等于两序列向量长度之和，否则 [`HintEncodeError::
//! EntryCountMismatch`]——读端按头部计数重建两条序列，计数与实际向量不符的模型
//! 无从往返（也是读端 `check_header_consistency` 里项 3 大于项 4 的自相矛盾来源）。
//!
//! **签名自洽**：Table F.6 项 2 的标志须与项 3 的 16 字节 MD5 严格同现，否则
//! [`HintEncodeError::SignatureFlagMismatch`]——读端只在标志置位时读 128 位并据此
//! 回填 `Option`。

use crate::core::reader::linearization::hints::{SharedObjectGroupEntry, SharedObjectHeader};

use super::super::bits::{HintBitWriter, HintEncodeError, MAX_UNSIGNED_BITS};
use crate::core::reader::linearization::hints::SharedObjectHintTable;

/// Table F.5 头部项 5/7 声明位宽的上界：与读端 `BitReader` 的 64 位原语门同口径。
const HEADER_WIDTH_LIMIT: u32 = MAX_UNSIGNED_BITS;

/// 把一个合法的 [`SharedObjectHintTable`] 编码成从字节边界开始、可按 F.4.2 解析的
/// 位流字节。
///
/// 编码顺序严格镜像读端 [`SharedObjectHintTable::parse`]：先 Table F.5 头部 7 项，
/// 再首页序列（按项交错）、再共享对象区序列（按项交错）。所有拒绝都在写出任何
/// 位之前给出，绝不产出半成品字节。
pub fn encode_shared_object_table(
    table: &SharedObjectHintTable,
) -> Result<Vec<u8>, HintEncodeError> {
    validate_counts(&table.header, table)?;
    validate_group_widths(&table.header)?;
    let mut w = HintBitWriter::new();
    write_header(&mut w, &table.header)?;
    write_sequence(&mut w, &table.header, &table.first_page_groups)?;
    write_sequence(&mut w, &table.header, &table.shared_section_groups)?;
    Ok(w.finish())
}

/// Table F.5 头部：32/32/32/32/16/32/16。项 5/项 7 的声明宽度受 16 位字段本身
/// 约束（`value < 2^16`），故此处按写入 16 位校验：越界的宽度已由
/// [`validate_group_widths`] 提前挡住（`HEADER_WIDTH_LIMIT` 远小于 `2^16`）。
fn write_header(w: &mut HintBitWriter, header: &SharedObjectHeader) -> Result<(), HintEncodeError> {
    w.unsigned(header.first_object_number, 32)?;
    w.unsigned(header.first_object_location, 32)?;
    w.unsigned(header.first_page_entries, 32)?;
    w.unsigned(header.total_entries, 32)?;
    w.unsigned(u64::from(header.group_object_bits), 16)?;
    w.unsigned(header.least_group_length, 32)?;
    w.unsigned(u64::from(header.group_length_bits), 16)?;
    Ok(())
}

/// 一个序列的 Table F.6 组条目，按 F.4.2 a)—d) 的项交错（转置）序写出：
/// 项 1 全组 → 项 2 全组 → 项 3（仅标志置位者，按组顺序）→ 项 4 全组。
fn write_sequence(
    w: &mut HintBitWriter,
    header: &SharedObjectHeader,
    groups: &[SharedObjectGroupEntry],
) -> Result<(), HintEncodeError> {
    // 先做整序列的一致性预检：任一组的标志/签名自洽问题都在写任何位之前拒绝。
    for (index, group) in groups.iter().enumerate() {
        if group.signature_present != group.signature.is_some() {
            return Err(HintEncodeError::SignatureFlagMismatch { group: index });
        }
    }
    // a) 项 1：长度增量（宽度 = 头部项 7）。
    for group in groups {
        w.unsigned(group.length_delta, header.group_length_bits)?;
    }
    // b) 项 2：1 位签名标志。
    for group in groups {
        w.flag(group.signature_present)?;
    }
    // c) 项 3：按组顺序为标志置位的组写 128 位 MD5。
    for group in groups {
        if let Some(signature) = group.signature {
            w.bytes(&signature)?;
        }
    }
    // d) 项 4：对象数减一（宽度 = 头部项 5）。
    for group in groups {
        w.unsigned(group.object_count_minus_one, header.group_object_bits)?;
    }
    Ok(())
}

/// Table F.5 项 3/项 4 与实际组向量的自洽校验。
fn validate_counts(
    header: &SharedObjectHeader,
    table: &SharedObjectHintTable,
) -> Result<(), HintEncodeError> {
    let first_page = table.first_page_groups.len();
    let total = first_page + table.shared_section_groups.len();
    let declared_first_page = usize::try_from(header.first_page_entries).ok();
    let declared_total = usize::try_from(header.total_entries).ok();
    if declared_first_page != Some(first_page) || declared_total != Some(total) {
        return Err(HintEncodeError::EntryCountMismatch {
            declared_first_page: header.first_page_entries,
            actual_first_page: first_page,
            declared_total: header.total_entries,
            actual_total: total,
        });
    }
    Ok(())
}

/// 表头声明的两个字段宽度须落进读端可解析的原语上限（`HEADER_WIDTH_LIMIT`）。
fn validate_group_widths(header: &SharedObjectHeader) -> Result<(), HintEncodeError> {
    for bits in [header.group_object_bits, header.group_length_bits] {
        if bits > HEADER_WIDTH_LIMIT {
            return Err(HintEncodeError::WidthTooWide {
                bits: u64::from(bits),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张自洽的共享对象 hint 表：首页 2 组（首组带 16 字节 MD5）、共享对象区
    /// 1 组（带签名），宽度/计数/标志全部与读端重建口径一致。
    fn valid_table() -> SharedObjectHintTable {
        let md5 = std::array::from_fn::<u8, 16, _>(|i| (i * 7) as u8);
        SharedObjectHintTable {
            header: SharedObjectHeader {
                first_object_number: 500,
                first_object_location: 3_000,
                first_page_entries: 2,
                total_entries: 3,
                group_object_bits: 5,
                least_group_length: 16,
                group_length_bits: 9,
            },
            first_page_groups: vec![
                SharedObjectGroupEntry {
                    length_delta: 100,
                    signature_present: true,
                    signature: Some(md5),
                    object_count_minus_one: 3,
                },
                SharedObjectGroupEntry {
                    length_delta: 200,
                    signature_present: false,
                    signature: None,
                    object_count_minus_one: 5,
                },
            ],
            shared_section_groups: vec![SharedObjectGroupEntry {
                length_delta: 300,
                signature_present: true,
                signature: Some(md5),
                object_count_minus_one: 1,
            }],
        }
    }

    /// 编码回读：表头七项与两条序列的每一组四项全部逐字段相等（整结构相等）。
    #[test]
    fn encode_shared_object_table_roundtrips_header_and_both_group_sequences() {
        let original = valid_table();
        let bytes = encode_shared_object_table(&original).unwrap();
        let parsed = SharedObjectHintTable::parse(&bytes, 0).unwrap();
        assert_eq!(parsed.header, original.header);
        assert_eq!(parsed.first_page_groups, original.first_page_groups);
        assert_eq!(parsed.shared_section_groups, original.shared_section_groups);
        assert_eq!(parsed, original);
    }

    /// 空序列（首页 0 组、共享区 0 组）也合法，编码只落表头 7 项定宽。
    #[test]
    fn encode_shared_object_table_roundtrips_an_empty_two_sequence_table() {
        let mut table = valid_table();
        table.header.first_page_entries = 0;
        table.header.total_entries = 0;
        table.first_page_groups.clear();
        table.shared_section_groups.clear();
        let bytes = encode_shared_object_table(&table).unwrap();
        assert_eq!(bytes.len(), 24); // 7 项 = 32*5 + 16*2 = 192 位 = 24 字节整
        assert_eq!(SharedObjectHintTable::parse(&bytes, 0).unwrap(), table);
    }

    /// 表头声明位宽超过 64 位原语上限即拒（写出前失败、零字节）——与读端同宽度会 InvalidWidth 对偶。
    #[test]
    fn encode_shared_object_table_rejects_header_width_above_the_primitive_limit() {
        let mut table = valid_table();
        table.header.group_object_bits = HEADER_WIDTH_LIMIT + 1;
        assert_eq!(
            encode_shared_object_table(&table),
            Err(HintEncodeError::WidthTooWide {
                bits: u64::from(HEADER_WIDTH_LIMIT) + 1
            })
        );
    }

    /// 首页/总条目计数与实际向量不符即拒——读端按头部计数重建序列，不符无从往返。
    #[test]
    fn encode_shared_object_table_rejects_inconsistent_entry_counts() {
        let mut table = valid_table();
        table.header.total_entries = 99;
        assert!(matches!(
            encode_shared_object_table(&table),
            Err(HintEncodeError::EntryCountMismatch { .. })
        ));
        let mut table = valid_table();
        table.header.first_page_entries = 5;
        assert!(matches!(
            encode_shared_object_table(&table),
            Err(HintEncodeError::EntryCountMismatch { .. })
        ));
    }

    /// 签名标志与 16 字节 MD5 必须同现：置位却无签名、或清零却带签名，都拒。
    #[test]
    fn encode_shared_object_table_rejects_signature_flag_payload_mismatch() {
        let mut table = valid_table();
        table.first_page_groups[0].signature = None;
        assert!(matches!(
            encode_shared_object_table(&table),
            Err(HintEncodeError::SignatureFlagMismatch { group: 0 })
        ));
        let mut table = valid_table();
        table.first_page_groups[1].signature_present = true;
        assert!(matches!(
            encode_shared_object_table(&table),
            Err(HintEncodeError::SignatureFlagMismatch { group: 1 })
        ));
    }

    /// 组字段超出表头声明宽度即拒（写出前失败、零字节），拒绝截断出读端解不回的值。
    #[test]
    fn encode_shared_object_table_rejects_group_field_over_declared_width() {
        let mut table = valid_table();
        table.first_page_groups[0].length_delta = 1u64 << table.header.group_length_bits;
        assert!(matches!(
            encode_shared_object_table(&table),
            Err(HintEncodeError::ValueTooWide { .. })
        ));
        let mut table = valid_table();
        table.shared_section_groups[0].object_count_minus_one =
            1u64 << table.header.group_object_bits;
        assert!(matches!(
            encode_shared_object_table(&table),
            Err(HintEncodeError::ValueTooWide { .. })
        ));
    }

    /// 总位数非 8 的倍数时末尾补零到字节边界；读端只看前面的位，往返仍逐字段相等。
    #[test]
    fn encode_shared_object_table_pads_the_final_byte_and_stays_parseable() {
        let mut table = valid_table();
        // 收窄声明宽度让总位数落在非字节边界上。
        table.header.group_object_bits = 1;
        table.header.group_length_bits = 1;
        table.header.first_page_entries = 1;
        table.header.total_entries = 1;
        table.first_page_groups = vec![SharedObjectGroupEntry {
            length_delta: 0,
            signature_present: false,
            signature: None,
            object_count_minus_one: 1,
        }];
        table.shared_section_groups.clear();
        let bytes = encode_shared_object_table(&table).unwrap();
        // 表头 192 位 + 组条目 1+1+1 = 195 位 → 补齐 25 字节。
        assert_eq!(bytes.len(), 25);
        assert_eq!(SharedObjectHintTable::parse(&bytes, 0).unwrap(), table);
    }
}
