//! `cmap` 单元测试：子表选择优先级、各 format 的码→字形查询与反向映射、
//! 畸形子表的拒绝；子表构造夹具共享自 `support`。

mod support;

use super::{Cmap, CmapFormat, FORMAT8_GROUP_COUNT_START, FORMAT8_GROUPS_START};
use support::*;

#[test]
fn maps_bmp_codepoints_with_format4() {
    let cmap_data = unicode_cmap(format4_table(), 4);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format4);
    assert!(!parsed.is_symbolic());
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), Some(3));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x42, 8).unwrap(), None);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F600, 8).unwrap(), None);
}

#[test]
fn selects_symbol_only_format4_cmap() {
    let cmap_data = records_cmap(symbolic_format4_table(1), 3, 0);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format4);
    assert!(parsed.is_symbolic());
    assert_eq!(parsed.glyph_id(&cmap_data, 0xF041, 8).unwrap(), Some(1));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), None);
}

#[test]
fn prefers_unicode_format4_over_symbolic_format4() {
    let unicode = format4_table();
    let symbolic = symbolic_format4_table(1);
    let records_length = 4 + 2 * 8;
    let unicode_offset = records_length;
    let symbolic_offset = unicode_offset + unicode.len();
    let mut cmap_data = vec![0; symbolic_offset + symbolic.len()];
    cmap_data[2..4].copy_from_slice(&2u16.to_be_bytes());
    cmap_data[4..6].copy_from_slice(&3u16.to_be_bytes());
    cmap_data[6..8].copy_from_slice(&1u16.to_be_bytes());
    cmap_data[8..12].copy_from_slice(&(unicode_offset as u32).to_be_bytes());
    cmap_data[12..14].copy_from_slice(&3u16.to_be_bytes());
    cmap_data[14..16].copy_from_slice(&0u16.to_be_bytes());
    cmap_data[16..20].copy_from_slice(&(symbolic_offset as u32).to_be_bytes());
    cmap_data[unicode_offset..symbolic_offset].copy_from_slice(&unicode);
    cmap_data[symbolic_offset..].copy_from_slice(&symbolic);

    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format4);
    assert!(!parsed.is_symbolic());
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), Some(3));
    assert_eq!(parsed.glyph_id(&cmap_data, 0xF041, 8).unwrap(), None);
}

#[test]
fn maps_bmp_codepoints_with_format4_glyph_array() {
    let mut subtable = format4_table();
    subtable.resize(36, 0);
    subtable[2..4].copy_from_slice(&36u16.to_be_bytes());
    subtable[14..16].copy_from_slice(&0x42u16.to_be_bytes());
    subtable[24..26].copy_from_slice(&0i16.to_be_bytes());
    subtable[28..30].copy_from_slice(&4u16.to_be_bytes());
    subtable[32..34].copy_from_slice(&5u16.to_be_bytes());
    subtable[34..36].copy_from_slice(&0u16.to_be_bytes());
    let cmap_data = unicode_cmap(subtable, 4);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), Some(5));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x42, 8).unwrap(), Some(0));
}

#[test]
fn maps_supplementary_codepoints_with_format12() {
    let cmap_data = unicode_cmap(format12_table(), 12);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format12);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F600, 8).unwrap(), Some(4));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), None);
}

#[test]
fn maps_mixed_16_and_32_bit_codes_with_format8() {
    let subtable = format8_table(
        &[0x8000],
        &[(0x41, 0x41, 3), (0x8000, 0x8002, 5), (0x1F600, 0x1F601, 9)],
    );
    let cmap_data = unicode_cmap(subtable, 8);
    let parsed = Cmap::parse(&cmap_data, 16).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format8);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 16).unwrap(), Some(3));
    // 0x8000 is flagged in is32, so it is not a standalone 16-bit code.
    assert_eq!(parsed.glyph_id(&cmap_data, 0x8000, 16).unwrap(), None);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x8001, 16).unwrap(), Some(6));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x8002, 16).unwrap(), Some(7));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F600, 16).unwrap(), Some(9));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F601, 16).unwrap(), Some(10));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F602, 16).unwrap(), None);

    let reverse = parsed.reverse_cmap(&cmap_data, 16).unwrap();
    assert_eq!(reverse.get(&3), Some(&0x41));
    assert_eq!(reverse.get(&6), Some(&0x8001));
    assert_eq!(reverse.get(&7), Some(&0x8002));
    assert_eq!(reverse.get(&9), Some(&0x1F600));
    assert!(!reverse.values().any(|&code| code == 0x8000));
}

#[test]
fn maps_trimmed_32_bit_array_with_format10() {
    let cmap_data = unicode_cmap(format10_table(0x1_0000, &[4, 5, 6]), 10);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format10);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1_0000, 8).unwrap(), Some(4));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1_0002, 8).unwrap(), Some(6));
    assert_eq!(parsed.glyph_id(&cmap_data, 0xFFFF, 8).unwrap(), None);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1_0003, 8).unwrap(), None);

    let reverse = parsed.reverse_cmap(&cmap_data, 8).unwrap();
    assert_eq!(reverse.get(&4), Some(&0x1_0000));
    assert_eq!(reverse.get(&6), Some(&0x1_0002));
}

#[test]
fn prefers_format12_over_format8_and_format8_over_format4() {
    let cmap_data = multi_records_cmap(&[
        (3, 10, format8_table(&[], &[(0x41, 0x41, 3)])),
        (3, 10, format12_table()),
    ]);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format12);

    let cmap_data = multi_records_cmap(&[
        (3, 1, format4_table()),
        (3, 10, format8_table(&[], &[(0x41, 0x41, 3)])),
    ]);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format8);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), Some(3));
}

#[test]
fn rejects_invalid_format8_and_format10_tables() {
    // The group count promises more records than the subtable holds.
    let mut truncated = format8_table(&[], &[(0x41, 0x41, 3)]);
    truncated[FORMAT8_GROUP_COUNT_START..FORMAT8_GROUPS_START].copy_from_slice(&2u32.to_be_bytes());
    assert!(Cmap::parse(&unicode_cmap(truncated, 8), 8).is_err());

    let overlapping = format8_table(&[], &[(0x41, 0x42, 3), (0x42, 0x43, 4)]);
    assert!(Cmap::parse(&unicode_cmap(overlapping, 8), 8).is_err());

    let missing_glyph = format8_table(&[], &[(0x41, 0x41, 8)]);
    assert!(Cmap::parse(&unicode_cmap(missing_glyph, 8), 8).is_err());

    // The glyph array is shorter than numChars promises.
    let mut truncated = format10_table(0x41, &[3, 4]);
    truncated.truncate(23);
    truncated[4..8].copy_from_slice(&23u32.to_be_bytes());
    assert!(Cmap::parse(&unicode_cmap(truncated, 10), 8).is_err());

    // Character codes must stay inside the Unicode range.
    let beyond_unicode = format10_table(0x10_FFFF, &[3, 4]);
    assert!(Cmap::parse(&unicode_cmap(beyond_unicode, 10), 8).is_err());

    let missing_glyph = format10_table(0x41, &[8]);
    assert!(Cmap::parse(&unicode_cmap(missing_glyph, 10), 8).is_err());
}

#[test]
fn maps_high_byte_subheaders_with_format2() {
    let cmap_data = records_cmap(format2_table(), 0, 3);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format2);

    // Single-byte codes route through subHeader 0.
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), Some(1));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x42, 8).unwrap(), Some(2));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x43, 8).unwrap(), None);
    // Double-byte codes route through subHeaderKeys[0x81] and apply idDelta.
    assert_eq!(parsed.glyph_id(&cmap_data, 0x8140, 8).unwrap(), Some(3));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x8141, 8).unwrap(), Some(4));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x8142, 8).unwrap(), None);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x8241, 8).unwrap(), None);
    // Format 2 only covers the BMP.
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F600, 8).unwrap(), None);

    let reverse = parsed.reverse_cmap(&cmap_data, 8).unwrap();
    assert_eq!(reverse.len(), 4);
    assert_eq!(reverse.get(&1), Some(&0x41));
    assert_eq!(reverse.get(&2), Some(&0x42));
    assert_eq!(reverse.get(&3), Some(&0x8140));
    assert_eq!(reverse.get(&4), Some(&0x8141));

    // Windows Unicode BMP (3, 1) also accepts format 2.
    let windows_data = records_cmap(format2_table(), 3, 1);
    let windows = Cmap::parse(&windows_data, 8).unwrap();
    assert_eq!(windows.format(), CmapFormat::Format2);
    assert_eq!(windows.glyph_id(&windows_data, 0x8140, 8).unwrap(), Some(3));
}

#[test]
fn maps_many_codes_to_one_glyph_with_format13() {
    let subtable = format13_table(&[
        (0x100, 0x102, 5),
        (0x200, 0x201, 5),
        (0x300, 0x300, 6),
        (0x1F600, 0x1F601, 7),
    ]);
    let cmap_data = unicode_cmap(subtable, 13);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format13);

    for code in [0x100, 0x101, 0x102, 0x200, 0x201] {
        assert_eq!(parsed.glyph_id(&cmap_data, code, 8).unwrap(), Some(5));
    }
    assert_eq!(parsed.glyph_id(&cmap_data, 0x300, 8).unwrap(), Some(6));
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F601, 8).unwrap(), Some(7));
    assert_eq!(parsed.glyph_id(&cmap_data, 0xFF, 8).unwrap(), None);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x202, 8).unwrap(), None);

    // Reverse lookup keeps the smallest codepoint per glyph.
    let reverse = parsed.reverse_cmap(&cmap_data, 8).unwrap();
    assert_eq!(reverse.get(&5), Some(&0x100));
    assert_eq!(reverse.get(&6), Some(&0x300));
    assert_eq!(reverse.get(&7), Some(&0x1F600));
}

#[test]
fn prefers_format12_when_both_unicode_formats_are_available() {
    let format4 = format4_table();
    let format12 = format12_table();
    let records_length = 4 + 2 * 8;
    let format4_offset = records_length;
    let format12_offset = format4_offset + format4.len();
    let mut cmap_data = vec![0; format12_offset + format12.len()];
    cmap_data[2..4].copy_from_slice(&2u16.to_be_bytes());
    cmap_data[4..6].copy_from_slice(&3u16.to_be_bytes());
    cmap_data[6..8].copy_from_slice(&1u16.to_be_bytes());
    cmap_data[8..12].copy_from_slice(&(format4_offset as u32).to_be_bytes());
    cmap_data[12..14].copy_from_slice(&3u16.to_be_bytes());
    cmap_data[14..16].copy_from_slice(&10u16.to_be_bytes());
    cmap_data[16..20].copy_from_slice(&(format12_offset as u32).to_be_bytes());
    cmap_data[format4_offset..format12_offset].copy_from_slice(&format4);
    cmap_data[format12_offset..].copy_from_slice(&format12);

    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format12);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F600, 8).unwrap(), Some(4));
}

#[test]
fn prefers_format12_over_format13() {
    let cmap_data = multi_records_cmap(&[
        (3, 10, format13_table(&[(0x100, 0x101, 5)])),
        (3, 10, format12_table()),
    ]);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format12);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x100, 8).unwrap(), None);
    assert_eq!(parsed.glyph_id(&cmap_data, 0x1F600, 8).unwrap(), Some(4));

    // Record order must not override the priority.
    let cmap_data = multi_records_cmap(&[
        (3, 10, format12_table()),
        (3, 10, format13_table(&[(0x100, 0x101, 5)])),
    ]);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format12);
}

#[test]
fn prefers_format4_over_format2() {
    let cmap_data = multi_records_cmap(&[(3, 1, format2_table()), (0, 3, format4_table())]);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format4);
    // Format 4 maps U+0041 to glyph 3; format 2 would map it to glyph 1.
    assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), Some(3));

    let cmap_data = multi_records_cmap(&[(0, 3, format4_table()), (3, 1, format2_table())]);
    let parsed = Cmap::parse(&cmap_data, 8).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format4);
}

#[test]
fn rejects_out_of_range_format2_offsets() {
    // The glyph array for subHeader 1 starts past the end of the subtable.
    let mut bad_offset = format2_table();
    bad_offset[532..534].copy_from_slice(&0x7FFEu16.to_be_bytes());
    let records = records_cmap(bad_offset, 3, 1);
    assert!(Cmap::parse(&records, 8).is_err());

    // idRangeOffset must be even.
    let mut odd_offset = format2_table();
    odd_offset[524..526].copy_from_slice(&9u16.to_be_bytes());
    let records = records_cmap(odd_offset, 3, 1);
    assert!(Cmap::parse(&records, 8).is_err());

    // A subHeaderKey must point at an aligned, existing subHeader.
    let mut bogus_key = format2_table();
    bogus_key[6 + 0x82 * 2..8 + 0x82 * 2].copy_from_slice(&0x7FF8u16.to_be_bytes());
    let records = records_cmap(bogus_key, 3, 1);
    assert!(Cmap::parse(&records, 8).is_err());

    let mut misaligned_key = format2_table();
    misaligned_key[6 + 0x83 * 2..8 + 0x83 * 2].copy_from_slice(&9u16.to_be_bytes());
    let records = records_cmap(misaligned_key, 3, 1);
    assert!(Cmap::parse(&records, 8).is_err());

    // A truncated subHeader array cannot be validated.
    let mut truncated = format2_table();
    truncated.truncate(520);
    truncated[2..4].copy_from_slice(&520u16.to_be_bytes());
    let records = records_cmap(truncated, 3, 1);
    assert!(Cmap::parse(&records, 8).is_err());

    // The glyph arrays must fit inside the subtable.
    let mut clipped_array = format2_table();
    clipped_array.truncate(536);
    clipped_array[2..4].copy_from_slice(&536u16.to_be_bytes());
    let records = records_cmap(clipped_array, 3, 1);
    assert!(Cmap::parse(&records, 8).is_err());

    // Every mapped glyph must exist in the font.
    let records = records_cmap(format2_table(), 3, 1);
    assert!(Cmap::parse(&records, 3).is_err());
}

#[test]
fn rejects_invalid_format13_groups() {
    let overlapping = format13_table(&[(0x100, 0x101, 5), (0x101, 0x102, 6)]);
    let records = unicode_cmap(overlapping, 13);
    assert!(Cmap::parse(&records, 8).is_err());

    let unordered = format13_table(&[(0x200, 0x201, 5), (0x100, 0x101, 6)]);
    let records = unicode_cmap(unordered, 13);
    assert!(Cmap::parse(&records, 8).is_err());

    let inverted = format13_table(&[(0x200, 0x100, 5)]);
    let records = unicode_cmap(inverted, 13);
    assert!(Cmap::parse(&records, 8).is_err());

    let missing_glyph = format13_table(&[(0x100, 0x101, 8)]);
    let records = unicode_cmap(missing_glyph, 13);
    assert!(Cmap::parse(&records, 8).is_err());

    // The group count promises more records than the subtable holds.
    let mut truncated = format13_table(&[(0x100, 0x101, 5)]);
    truncated[12..16].copy_from_slice(&2u32.to_be_bytes());
    let records = unicode_cmap(truncated, 13);
    assert!(Cmap::parse(&records, 8).is_err());
}

#[test]
fn rejects_malformed_cmap_data() {
    assert!(Cmap::parse(&[0, 0, 0, 1], 8).is_err());

    let mut invalid_format4 = format4_table();
    invalid_format4[16..18].copy_from_slice(&0x40u16.to_be_bytes());
    let records = unicode_cmap(invalid_format4, 4);
    assert!(Cmap::parse(&records, 8).is_err());

    let mut invalid_glyph_array_offset = format4_table();
    invalid_glyph_array_offset[28..30].copy_from_slice(&2u16.to_be_bytes());
    let records = unicode_cmap(invalid_glyph_array_offset, 4);
    assert!(Cmap::parse(&records, 8).is_err());

    let records = unicode_cmap(format4_table(), 4);
    let parsed = Cmap::parse(&records, 3).unwrap();
    assert!(parsed.glyph_id(&records, 0x41, 3).is_err());

    let mut invalid_format12 = format12_table();
    invalid_format12[24..28].copy_from_slice(&u32::MAX.to_be_bytes());
    let records = unicode_cmap(invalid_format12, 4);
    assert!(Cmap::parse(&records, 8).is_err());
}

#[test]
fn maps_byte_encoding_with_format0() {
    let mut subtable = vec![0; 262];
    subtable[..2].copy_from_slice(&0u16.to_be_bytes());
    subtable[2..4].copy_from_slice(&262u16.to_be_bytes());
    subtable[6] = 0x41;
    subtable[7] = 0x42;
    let records = unicode_cmap(subtable.clone(), 0);
    let parsed = Cmap::parse(&records, 256).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format0);
    assert_eq!(parsed.glyph_id(&records, 0, 256).unwrap(), Some(0x41));
    assert_eq!(parsed.glyph_id(&records, 1, 256).unwrap(), Some(0x42));
    assert_eq!(parsed.glyph_id(&records, 256, 256).unwrap(), None);
}

#[test]
fn maps_trimmed_table_with_format6() {
    let mut subtable = vec![0; 20];
    subtable[..2].copy_from_slice(&6u16.to_be_bytes());
    subtable[2..4].copy_from_slice(&20u16.to_be_bytes());
    subtable[4..6].copy_from_slice(&0u16.to_be_bytes());
    subtable[6..8].copy_from_slice(&0x41u16.to_be_bytes());
    subtable[8..10].copy_from_slice(&3u16.to_be_bytes());
    subtable[10..12].copy_from_slice(&10u16.to_be_bytes());
    subtable[12..14].copy_from_slice(&11u16.to_be_bytes());
    subtable[14..16].copy_from_slice(&12u16.to_be_bytes());
    let records = unicode_cmap(subtable.clone(), 6);
    let parsed = Cmap::parse(&records, 256).unwrap();
    assert_eq!(parsed.format(), CmapFormat::Format6);
    assert_eq!(parsed.glyph_id(&records, 0x41, 256).unwrap(), Some(10));
    assert_eq!(parsed.glyph_id(&records, 0x42, 256).unwrap(), Some(11));
    assert_eq!(parsed.glyph_id(&records, 0x43, 256).unwrap(), Some(12));
    assert_eq!(parsed.glyph_id(&records, 0x40, 256).unwrap(), None);
    assert_eq!(parsed.glyph_id(&records, 0x44, 256).unwrap(), None);
}
