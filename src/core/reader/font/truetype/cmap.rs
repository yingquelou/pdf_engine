use std::ops::Range;

use super::super::FontError;

/// A supported TrueType Unicode character-map format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmapFormat {
    Format4,
    Format12,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cmap {
    subtable: Range<usize>,
    format: CmapFormat,
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

        let mut selected: Option<(u8, Range<usize>, CmapFormat)> = None;
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
            let length = match format {
                CmapFormat::Format4 => usize::from(read_u16(data, checked_add(offset, 2)?)?),
                CmapFormat::Format12 => {
                    usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                        .map_err(|_| invalid_cmap("format 12 length does not fit this platform"))?
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
                .is_none_or(|(existing_priority, _, _)| priority < *existing_priority)
            {
                selected = Some((priority, offset..end, format));
            }
        }
        let Some((_, subtable, format)) = selected else {
            return Err(invalid_cmap(
                "no supported Unicode format 4 or format 12 subtable",
            ));
        };
        let cmap = Self { subtable, format };
        cmap.validate(data, glyph_count)?;
        Ok(cmap)
    }

    pub fn format(&self) -> CmapFormat {
        self.format
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
            CmapFormat::Format4 => {
                let Ok(code) = u16::try_from(codepoint) else {
                    return Ok(None);
                };
                self.format4_glyph(subtable, code)?
            }
            CmapFormat::Format12 => self.format12_glyph(subtable, codepoint)?,
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
}

fn cmap_priority(platform: u16, encoding: u16, format: u16) -> Option<(u8, CmapFormat)> {
    let supported_format = match format {
        4 => CmapFormat::Format4,
        12 => CmapFormat::Format12,
        _ => return None,
    };
    let priority = match (platform, encoding, format) {
        (3, 10, 12) => 0,
        (0, _, 12) => 1,
        (3, 1, 4) => 2,
        (0, _, 4) => 3,
        _ => return None,
    };
    Some((priority, supported_format))
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
    use super::{Cmap, CmapFormat};

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

    #[test]
    fn maps_bmp_codepoints_with_format4() {
        let cmap_data = unicode_cmap(format4_table(), 4);
        let parsed = Cmap::parse(&cmap_data, 8).unwrap();
        assert_eq!(parsed.format(), CmapFormat::Format4);
        assert_eq!(parsed.glyph_id(&cmap_data, 0x41, 8).unwrap(), Some(3));
        assert_eq!(parsed.glyph_id(&cmap_data, 0x42, 8).unwrap(), None);
        assert_eq!(parsed.glyph_id(&cmap_data, 0x1F600, 8).unwrap(), None);
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
        let mut data = vec![0; 12 + subtable.len()];
        data[2..4].copy_from_slice(&1u16.to_be_bytes());
        data[4..6].copy_from_slice(&3u16.to_be_bytes());
        data[6..8].copy_from_slice(&(if format == 12 { 10u16 } else { 1u16 }).to_be_bytes());
        data[8..12].copy_from_slice(&12u32.to_be_bytes());
        data[12..].copy_from_slice(&subtable);
        data
    }
}
