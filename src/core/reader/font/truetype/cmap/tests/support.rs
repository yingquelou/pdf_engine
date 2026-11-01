//! `cmap` 测试共享夹具：各 format 子表的字节构造与 `cmap` 表封装。

use super::super::{
    FORMAT2_SUBHEADER_SIZE, FORMAT2_SUBHEADERS_START, FORMAT8_GROUP_COUNT_START,
    FORMAT8_GROUPS_START, FORMAT8_IS32_START, GROUP_SIZE,
};

pub(super) fn format4_table() -> Vec<u8> {
    // Two segments: U+0041 maps to glyph 3, and the required U+FFFF sentinel.
    let mut data = vec![0; 32];
    data[..2].copy_from_slice(&4u16.to_be_bytes());
    data[2..4].copy_from_slice(&32u16.to_be_bytes());
    data[4..6].copy_from_slice(&0u16.to_be_bytes());
    data[6..8].copy_from_slice(&4u16.to_be_bytes());
    data[8..10].copy_from_slice(&0u16.to_be_bytes());
    data[10..12].copy_from_slice(&2u16.to_be_bytes());
    data[12..14].copy_from_slice(&2u16.to_be_bytes());
    data[14..16].copy_from_slice(&0x41u16.to_be_bytes());
    data[16..18].copy_from_slice(&0xFFFFu16.to_be_bytes());
    data[18..20].copy_from_slice(&0u16.to_be_bytes());
    data[20..22].copy_from_slice(&0x41u16.to_be_bytes());
    data[22..24].copy_from_slice(&0xFFFFu16.to_be_bytes());
    data[24..26].copy_from_slice(&(-62i16).to_be_bytes());
    data[26..28].copy_from_slice(&1i16.to_be_bytes());
    data[28..30].copy_from_slice(&0u16.to_be_bytes());
    data[30..32].copy_from_slice(&0u16.to_be_bytes());
    data
}

/// Format 2 subtable with a single-byte subHeader 0 (`0x41..=0x42` ->
/// glyphs 1 and 2) and a double-byte subHeader for high byte `0x81`
/// (`0x8140..=0x8141` -> raw glyphs 2 and 3 plus an `idDelta` of 1).
pub(super) fn format2_table() -> Vec<u8> {
    let glyph_arrays_start = FORMAT2_SUBHEADERS_START + 2 * FORMAT2_SUBHEADER_SIZE;
    let mut data = vec![0; glyph_arrays_start + 4 * 2];
    let length = data.len() as u16;
    data[..2].copy_from_slice(&2u16.to_be_bytes());
    data[2..4].copy_from_slice(&length.to_be_bytes());
    data[4..6].copy_from_slice(&0u16.to_be_bytes());
    // High byte 0x81 selects subHeader 1 (index * 8).
    data[6 + 0x81 * 2..8 + 0x81 * 2].copy_from_slice(&8u16.to_be_bytes());
    // subHeader 0: single-byte range 0x41..=0x42 with an idDelta of 0.
    data[FORMAT2_SUBHEADERS_START..FORMAT2_SUBHEADERS_START + 2]
        .copy_from_slice(&0x41u16.to_be_bytes());
    data[FORMAT2_SUBHEADERS_START + 2..FORMAT2_SUBHEADERS_START + 4]
        .copy_from_slice(&2u16.to_be_bytes());
    data[FORMAT2_SUBHEADERS_START + 4..FORMAT2_SUBHEADERS_START + 6]
        .copy_from_slice(&0i16.to_be_bytes());
    // idRangeOffset points at the shared glyph array: 534 - 524 = 10.
    data[FORMAT2_SUBHEADERS_START + 6..FORMAT2_SUBHEADERS_START + 8]
        .copy_from_slice(&10u16.to_be_bytes());
    // subHeader 1: low-byte range 0x40..=0x41 with an idDelta of 1.
    data[FORMAT2_SUBHEADERS_START + 8..FORMAT2_SUBHEADERS_START + 10]
        .copy_from_slice(&0x40u16.to_be_bytes());
    data[FORMAT2_SUBHEADERS_START + 10..FORMAT2_SUBHEADERS_START + 12]
        .copy_from_slice(&2u16.to_be_bytes());
    data[FORMAT2_SUBHEADERS_START + 12..FORMAT2_SUBHEADERS_START + 14]
        .copy_from_slice(&1i16.to_be_bytes());
    // idRangeOffset points at the shared glyph array: 538 - 532 = 6.
    data[FORMAT2_SUBHEADERS_START + 14..FORMAT2_SUBHEADERS_START + 16]
        .copy_from_slice(&6u16.to_be_bytes());
    data[glyph_arrays_start..glyph_arrays_start + 2].copy_from_slice(&1u16.to_be_bytes());
    data[glyph_arrays_start + 2..glyph_arrays_start + 4].copy_from_slice(&2u16.to_be_bytes());
    data[glyph_arrays_start + 4..glyph_arrays_start + 6].copy_from_slice(&2u16.to_be_bytes());
    data[glyph_arrays_start + 6..glyph_arrays_start + 8].copy_from_slice(&3u16.to_be_bytes());
    data
}

pub(super) fn format12_table() -> Vec<u8> {
    let mut data = vec![0; 28];
    data[..2].copy_from_slice(&12u16.to_be_bytes());
    data[4..8].copy_from_slice(&28u32.to_be_bytes());
    data[12..16].copy_from_slice(&1u32.to_be_bytes());
    data[16..20].copy_from_slice(&0x1F600u32.to_be_bytes());
    data[20..24].copy_from_slice(&0x1F600u32.to_be_bytes());
    data[24..28].copy_from_slice(&4u32.to_be_bytes());
    data
}

pub(super) fn format13_table(groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut data = vec![0; 16 + groups.len() * 12];
    let length = data.len() as u32;
    data[..2].copy_from_slice(&13u16.to_be_bytes());
    data[4..8].copy_from_slice(&length.to_be_bytes());
    data[12..16].copy_from_slice(&(groups.len() as u32).to_be_bytes());
    for (index, (start, end, glyph)) in groups.iter().enumerate() {
        let group = 16 + index * 12;
        data[group..group + 4].copy_from_slice(&start.to_be_bytes());
        data[group + 4..group + 8].copy_from_slice(&end.to_be_bytes());
        data[group + 8..group + 12].copy_from_slice(&glyph.to_be_bytes());
    }
    data
}

/// Format 8 subtable whose `is32` bitmap marks `is32_codes`; `groups`
/// are `(start, end, start_glyph)` records.
pub(super) fn format8_table(is32_codes: &[u16], groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut data = vec![0; FORMAT8_GROUPS_START + groups.len() * GROUP_SIZE];
    let length = data.len() as u32;
    data[..2].copy_from_slice(&8u16.to_be_bytes());
    data[4..8].copy_from_slice(&length.to_be_bytes());
    for &code in is32_codes {
        data[FORMAT8_IS32_START + (usize::from(code) >> 3)] |= 1 << (code % 8);
    }
    data[FORMAT8_GROUP_COUNT_START..FORMAT8_GROUPS_START]
        .copy_from_slice(&(groups.len() as u32).to_be_bytes());
    for (index, (start, end, glyph)) in groups.iter().enumerate() {
        let group = FORMAT8_GROUPS_START + index * GROUP_SIZE;
        data[group..group + 4].copy_from_slice(&start.to_be_bytes());
        data[group + 4..group + 8].copy_from_slice(&end.to_be_bytes());
        data[group + 8..group + 12].copy_from_slice(&glyph.to_be_bytes());
    }
    data
}

pub(super) fn format10_table(start: u32, glyphs: &[u16]) -> Vec<u8> {
    let mut data = vec![0; 20 + glyphs.len() * 2];
    let length = data.len() as u32;
    data[..2].copy_from_slice(&10u16.to_be_bytes());
    data[4..8].copy_from_slice(&length.to_be_bytes());
    data[12..16].copy_from_slice(&start.to_be_bytes());
    data[16..20].copy_from_slice(&(glyphs.len() as u32).to_be_bytes());
    for (index, glyph) in glyphs.iter().enumerate() {
        let offset = 20 + index * 2;
        data[offset..offset + 2].copy_from_slice(&glyph.to_be_bytes());
    }
    data
}

/// Format 4 subtable with the classic symbol range: `0xF041` maps to
/// `glyph` through the segment delta, plus the required `0xFFFF` sentinel.
pub(super) fn symbolic_format4_table(glyph: u16) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[..2].copy_from_slice(&4u16.to_be_bytes());
    data[2..4].copy_from_slice(&32u16.to_be_bytes());
    data[6..8].copy_from_slice(&4u16.to_be_bytes());
    data[10..12].copy_from_slice(&2u16.to_be_bytes());
    data[12..14].copy_from_slice(&2u16.to_be_bytes());
    data[14..16].copy_from_slice(&0xF041u16.to_be_bytes());
    data[16..18].copy_from_slice(&0xFFFFu16.to_be_bytes());
    data[20..22].copy_from_slice(&0xF041u16.to_be_bytes());
    data[22..24].copy_from_slice(&0xFFFFu16.to_be_bytes());
    data[24..26].copy_from_slice(&glyph.wrapping_sub(0xF041).to_be_bytes());
    data[26..28].copy_from_slice(&1u16.to_be_bytes());
    data
}

pub(super) fn unicode_cmap(subtable: Vec<u8>, format: u16) -> Vec<u8> {
    let encoding = if matches!(format, 8 | 10 | 12 | 13) {
        10
    } else {
        1
    };
    records_cmap(subtable, 3, encoding)
}

pub(super) fn records_cmap(subtable: Vec<u8>, platform: u16, encoding: u16) -> Vec<u8> {
    let mut data = vec![0; 12 + subtable.len()];
    data[2..4].copy_from_slice(&1u16.to_be_bytes());
    data[4..6].copy_from_slice(&platform.to_be_bytes());
    data[6..8].copy_from_slice(&encoding.to_be_bytes());
    data[8..12].copy_from_slice(&12u32.to_be_bytes());
    data[12..].copy_from_slice(&subtable);
    data
}

pub(super) fn multi_records_cmap(records: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let records_length = 4 + records.len() * 8;
    let total = records_length
        + records
            .iter()
            .map(|(_, _, subtable)| subtable.len())
            .sum::<usize>();
    let mut data = vec![0; total];
    data[2..4].copy_from_slice(&(records.len() as u16).to_be_bytes());
    let mut offset = records_length;
    for (index, (platform, encoding, subtable)) in records.iter().enumerate() {
        let record = 4 + index * 8;
        data[record..record + 2].copy_from_slice(&platform.to_be_bytes());
        data[record + 2..record + 4].copy_from_slice(&encoding.to_be_bytes());
        data[record + 4..record + 8].copy_from_slice(&(offset as u32).to_be_bytes());
        data[offset..offset + subtable.len()].copy_from_slice(subtable);
        offset += subtable.len();
    }
    data
}
