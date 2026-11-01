//! 各 format 子表的码→字形查询：`Cmap::glyph_id` 按选中格式分发，
//! formats 0/2/4/6/8/10/12/13 的解码路径各自独立。

use super::raw::{invalid_cmap, read_u8, read_u16, read_u32};
use super::{
    Cmap, CmapFormat, FORMAT2_SUBHEADER_KEYS_START, FORMAT2_SUBHEADER_SIZE,
    FORMAT2_SUBHEADERS_START, FORMAT8_GROUP_COUNT_START, FORMAT8_GROUPS_START, FORMAT8_IS32_START,
    FontError, GROUP_SIZE,
};

impl Cmap {
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
    pub(super) fn format2_subheader_glyph(
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

    pub(super) fn format8_is_32(subtable: &[u8], code: u16) -> Result<bool, FontError> {
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
}
