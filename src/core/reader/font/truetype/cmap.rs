use std::collections::BTreeMap;
use std::ops::Range;

use super::super::FontError;

/// A supported TrueType character-map format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmapFormat {
    Format0,
    Format2,
    Format4,
    Format6,
    Format8,
    Format10,
    Format12,
    Format13,
}

/// First byte of the `subHeaderKeys[256]` array in a format 2 subtable.
const FORMAT2_SUBHEADER_KEYS_START: usize = 6;
/// First byte of the `subHeaders[]` array in a format 2 subtable.
const FORMAT2_SUBHEADERS_START: usize = FORMAT2_SUBHEADER_KEYS_START + 256 * 2;
/// Size of one format 2 `SubHeader` record.
const FORMAT2_SUBHEADER_SIZE: usize = 8;

/// Byte offset of the `is32[8192]` bitmap in a format 8 subtable.
const FORMAT8_IS32_START: usize = 12;
/// Byte offset of `nGroups` in a format 8 subtable.
const FORMAT8_GROUP_COUNT_START: usize = FORMAT8_IS32_START + 8192;
/// First byte of the group array in a format 8 subtable.
const FORMAT8_GROUPS_START: usize = FORMAT8_GROUP_COUNT_START + 4;
/// Size of one format 8/12/13 group record.
const GROUP_SIZE: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cmap {
    subtable: Range<usize>,
    format: CmapFormat,
    symbolic: bool,
}

impl Cmap {
    pub fn parse(data: &[u8], glyph_count: u16) -> Result<Self, FontError> {
        if data.len() < 4 {
            return Err(invalid_cmap("header is truncated"));
        }
        if read_u16(data, 0)? != 0 {
            return Err(invalid_cmap("unsupported cmap table version"));
        }
        let record_count = usize::from(read_u16(data, 2)?);
        let records_end = 4usize
            .checked_add(
                record_count
                    .checked_mul(8)
                    .ok_or_else(|| invalid_cmap("encoding record count overflows"))?,
            )
            .ok_or_else(|| invalid_cmap("encoding record range overflows"))?;
        if record_count == 0 || records_end > data.len() {
            return Err(invalid_cmap("encoding records are truncated or empty"));
        }

        let mut selected: Option<(u8, Range<usize>, CmapFormat, bool)> = None;
        for index in 0..record_count {
            let record = 4 + index * 8;
            let platform = read_u16(data, record)?;
            let encoding = read_u16(data, record + 2)?;
            let offset = usize::try_from(read_u32(data, record + 4)?)
                .map_err(|_| invalid_cmap("subtable offset does not fit this platform"))?;
            let format = read_u16(data, offset)?;
            let Some((priority, format)) = cmap_priority(platform, encoding, format) else {
                continue;
            };
            let symbolic = (platform, encoding) == (3, 0);
            let length = match format {
                CmapFormat::Format0 => 262,
                CmapFormat::Format2 => usize::from(read_u16(data, checked_add(offset, 2)?)?),
                CmapFormat::Format4 => usize::from(read_u16(data, checked_add(offset, 2)?)?),
                CmapFormat::Format6 => usize::from(read_u16(data, checked_add(offset, 2)?)?),
                CmapFormat::Format8 => usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                    .map_err(|_| invalid_cmap("format 8 length does not fit this platform"))?,
                CmapFormat::Format10 => {
                    usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                        .map_err(|_| invalid_cmap("format 10 length does not fit this platform"))?
                }
                CmapFormat::Format12 => {
                    usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                        .map_err(|_| invalid_cmap("format 12 length does not fit this platform"))?
                }
                CmapFormat::Format13 => {
                    usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                        .map_err(|_| invalid_cmap("format 13 length does not fit this platform"))?
                }
            };
            let end = offset
                .checked_add(length)
                .ok_or_else(|| invalid_cmap("subtable range overflows"))?;
            if offset < records_end || end > data.len() {
                return Err(invalid_cmap("subtable range is outside the cmap table"));
            }
            if selected
                .as_ref()
                .is_none_or(|(existing_priority, _, _, _)| priority < *existing_priority)
            {
                selected = Some((priority, offset..end, format, symbolic));
            }
        }
        let Some((_, subtable, format, symbolic)) = selected else {
            return Err(invalid_cmap(
                "no supported cmap format 0, 2, 4, 6, 8, 10, 12, or 13 subtable",
            ));
        };
        let cmap = Self {
            subtable,
            format,
            symbolic,
        };
        cmap.validate(data, glyph_count)?;
        Ok(cmap)
    }

    pub fn format(&self) -> CmapFormat {
        self.format
    }

    /// Return whether the selected subtable is the Microsoft Symbol (3, 0)
    /// encoding, whose codes commonly occupy `0xF000..=0xF0FF`.
    pub(super) fn is_symbolic(&self) -> bool {
        self.symbolic
    }

    pub fn glyph_id(
        &self,
        cmap_data: &[u8],
        codepoint: u32,
        glyph_count: u16,
    ) -> Result<Option<u16>, FontError> {
        let subtable = cmap_data
            .get(self.subtable.clone())
            .ok_or_else(|| invalid_cmap("selected subtable is outside the cmap table"))?;
        let glyph = match self.format {
            CmapFormat::Format0 => self.format0_glyph(subtable, codepoint)?,
            CmapFormat::Format2 => self.format2_glyph(subtable, codepoint)?,
            CmapFormat::Format4 => {
                let Ok(code) = u16::try_from(codepoint) else {
                    return Ok(None);
                };
                self.format4_glyph(subtable, code)?
            }
            CmapFormat::Format6 => self.format6_glyph(subtable, codepoint)?,
            CmapFormat::Format8 => self.format8_glyph(subtable, codepoint)?,
            CmapFormat::Format10 => self.format10_glyph(subtable, codepoint)?,
            CmapFormat::Format12 => self.format12_glyph(subtable, codepoint)?,
            CmapFormat::Format13 => self.format13_glyph(subtable, codepoint)?,
        };
        let Some(glyph) = glyph else {
            return Ok(None);
        };
        if glyph >= glyph_count {
            return Err(invalid_cmap("character maps to an invalid glyph ID"));
        }
        Ok(Some(glyph))
    }

    fn validate(&self, cmap_data: &[u8], glyph_count: u16) -> Result<(), FontError> {
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

    fn format4_glyph(&self, subtable: &[u8], code: u16) -> Result<Option<u16>, FontError> {
        let segment_count = usize::from(read_u16(subtable, 6)?) / 2;
        let end_codes_start = 14;
        let start_codes_start = end_codes_start + segment_count * 2 + 2;
        let deltas_start = start_codes_start + segment_count * 2;
        let range_offsets_start = deltas_start + segment_count * 2;
        for index in 0..segment_count {
            let end_code = read_u16(subtable, end_codes_start + index * 2)?;
            if code > end_code {
                continue;
            }
            let start_code = read_u16(subtable, start_codes_start + index * 2)?;
            if code < start_code {
                return Ok(None);
            }
            let delta = read_u16(subtable, deltas_start + index * 2)?;
            let range_offset_position = range_offsets_start + index * 2;
            let range_offset = read_u16(subtable, range_offset_position)?;
            let glyph = if range_offset == 0 {
                code.wrapping_add(delta)
            } else {
                let glyph_position = range_offset_position
                    .checked_add(usize::from(range_offset))
                    .and_then(|position| position.checked_add(usize::from(code - start_code) * 2))
                    .ok_or_else(|| invalid_cmap("format 4 glyph offset overflows"))?;
                let glyph = read_u16(subtable, glyph_position)?;
                if glyph == 0 {
                    0
                } else {
                    glyph.wrapping_add(delta)
                }
            };
            return Ok(Some(glyph));
        }
        Ok(None)
    }

    fn format2_glyph(&self, subtable: &[u8], codepoint: u32) -> Result<Option<u16>, FontError> {
        let Ok(code) = u16::try_from(codepoint) else {
            return Ok(None);
        };
        let high = usize::from(code >> 8);
        let subheader = if high == 0 {
            0
        } else {
            let key = usize::from(read_u16(subtable, FORMAT2_SUBHEADER_KEYS_START + high * 2)?);
            if key == 0 {
                return Ok(None);
            }
            key / FORMAT2_SUBHEADER_SIZE
        };
        Self::format2_subheader_glyph(subtable, subheader, code & 0x00FF)
    }

    /// Resolve a byte-sized character code through one format 2 subHeader.
    ///
    /// The glyph position is `&idRangeOffset + idRangeOffset + 2 * (code -
    /// firstCode)`, where `&idRangeOffset` is the byte position of the
    /// subHeader's own `idRangeOffset` field.
    fn format2_subheader_glyph(
        subtable: &[u8],
        subheader: usize,
        char_code: u16,
    ) -> Result<Option<u16>, FontError> {
        let record = FORMAT2_SUBHEADERS_START + subheader * FORMAT2_SUBHEADER_SIZE;
        let first_code = u32::from(read_u16(subtable, record)?);
        let entry_count = u32::from(read_u16(subtable, record + 2)?);
        let id_delta = read_u16(subtable, record + 4)?;
        let range_offset = usize::from(read_u16(subtable, record + 6)?);
        let code = u32::from(char_code);
        if code < first_code || code >= first_code + entry_count {
            return Ok(None);
        }
        let glyph_offset = record
            .checked_add(6)
            .and_then(|position| position.checked_add(range_offset))
            .and_then(|position| position.checked_add((code - first_code) as usize * 2))
            .ok_or_else(|| invalid_cmap("format 2 glyph offset overflows"))?;
        let glyph = read_u16(subtable, glyph_offset)?;
        let glyph = if glyph == 0 {
            0
        } else {
            glyph.wrapping_add(id_delta)
        };
        Ok(Some(glyph))
    }

    fn format0_glyph(&self, subtable: &[u8], codepoint: u32) -> Result<Option<u16>, FontError> {
        if codepoint > 255 {
            return Ok(None);
        }
        if subtable.len() < 262 {
            return Err(invalid_cmap("format 0 subtable is truncated"));
        }
        let glyph = subtable[6 + codepoint as usize];
        Ok(Some(u16::from(glyph)))
    }

    fn format6_glyph(&self, subtable: &[u8], codepoint: u32) -> Result<Option<u16>, FontError> {
        if subtable.len() < 10 {
            return Err(invalid_cmap("format 6 subtable is truncated"));
        }
        let first_code = u32::from(read_u16(subtable, 6)?);
        let entry_count = usize::from(read_u16(subtable, 8)?);
        if codepoint < first_code || codepoint >= first_code + entry_count as u32 {
            return Ok(None);
        }
        let index = (codepoint - first_code) as usize;
        let glyph_offset = 10 + index * 2;
        if glyph_offset + 2 > subtable.len() {
            return Err(invalid_cmap("format 6 glyph array is truncated"));
        }
        let glyph = read_u16(subtable, glyph_offset)?;
        Ok(Some(glyph))
    }

    /// Look up a code through a format 8 mixed 16/32-bit subtable.
    ///
    /// A 16-bit value is mapped directly only when its `is32` bit is clear;
    /// marked values are the high half of a 32-bit code. Groups then map the
    /// resulting character code to a glyph ID.
    fn format8_glyph(&self, subtable: &[u8], codepoint: u32) -> Result<Option<u16>, FontError> {
        if let Ok(code) = u16::try_from(codepoint)
            && Self::format8_is_32(subtable, code)?
        {
            return Ok(None);
        }
        let group_count = usize::try_from(read_u32(subtable, FORMAT8_GROUP_COUNT_START)?)
            .map_err(|_| invalid_cmap("format 8 group count does not fit this platform"))?;
        let mut low = 0;
        let mut high = group_count;
        while low < high {
            let middle = low + (high - low) / 2;
            let group = FORMAT8_GROUPS_START + middle * GROUP_SIZE;
            let start = read_u32(subtable, group)?;
            let end = read_u32(subtable, group + 4)?;
            if codepoint < start {
                high = middle;
            } else if codepoint > end {
                low = middle + 1;
            } else {
                let start_glyph = read_u32(subtable, group + 8)?;
                let glyph = start_glyph + (codepoint - start);
                let glyph = u16::try_from(glyph)
                    .map_err(|_| invalid_cmap("glyph ID does not fit TrueType glyph range"))?;
                return Ok(Some(glyph));
            }
        }
        Ok(None)
    }

    fn format8_is_32(subtable: &[u8], code: u16) -> Result<bool, FontError> {
        let byte = read_u8(subtable, FORMAT8_IS32_START + (usize::from(code) >> 3))?;
        Ok(byte & (1 << (code % 8)) != 0)
    }

    fn format10_glyph(&self, subtable: &[u8], codepoint: u32) -> Result<Option<u16>, FontError> {
        let start = read_u32(subtable, 12)?;
        let count = read_u32(subtable, 16)?;
        if codepoint < start || codepoint >= start.saturating_add(count) {
            return Ok(None);
        }
        let index = usize::try_from(codepoint - start)
            .map_err(|_| invalid_cmap("format 10 character index does not fit this platform"))?;
        Ok(Some(read_u16(subtable, 20 + index * 2)?))
    }

    fn format12_glyph(&self, subtable: &[u8], codepoint: u32) -> Result<Option<u16>, FontError> {
        let group_count = usize::try_from(read_u32(subtable, 12)?)
            .map_err(|_| invalid_cmap("format 12 group count does not fit this platform"))?;
        let mut low = 0;
        let mut high = group_count;
        while low < high {
            let middle = low + (high - low) / 2;
            let group = 16 + middle * 12;
            let start = read_u32(subtable, group)?;
            let end = read_u32(subtable, group + 4)?;
            if codepoint < start {
                high = middle;
            } else if codepoint > end {
                low = middle + 1;
            } else {
                let start_glyph = read_u32(subtable, group + 8)?;
                let glyph = start_glyph + (codepoint - start);
                let glyph = u16::try_from(glyph)
                    .map_err(|_| invalid_cmap("glyph ID does not fit TrueType glyph range"))?;
                return Ok(Some(glyph));
            }
        }
        Ok(None)
    }

    fn format13_glyph(&self, subtable: &[u8], codepoint: u32) -> Result<Option<u16>, FontError> {
        let group_count = usize::try_from(read_u32(subtable, 12)?)
            .map_err(|_| invalid_cmap("format 13 group count does not fit this platform"))?;
        let mut low = 0;
        let mut high = group_count;
        while low < high {
            let middle = low + (high - low) / 2;
            let group = 16 + middle * 12;
            let start = read_u32(subtable, group)?;
            let end = read_u32(subtable, group + 4)?;
            if codepoint < start {
                high = middle;
            } else if codepoint > end {
                low = middle + 1;
            } else {
                let glyph = read_u32(subtable, group + 8)?;
                let glyph = u16::try_from(glyph)
                    .map_err(|_| invalid_cmap("glyph ID does not fit TrueType glyph range"))?;
                return Ok(Some(glyph));
            }
        }
        Ok(None)
    }

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

fn cmap_priority(platform: u16, encoding: u16, format: u16) -> Option<(u8, CmapFormat)> {
    let supported_format = match format {
        0 => CmapFormat::Format0,
        2 => CmapFormat::Format2,
        4 => CmapFormat::Format4,
        6 => CmapFormat::Format6,
        8 => CmapFormat::Format8,
        10 => CmapFormat::Format10,
        12 => CmapFormat::Format12,
        13 => CmapFormat::Format13,
        _ => return None,
    };
    let priority = match (platform, encoding, format) {
        (3, 10, 12) => 0,
        // Formats 8 and 10 cover the full repertoire like format 12 but are
        // legacy Apple encodings, so they rank just below it.
        (3, 10, 8) => 1,
        (3, 10, 10) => 2,
        // Format 13 follows the same platform rules as format 12 but is a
        // many-to-one last-resort map, so it ranks below the one-to-one maps.
        (3, 10, 13) => 3,
        (0, _, 12) => 4,
        (0, _, 8) => 5,
        (0, _, 10) => 6,
        (0, _, 13) => 7,
        (3, 1, 4) => 8,
        (0, _, 4) => 9,
        // Format 2 is a legacy high-byte map; prefer the full BMP map 4.
        (3, 1, 2) => 10,
        (0, _, 2) => 11,
        (3, 1, 0) => 12,
        (1, 0, 0) => 13,
        (0, _, 0) => 14,
        (3, 1, 6) => 15,
        (1, 0, 6) => 16,
        (0, _, 6) => 17,
        // Microsoft Symbol (3, 0) subtables rank below every Unicode subtable;
        // they are used only when no Unicode mapping is available. Their
        // relative order is unchanged.
        (3, 0, 12) => 18,
        (3, 0, 13) => 19,
        (3, 0, 8) => 20,
        (3, 0, 10) => 21,
        (3, 0, 4) => 22,
        (3, 0, 0) => 23,
        (3, 0, 6) => 24,
        _ => return None,
    };
    Some((priority, supported_format))
}

fn read_u8(data: &[u8], offset: usize) -> Result<u8, FontError> {
    data.get(offset)
        .copied()
        .ok_or_else(|| invalid_cmap("unexpected end of cmap data"))
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let bytes = data
        .get(offset..checked_add(offset, 2)?)
        .ok_or_else(|| invalid_cmap("unexpected end of cmap data"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let bytes = data
        .get(offset..checked_add(offset, 4)?)
        .ok_or_else(|| invalid_cmap("unexpected end of cmap data"))?;
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn checked_add(offset: usize, length: usize) -> Result<usize, FontError> {
    offset
        .checked_add(length)
        .ok_or_else(|| invalid_cmap("cmap offset overflows"))
}

fn invalid_cmap(message: &str) -> FontError {
    FontError::Invalid(format!("invalid TrueType cmap: {message}"))
}

#[cfg(test)]
mod tests {
    use super::{
        Cmap, CmapFormat, FORMAT2_SUBHEADER_SIZE, FORMAT2_SUBHEADERS_START,
        FORMAT8_GROUP_COUNT_START, FORMAT8_GROUPS_START, FORMAT8_IS32_START, GROUP_SIZE,
    };

    fn format4_table() -> Vec<u8> {
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
    fn format2_table() -> Vec<u8> {
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

    fn format12_table() -> Vec<u8> {
        let mut data = vec![0; 28];
        data[..2].copy_from_slice(&12u16.to_be_bytes());
        data[4..8].copy_from_slice(&28u32.to_be_bytes());
        data[12..16].copy_from_slice(&1u32.to_be_bytes());
        data[16..20].copy_from_slice(&0x1F600u32.to_be_bytes());
        data[20..24].copy_from_slice(&0x1F600u32.to_be_bytes());
        data[24..28].copy_from_slice(&4u32.to_be_bytes());
        data
    }

    fn format13_table(groups: &[(u32, u32, u32)]) -> Vec<u8> {
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
    fn format8_table(is32_codes: &[u16], groups: &[(u32, u32, u32)]) -> Vec<u8> {
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

    fn format10_table(start: u32, glyphs: &[u16]) -> Vec<u8> {
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
    fn symbolic_format4_table(glyph: u16) -> Vec<u8> {
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
        truncated[FORMAT8_GROUP_COUNT_START..FORMAT8_GROUPS_START]
            .copy_from_slice(&2u32.to_be_bytes());
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

    fn unicode_cmap(subtable: Vec<u8>, format: u16) -> Vec<u8> {
        let encoding = if matches!(format, 8 | 10 | 12 | 13) {
            10
        } else {
            1
        };
        records_cmap(subtable, 3, encoding)
    }

    fn records_cmap(subtable: Vec<u8>, platform: u16, encoding: u16) -> Vec<u8> {
        let mut data = vec![0; 12 + subtable.len()];
        data[2..4].copy_from_slice(&1u16.to_be_bytes());
        data[4..6].copy_from_slice(&platform.to_be_bytes());
        data[6..8].copy_from_slice(&encoding.to_be_bytes());
        data[8..12].copy_from_slice(&12u32.to_be_bytes());
        data[12..].copy_from_slice(&subtable);
        data
    }

    fn multi_records_cmap(records: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
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
}
