//! 选中子表的结构性校验：`Cmap::parse` 返回前逐 format 验证子表边界、
//! 段序、组序与字形 ID 合法性。

use super::raw::{checked_add, invalid_cmap, read_u16, read_u32};
use super::{
    Cmap, CmapFormat, FORMAT2_SUBHEADER_KEYS_START, FORMAT2_SUBHEADER_SIZE,
    FORMAT2_SUBHEADERS_START, FORMAT8_GROUP_COUNT_START, FORMAT8_GROUPS_START, FontError,
    GROUP_SIZE,
};

impl Cmap {
    pub(super) fn validate(&self, cmap_data: &[u8], glyph_count: u16) -> Result<(), FontError> {
        let subtable = cmap_data
            .get(self.subtable.clone())
            .ok_or_else(|| invalid_cmap("selected subtable is outside the cmap table"))?;
        match self.format {
            CmapFormat::Format0 => {
                if subtable.len() < 262 {
                    return Err(invalid_cmap("format 0 subtable is truncated"));
                }
            }
            CmapFormat::Format2 => {
                if subtable.len() < FORMAT2_SUBHEADERS_START {
                    return Err(invalid_cmap("format 2 subtable is truncated"));
                }
                let mut highest_key = 0usize;
                for index in 0..256 {
                    let key = usize::from(read_u16(
                        subtable,
                        FORMAT2_SUBHEADER_KEYS_START + index * 2,
                    )?);
                    if key % FORMAT2_SUBHEADER_SIZE != 0 {
                        return Err(invalid_cmap(
                            "format 2 subHeaderKeys are not aligned to subHeaders",
                        ));
                    }
                    highest_key = highest_key.max(key);
                }
                let subheader_count = highest_key / FORMAT2_SUBHEADER_SIZE + 1;
                let glyph_arrays_start = checked_add(
                    FORMAT2_SUBHEADERS_START,
                    subheader_count
                        .checked_mul(FORMAT2_SUBHEADER_SIZE)
                        .ok_or_else(|| invalid_cmap("format 2 subHeader count overflows"))?,
                )?;
                if glyph_arrays_start > subtable.len() {
                    return Err(invalid_cmap("format 2 subHeaders are truncated"));
                }
                for index in 0..subheader_count {
                    let record = FORMAT2_SUBHEADERS_START + index * FORMAT2_SUBHEADER_SIZE;
                    let first_code = read_u16(subtable, record)?;
                    let entry_count = usize::from(read_u16(subtable, record + 2)?);
                    let id_delta = read_u16(subtable, record + 4)?;
                    let range_offset = usize::from(read_u16(subtable, record + 6)?);
                    if range_offset % 2 != 0 {
                        return Err(invalid_cmap("format 2 idRangeOffset is not even"));
                    }
                    if entry_count == 0 {
                        continue;
                    }
                    if u32::from(first_code) + entry_count as u32 > 0x100 {
                        return Err(invalid_cmap(
                            "format 2 subHeader codes exceed the byte range",
                        ));
                    }
                    let first_glyph_offset = record
                        .checked_add(6)
                        .and_then(|position| position.checked_add(range_offset))
                        .ok_or_else(|| invalid_cmap("format 2 glyph offset overflows"))?;
                    let last_glyph_offset = first_glyph_offset
                        .checked_add(
                            entry_count
                                .checked_mul(2)
                                .ok_or_else(|| invalid_cmap("format 2 glyph range overflows"))?,
                        )
                        .ok_or_else(|| invalid_cmap("format 2 glyph range overflows"))?;
                    if first_glyph_offset < glyph_arrays_start || last_glyph_offset > subtable.len()
                    {
                        return Err(invalid_cmap("format 2 glyph array is outside the subtable"));
                    }
                    for entry in 0..entry_count {
                        let glyph = read_u16(subtable, first_glyph_offset + entry * 2)?;
                        let glyph = if glyph == 0 {
                            0
                        } else {
                            glyph.wrapping_add(id_delta)
                        };
                        if glyph >= glyph_count {
                            return Err(invalid_cmap("format 2 maps to an invalid glyph ID"));
                        }
                    }
                }
            }
            CmapFormat::Format4 => {
                if subtable.len() < 16 {
                    return Err(invalid_cmap("format 4 subtable is truncated"));
                }
                let segment_count_x2 = usize::from(read_u16(subtable, 6)?);
                if segment_count_x2 == 0 || segment_count_x2 % 2 != 0 {
                    return Err(invalid_cmap("format 4 has an invalid segment count"));
                }
                let segment_count = segment_count_x2 / 2;
                let end_codes_start = 14usize;
                let start_codes_start = end_codes_start + segment_count * 2 + 2;
                let deltas_start = start_codes_start + segment_count * 2;
                let range_offsets_start = deltas_start + segment_count * 2;
                let required_length = range_offsets_start + segment_count * 2;
                if required_length > subtable.len() {
                    return Err(invalid_cmap("format 4 segment arrays are truncated"));
                }
                let mut previous_end = None;
                for index in 0..segment_count {
                    let end_code = read_u16(subtable, end_codes_start + index * 2)?;
                    let start_code = read_u16(subtable, start_codes_start + index * 2)?;
                    if start_code > end_code || previous_end.is_some_and(|end| end >= start_code) {
                        return Err(invalid_cmap("format 4 segments are unordered or overlap"));
                    }
                    let range_offset = read_u16(subtable, range_offsets_start + index * 2)?;
                    if range_offset != 0 {
                        let first_glyph_offset = range_offsets_start
                            .checked_add(index * 2)
                            .and_then(|position| position.checked_add(usize::from(range_offset)))
                            .ok_or_else(|| invalid_cmap("format 4 glyph offset overflows"))?;
                        let last_glyph_offset = first_glyph_offset
                            .checked_add(usize::from(end_code - start_code) * 2)
                            .and_then(|position| position.checked_add(2))
                            .ok_or_else(|| invalid_cmap("format 4 glyph range overflows"))?;
                        if first_glyph_offset < required_length
                            || last_glyph_offset > subtable.len()
                        {
                            return Err(invalid_cmap(
                                "format 4 glyph array is outside the subtable",
                            ));
                        }
                    }
                    previous_end = Some(end_code);
                }
            }
            CmapFormat::Format6 => {
                if subtable.len() < 10 {
                    return Err(invalid_cmap("format 6 subtable is truncated"));
                }
                let entry_count = usize::from(read_u16(subtable, 8)?);
                let required_length = 10usize
                    .checked_add(
                        entry_count
                            .checked_mul(2)
                            .ok_or_else(|| invalid_cmap("format 6 entry count overflows"))?,
                    )
                    .ok_or_else(|| invalid_cmap("format 6 length overflows"))?;
                if required_length > subtable.len() {
                    return Err(invalid_cmap("format 6 glyph array is truncated"));
                }
            }
            CmapFormat::Format8 => {
                if subtable.len() < FORMAT8_GROUPS_START {
                    return Err(invalid_cmap("format 8 subtable is truncated"));
                }
                let group_count = usize::try_from(read_u32(subtable, FORMAT8_GROUP_COUNT_START)?)
                    .map_err(|_| {
                    invalid_cmap("format 8 group count does not fit this platform")
                })?;
                let required_length = FORMAT8_GROUPS_START
                    .checked_add(
                        group_count
                            .checked_mul(GROUP_SIZE)
                            .ok_or_else(|| invalid_cmap("format 8 group count overflows"))?,
                    )
                    .ok_or_else(|| invalid_cmap("format 8 group range overflows"))?;
                if required_length > subtable.len() {
                    return Err(invalid_cmap("format 8 groups are truncated"));
                }
                let mut previous_end = None;
                for index in 0..group_count {
                    let group = FORMAT8_GROUPS_START + index * GROUP_SIZE;
                    let start = read_u32(subtable, group)?;
                    let end = read_u32(subtable, group + 4)?;
                    let start_glyph = read_u32(subtable, group + 8)?;
                    let final_glyph = start_glyph
                        .checked_add(end.saturating_sub(start))
                        .ok_or_else(|| invalid_cmap("format 8 glyph range overflows"))?;
                    if start > end
                        || end > 0x10_FFFF
                        || previous_end.is_some_and(|previous| previous >= start)
                        || final_glyph >= u32::from(glyph_count)
                    {
                        return Err(invalid_cmap("format 8 group is invalid"));
                    }
                    previous_end = Some(end);
                }
            }
            CmapFormat::Format10 => {
                if subtable.len() < 20 {
                    return Err(invalid_cmap("format 10 subtable is truncated"));
                }
                let start = read_u32(subtable, 12)?;
                let count = read_u32(subtable, 16)?;
                let count_usize = usize::try_from(count).map_err(|_| {
                    invalid_cmap("format 10 character count does not fit this platform")
                })?;
                let required_length = 20usize
                    .checked_add(
                        count_usize
                            .checked_mul(2)
                            .ok_or_else(|| invalid_cmap("format 10 character count overflows"))?,
                    )
                    .ok_or_else(|| invalid_cmap("format 10 length overflows"))?;
                if required_length > subtable.len() {
                    return Err(invalid_cmap("format 10 glyph array is truncated"));
                }
                if count > 0 {
                    let end = start
                        .checked_add(count - 1)
                        .ok_or_else(|| invalid_cmap("format 10 character range overflows"))?;
                    if end > 0x10_FFFF {
                        return Err(invalid_cmap("format 10 character range exceeds Unicode"));
                    }
                }
                for index in 0..count_usize {
                    if read_u16(subtable, 20 + index * 2)? >= glyph_count {
                        return Err(invalid_cmap("format 10 maps to an invalid glyph ID"));
                    }
                }
            }
            CmapFormat::Format12 => {
                if subtable.len() < 16 {
                    return Err(invalid_cmap("format 12 subtable is truncated"));
                }
                let group_count = usize::try_from(read_u32(subtable, 12)?).map_err(|_| {
                    invalid_cmap("format 12 group count does not fit this platform")
                })?;
                let required_length = 16usize
                    .checked_add(
                        group_count
                            .checked_mul(12)
                            .ok_or_else(|| invalid_cmap("format 12 group count overflows"))?,
                    )
                    .ok_or_else(|| invalid_cmap("format 12 group range overflows"))?;
                if required_length > subtable.len() {
                    return Err(invalid_cmap("format 12 groups are truncated"));
                }
                let mut previous_end = None;
                for index in 0..group_count {
                    let group = 16 + index * 12;
                    let start = read_u32(subtable, group)?;
                    let end = read_u32(subtable, group + 4)?;
                    let start_glyph = read_u32(subtable, group + 8)?;
                    let final_glyph = start_glyph
                        .checked_add(end.saturating_sub(start))
                        .ok_or_else(|| invalid_cmap("format 12 glyph range overflows"))?;
                    if start > end
                        || end > 0x10_FFFF
                        || previous_end.is_some_and(|previous| previous >= start)
                        || final_glyph >= u32::from(glyph_count)
                    {
                        return Err(invalid_cmap("format 12 group is invalid"));
                    }
                    previous_end = Some(end);
                }
            }
            CmapFormat::Format13 => {
                if subtable.len() < 16 {
                    return Err(invalid_cmap("format 13 subtable is truncated"));
                }
                let group_count = usize::try_from(read_u32(subtable, 12)?).map_err(|_| {
                    invalid_cmap("format 13 group count does not fit this platform")
                })?;
                let required_length = 16usize
                    .checked_add(
                        group_count
                            .checked_mul(12)
                            .ok_or_else(|| invalid_cmap("format 13 group count overflows"))?,
                    )
                    .ok_or_else(|| invalid_cmap("format 13 group range overflows"))?;
                if required_length > subtable.len() {
                    return Err(invalid_cmap("format 13 groups are truncated"));
                }
                let mut previous_end = None;
                for index in 0..group_count {
                    let group = 16 + index * 12;
                    let start = read_u32(subtable, group)?;
                    let end = read_u32(subtable, group + 4)?;
                    let glyph = read_u32(subtable, group + 8)?;
                    if start > end
                        || previous_end.is_some_and(|previous| previous >= start)
                        || glyph >= u32::from(glyph_count)
                    {
                        return Err(invalid_cmap("format 13 group is invalid"));
                    }
                    previous_end = Some(end);
                }
            }
        }
        Ok(())
    }
}
