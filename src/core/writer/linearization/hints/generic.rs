//! generic hint 表（ISO 32000-1 F.4.4，Table F.9）的**写端**编码：把读端已定义
//! 的类型化模型 [`GenericHintTable`] 的四个定宽项逐位还原成 F.4 前言「高位在前」
//! 的字节流，与既有读端 [`GenericHintTable::parse`] 精确对偶。
//!
//! 选择该家族作写端的**第二家**理由：它是 F.4 六家里唯一「长度与声明无关」的表
//! ——Table F.9 四项全部定宽 32 位（35801—35805），既不像共享对象表要靠表头声明
//! 宽度，也不像页偏移表（F.3/F.4）那样含逐页序列与增量列，是自足、易往返的形态。
//!
//! 规范锚点（行号＝`pdftotext -raw docs/PDF32000_2008.pdf` 输出行，沿用读端
//! `src/core/reader/linearization/hints/generic.rs` 已锚定的同一批）：
//! - F.4.4（35749—35751）：「Such hints shall be represented by a generic hint
//!   table, which describes a single group of objects that are located together
//!   in the PDF file」——一张 generic 表只描述**一个**对象组，没有条目数、没有
//!   交错列，四项按表内项号顺序连续写出；
//! - Table F.9 四项（35801—35805）：①组内第一个对象的对象号 ②组内第一个对象的
//!   位置 ③组内对象数 ④对象组的长度（字节），各 32 位，合计 128 位＝16 字节整，
//!   末尾无残位（读端只消费这 128 位、其余字节留给下一张以字节边界开始的表）。
//!
//! **溢出而非截断**：四项定宽 32 位，模型里任一 u64 值 ≥ 2^32 时 [`HintBitWriter::
//! unsigned`] 返回 [`HintEncodeError::ValueTooWide`] 且在写任何位前失败、产出零字节
//! ——读端 `BitReader::unsigned(32)` 会把高位丢弃，若写端不拒绝则往返不再逐项相等，
//! 故本层提前挡住。模型的 `key`/`start_byte`/`bits_consumed` 是解析侧的元数据（表身
//! 份与游标），**不进位流**，故往返只比对四项数值。

use crate::core::reader::linearization::hints::GenericHintTable;

use super::super::bits::{HintBitWriter, HintEncodeError};

/// Table F.9 四项的定宽（位，35801—35805）。与读端
/// [`GenericHintTable::parse`] 的 `TABLE_F9_ITEM_WIDTH_BITS` 同值。
const TABLE_F9_ITEM_WIDTH_BITS: u32 = 32;

/// 把一个 [`GenericHintTable`] 的四个定宽项编码成 16 字节（128 位）的位流。
///
/// 编码顺序严格镜像读端 [`GenericHintTable::parse`]：项 1 → 项 2 → 项 3 → 项 4，
/// 每项定宽 32 位、高位在前。四项落不进 32 位（值 ≥ 2^32）即 [`HintEncodeError::
/// ValueTooWide`]，且在任何位写出前失败、产出零字节。产物可直接交回
/// `GenericHintTable::parse(&bytes, 0, key)` 解出、四项逐一相等。
pub fn encode_generic_table(table: &GenericHintTable) -> Result<Vec<u8>, HintEncodeError> {
    let mut writer = HintBitWriter::new();
    write_group(&mut writer, table)?;
    Ok(writer.finish())
}

/// 按 Table F.9 项号顺序写出四个 32 位定宽项。
fn write_group(
    writer: &mut HintBitWriter,
    table: &GenericHintTable,
) -> Result<(), HintEncodeError> {
    writer.unsigned(table.first_object_number, TABLE_F9_ITEM_WIDTH_BITS)?;
    writer.unsigned(table.first_object_location, TABLE_F9_ITEM_WIDTH_BITS)?;
    writer.unsigned(table.object_count, TABLE_F9_ITEM_WIDTH_BITS)?;
    writer.unsigned(table.group_length, TABLE_F9_ITEM_WIDTH_BITS)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张合法的 generic 表（四项在 32 位内、游标元数据与定宽读出一致）。
    fn table(seed: u64) -> GenericHintTable {
        GenericHintTable {
            key: "O",
            first_object_number: 100 + seed,
            first_object_location: 20_000 + seed,
            object_count: 2 + seed,
            group_length: 500 + seed,
            start_byte: 0,
            bits_consumed: 128,
        }
    }

    /// 四项定宽 32 位 → 恒 16 字节整、无残位；解析回模型后逐项（含元数据）相等。
    #[test]
    fn encode_generic_table_is_sixteen_bytes_and_roundtrips_every_item() {
        let original = table(3);
        let bytes = encode_generic_table(&original).unwrap();
        assert_eq!(bytes.len(), 16);
        let parsed = GenericHintTable::parse(&bytes, 0, "O").unwrap();
        assert_eq!(parsed, original);
    }

    /// 任一四项超出 32 位即类型化拒绝、不产出被截断的字节（读端会丢高位，往返不再相等）。
    #[test]
    fn encode_generic_table_rejects_a_value_above_the_32_bit_fixed_width() {
        for field in 0..4 {
            let mut original = table(0);
            match field {
                0 => original.first_object_number = 1u64 << 32,
                1 => original.first_object_location = 1u64 << 32,
                2 => original.object_count = 1u64 << 32,
                _ => original.group_length = 1u64 << 32,
            }
            let bits = u64::from(TABLE_F9_ITEM_WIDTH_BITS);
            assert_eq!(
                encode_generic_table(&original),
                Err(HintEncodeError::ValueTooWide {
                    value: 1u64 << 32,
                    bits: bits as u32
                }),
                "field {field} above 32 bits must be refused"
            );
        }
    }

    /// 两张表首尾相邻排进一条流：各自从 0 与 16 字节处解析回原模型，互不越界。
    #[test]
    fn two_generic_tables_tile_the_stream_and_parse_back_at_their_offsets() {
        let first = table(0);
        let mut second = table(1);
        let mut bytes = encode_generic_table(&first).unwrap();
        bytes.extend_from_slice(&encode_generic_table(&second).unwrap());
        assert_eq!(GenericHintTable::parse(&bytes, 0, "O").unwrap(), first);
        // `start_byte` 是解析侧元数据，读出反映真实偏移（16），不进位流。
        second.start_byte = 16;
        assert_eq!(GenericHintTable::parse(&bytes, 16, "O").unwrap(), second);
    }

    /// `0xFF…FF`（32 位满值）在边界内、编码回读相等；`1<<32` 越界即拒。
    #[test]
    fn generic_table_encodes_the_top_of_the_32_bit_range_but_not_above_it() {
        let original = GenericHintTable {
            key: "L",
            first_object_number: u32::MAX as u64,
            first_object_location: 0,
            object_count: u32::MAX as u64,
            group_length: 0,
            start_byte: 0,
            bits_consumed: 128,
        };
        let bytes = encode_generic_table(&original).unwrap();
        assert_eq!(GenericHintTable::parse(&bytes, 0, "L").unwrap(), original);
    }
}
