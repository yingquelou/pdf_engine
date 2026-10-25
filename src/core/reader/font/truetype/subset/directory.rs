use std::collections::BTreeMap;

use super::{APPLE_TRUETYPE_SIGNATURE, CHECKSUM_MAGIC, TRUETYPE_SIGNATURE, invalid_subset};
use crate::core::reader::font::FontError;

type TableMap<'a> = BTreeMap<[u8; 4], &'a [u8]>;

pub(super) fn parse_directory(data: &[u8]) -> Result<([u8; 4], TableMap<'_>), FontError> {
    if data.len() < 12 {
        return Err(invalid_subset("SFNT header is truncated"));
    }
    let flavor: [u8; 4] = data[..4]
        .try_into()
        .expect("SFNT signature has a fixed-width slice");
    if flavor != TRUETYPE_SIGNATURE && flavor != APPLE_TRUETYPE_SIGNATURE {
        return Err(invalid_subset("unsupported SFNT signature"));
    }
    let table_count = usize::from(read_u16(data, 4)?);
    if table_count == 0 {
        return Err(invalid_subset("table directory is empty"));
    }
    let directory_length = table_count
        .checked_mul(16)
        .and_then(|length| length.checked_add(12))
        .ok_or_else(|| invalid_subset("table directory length overflows"))?;
    if directory_length > data.len() {
        return Err(invalid_subset("table directory is truncated"));
    }

    let mut tables = TableMap::new();
    let mut ranges = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let record = 12 + index * 16;
        let tag: [u8; 4] = data[record..record + 4]
            .try_into()
            .expect("table tag has a fixed-width slice");
        if tables.contains_key(&tag) {
            return Err(invalid_subset("table directory contains a duplicate tag"));
        }
        let offset = read_u32(data, record + 8)? as usize;
        let length = read_u32(data, record + 12)? as usize;
        if !offset.is_multiple_of(4) {
            return Err(invalid_subset("table offset is not four-byte aligned"));
        }
        if offset < directory_length {
            return Err(invalid_subset("table data overlaps the SFNT directory"));
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| invalid_subset("table range overflows"))?;
        if end > data.len() {
            return Err(invalid_subset("table data is outside the font program"));
        }
        if length > 0 {
            ranges.push((offset, end));
        }
        tables.insert(tag, &data[offset..end]);
    }

    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(invalid_subset("table data ranges overlap"));
    }
    Ok((flavor, tables))
}

pub(super) fn required_table<'a>(
    tables: &BTreeMap<[u8; 4], &'a [u8]>,
    tag: &[u8; 4],
) -> Result<&'a [u8], FontError> {
    tables.get(tag).copied().ok_or_else(|| {
        invalid_subset(format!(
            "missing required `{}` table",
            String::from_utf8_lossy(tag)
        ))
    })
}

pub(super) fn assemble_sfnt(
    flavor: [u8; 4],
    tables: BTreeMap<[u8; 4], Vec<u8>>,
) -> Result<Vec<u8>, FontError> {
    let table_count = u16::try_from(tables.len())
        .map_err(|_| invalid_subset("too many SFNT tables in the subset"))?;
    let directory_length = 12usize
        .checked_add(usize::from(table_count) * 16)
        .ok_or_else(|| invalid_subset("table directory length overflows"))?;

    let mut records = Vec::with_capacity(tables.len());
    let mut bytes = vec![0u8; directory_length];
    let mut cursor = directory_length;
    for (tag, table) in &tables {
        let offset = align4(cursor);
        let length =
            u32::try_from(table.len()).map_err(|_| invalid_subset("table length exceeds u32"))?;
        let offset_u32 =
            u32::try_from(offset).map_err(|_| invalid_subset("table offset exceeds u32"))?;
        records.push((*tag, table_checksum(table), offset_u32, length));
        bytes.resize(offset, 0);
        bytes.extend_from_slice(table);
        cursor = bytes.len();
    }
    bytes.resize(align4(bytes.len()), 0);

    let (search_range, entry_selector, range_shift) =
        sfnt_search_parameters(usize::from(table_count));
    bytes[..4].copy_from_slice(&flavor);
    bytes[4..6].copy_from_slice(&table_count.to_be_bytes());
    bytes[6..8].copy_from_slice(&search_range.to_be_bytes());
    bytes[8..10].copy_from_slice(&entry_selector.to_be_bytes());
    bytes[10..12].copy_from_slice(&range_shift.to_be_bytes());

    let mut head_offset = None;
    for (index, (tag, checksum, offset, length)) in records.iter().enumerate() {
        let record = 12 + index * 16;
        bytes[record..record + 4].copy_from_slice(tag);
        bytes[record + 4..record + 8].copy_from_slice(&checksum.to_be_bytes());
        bytes[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
        bytes[record + 12..record + 16].copy_from_slice(&length.to_be_bytes());
        if tag == b"head" {
            head_offset = Some(*offset as usize);
        }
    }

    let adjustment = CHECKSUM_MAGIC.wrapping_sub(table_checksum(&bytes));
    let head_offset = head_offset.ok_or_else(|| invalid_subset("subset is missing `head`"))?;
    let adjustment_start = head_offset
        .checked_add(8)
        .ok_or_else(|| invalid_subset("`head` table offset overflows"))?;
    let adjustment_end = adjustment_start
        .checked_add(4)
        .ok_or_else(|| invalid_subset("`head` table offset overflows"))?;
    let destination = bytes
        .get_mut(adjustment_start..adjustment_end)
        .ok_or_else(|| invalid_subset("`head` table is truncated"))?;
    destination.copy_from_slice(&adjustment.to_be_bytes());
    Ok(bytes)
}

fn sfnt_search_parameters(table_count: usize) -> (u16, u16, u16) {
    let mut power = 1usize;
    let mut entry_selector = 0u16;
    while power * 2 <= table_count {
        power *= 2;
        entry_selector += 1;
    }
    let search_range = u16::try_from(power * 16).expect("subset table count fits u16");
    let range_shift =
        u16::try_from(table_count * 16 - power * 16).expect("subset table count fits u16");
    (search_range, entry_selector, range_shift)
}

fn table_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in data.chunks(4) {
        let mut padded = [0u8; 4];
        padded[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(padded));
    }
    sum
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

pub(super) fn read_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| invalid_subset("font offset overflows"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| invalid_subset("font data is truncated"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn read_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| invalid_subset("font offset overflows"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| invalid_subset("font data is truncated"))?;
    Ok(i16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| invalid_subset("font offset overflows"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| invalid_subset("font data is truncated"))?;
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}
