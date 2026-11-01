//! 字形 ID → Unicode 码点的反向映射：逐 format 遍历全部映射项，
//! 同一字形命中多个码点时保留最小码点。

use std::collections::BTreeMap;

use super::raw::{invalid_cmap, read_u16, read_u32};
use super::{
    Cmap, CmapFormat, FORMAT2_SUBHEADER_KEYS_START, FORMAT2_SUBHEADER_SIZE,
    FORMAT2_SUBHEADERS_START, FORMAT8_GROUP_COUNT_START, FORMAT8_GROUPS_START, FontError,
    GROUP_SIZE,
};

impl Cmap {
    /// Build a reverse mapping from glyph ID to Unicode codepoint.
    ///
    /// Iterates all cmap entries to construct glyph → codepoint map.
    /// When multiple codepoints map to the same glyph, the lowest codepoint
    /// wins, keeping the result deterministic.
    pub fn reverse_cmap(
        &self,
        cmap_data: &[u8],
        glyph_count: u16,
    ) -> Result<BTreeMap<u16, u32>, FontError> {
        let subtable = cmap_data
            .get(self.subtable.clone())
            .ok_or_else(|| invalid_cmap("selected subtable is outside the cmap table"))?;

        let mut reverse = BTreeMap::new();

        match self.format {
            CmapFormat::Format0 => {
                if subtable.len() < 262 {
                    return Err(invalid_cmap("format 0 subtable is truncated"));
                }
                for code in 0..256u32 {
                    let glyph = u16::from(subtable[6 + code as usize]);
                    if glyph < glyph_count {
                        reverse.entry(glyph).or_insert(code);
                    }
                }
            }
            CmapFormat::Format2 => {
                if subtable.len() < FORMAT2_SUBHEADERS_START {
                    return Err(invalid_cmap("format 2 subtable is truncated"));
                }
                // Single-byte codes always use subHeader 0.
                for code in 0..=0xFFu16 {
                    if let Some(glyph) = Self::format2_subheader_glyph(subtable, 0, code)?
                        && glyph < glyph_count
                    {
                        reverse.entry(glyph).or_insert(u32::from(code));
                    }
                }
                for high in 1..=0xFFusize {
                    let key =
                        usize::from(read_u16(subtable, FORMAT2_SUBHEADER_KEYS_START + high * 2)?);
                    let subheader = key / FORMAT2_SUBHEADER_SIZE;
                    if subheader == 0 {
                        continue;
                    }
                    let record = FORMAT2_SUBHEADERS_START + subheader * FORMAT2_SUBHEADER_SIZE;
                    let first_code = u32::from(read_u16(subtable, record)?);
                    let entry_count = u32::from(read_u16(subtable, record + 2)?);
                    for offset in 0..entry_count {
                        let low = first_code + offset;
                        if low > 0xFF {
                            break;
                        }
                        let codepoint = ((high as u32) << 8) | low;
                        if let Some(glyph) =
                            Self::format2_subheader_glyph(subtable, subheader, low as u16)?
                            && glyph < glyph_count
                        {
                            reverse.entry(glyph).or_insert(codepoint);
                        }
                    }
                }
            }
            CmapFormat::Format4 => {
                let segment_count = usize::from(read_u16(subtable, 6)?) / 2;
                let end_codes_start = 14;
                let start_codes_start = end_codes_start + segment_count * 2 + 2;
                let deltas_start = start_codes_start + segment_count * 2;
                let range_offsets_start = deltas_start + segment_count * 2;

                for index in 0..segment_count {
                    let end_code = read_u16(subtable, end_codes_start + index * 2)?;
                    let start_code = read_u16(subtable, start_codes_start + index * 2)?;
                    let delta = read_u16(subtable, deltas_start + index * 2)?;
                    let range_offset_position = range_offsets_start + index * 2;
                    let range_offset = read_u16(subtable, range_offset_position)?;

                    for code in start_code..=end_code {
                        let glyph = if range_offset == 0 {
                            code.wrapping_add(delta)
                        } else {
                            let glyph_position = range_offset_position
                                .checked_add(usize::from(range_offset))
                                .and_then(|pos| pos.checked_add(usize::from(code - start_code) * 2))
                                .ok_or_else(|| invalid_cmap("format 4 glyph offset overflows"))?;
                            let g = read_u16(subtable, glyph_position)?;
                            if g == 0 { 0 } else { g.wrapping_add(delta) }
                        };

                        if glyph < glyph_count {
                            reverse.entry(glyph).or_insert(u32::from(code));
                        }
                    }
                }
            }
            CmapFormat::Format6 => {
                if subtable.len() < 10 {
                    return Err(invalid_cmap("format 6 subtable is truncated"));
                }
                let first_code = u32::from(read_u16(subtable, 6)?);
                let entry_count = usize::from(read_u16(subtable, 8)?);

                for index in 0..entry_count {
                    let glyph_offset = 10 + index * 2;
                    if glyph_offset + 2 > subtable.len() {
                        return Err(invalid_cmap("format 6 glyph array is truncated"));
                    }
                    let glyph = read_u16(subtable, glyph_offset)?;
                    if glyph < glyph_count {
                        reverse.entry(glyph).or_insert(first_code + index as u32);
                    }
                }
            }
            CmapFormat::Format8 => {
                let group_count = usize::try_from(read_u32(subtable, FORMAT8_GROUP_COUNT_START)?)
                    .map_err(|_| invalid_cmap("format 8 group count does not fit"))?;

                for index in 0..group_count {
                    let group = FORMAT8_GROUPS_START + index * GROUP_SIZE;
                    let start = read_u32(subtable, group)?;
                    let end = read_u32(subtable, group + 4)?;
                    let start_glyph = read_u32(subtable, group + 8)?;

                    for codepoint in start..=end {
                        if let Ok(code) = u16::try_from(codepoint)
                            && Self::format8_is_32(subtable, code)?
                        {
                            continue;
                        }
                        let glyph = start_glyph + (codepoint - start);
                        if let Ok(glyph) = u16::try_from(glyph)
                            && glyph < glyph_count
                        {
                            reverse.entry(glyph).or_insert(codepoint);
                        }
                    }
                }
            }
            CmapFormat::Format10 => {
                let start = read_u32(subtable, 12)?;
                let count = usize::try_from(read_u32(subtable, 16)?)
                    .map_err(|_| invalid_cmap("format 10 character count does not fit"))?;

                for index in 0..count {
                    let glyph = read_u16(subtable, 20 + index * 2)?;
                    if glyph < glyph_count {
                        reverse.entry(glyph).or_insert(start + index as u32);
                    }
                }
            }
            CmapFormat::Format12 => {
                let group_count = usize::try_from(read_u32(subtable, 12)?)
                    .map_err(|_| invalid_cmap("format 12 group count does not fit"))?;

                for index in 0..group_count {
                    let group = 16 + index * 12;
                    let start = read_u32(subtable, group)?;
                    let end = read_u32(subtable, group + 4)?;
                    let start_glyph = read_u32(subtable, group + 8)?;

                    for codepoint in start..=end {
                        let glyph = start_glyph + (codepoint - start);
                        if let Ok(glyph) = u16::try_from(glyph)
                            && glyph < glyph_count
                        {
                            reverse.entry(glyph).or_insert(codepoint);
                        }
                    }
                }
            }
            CmapFormat::Format13 => {
                let group_count = usize::try_from(read_u32(subtable, 12)?)
                    .map_err(|_| invalid_cmap("format 13 group count does not fit"))?;

                for index in 0..group_count {
                    let group = 16 + index * 12;
                    let start = read_u32(subtable, group)?;
                    let glyph = read_u32(subtable, group + 8)?;

                    // Format 13 maps a whole range to one glyph, so the group
                    // start is the smallest code that produces this glyph.
                    if let Ok(glyph) = u16::try_from(glyph)
                        && glyph < glyph_count
                    {
                        reverse
                            .entry(glyph)
                            .and_modify(|existing| *existing = (*existing).min(start))
                            .or_insert(start);
                    }
                }
            }
        }

        Ok(reverse)
    }
}
