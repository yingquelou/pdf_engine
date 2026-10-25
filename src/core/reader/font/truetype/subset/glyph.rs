use std::collections::BTreeMap;

use super::directory::{read_i16, read_u16};
use super::invalid_subset;
use super::source::SourceFont;
use crate::core::reader::font::FontError;

impl<'a> SourceFont<'a> {
    pub(super) fn glyph_record(&self, glyph_id: u16) -> Result<&'a [u8], FontError> {
        let range = self
            .glyph_ranges
            .get(usize::from(glyph_id))
            .ok_or_else(|| invalid_subset("glyph ID is outside the font"))?;
        self.glyf
            .get(range.clone())
            .ok_or_else(|| invalid_subset("glyph record is outside the `glyf` table"))
    }

    pub(super) fn build_glyf(
        &self,
        order: &[u16],
        mapping: &BTreeMap<u16, u16>,
    ) -> Result<(Vec<u8>, Vec<usize>), FontError> {
        let mut glyf = Vec::new();
        let mut offsets = Vec::with_capacity(order.len() + 1);

        for &glyph_id in order {
            let record = self.glyph_record(glyph_id)?;
            if record.is_empty() {
                offsets.push(glyf.len());
                continue;
            }
            let record = if glyph_contour_count(record)? == -1 {
                let mut rewritten = record.to_vec();
                for (position, component) in composite_component_refs(record)? {
                    let new_glyph_id = mapping.get(&component).ok_or_else(|| {
                        invalid_subset("composite component is missing from the subset")
                    })?;
                    rewritten[position..position + 2].copy_from_slice(&new_glyph_id.to_be_bytes());
                }
                rewritten
            } else {
                record.to_vec()
            };
            offsets.push(glyf.len());
            glyf.extend_from_slice(&record);
        }
        offsets.push(glyf.len());
        Ok((glyf, offsets))
    }
}

pub(super) fn glyph_contour_count(record: &[u8]) -> Result<i16, FontError> {
    if record.len() < 10 {
        return Err(invalid_subset("glyph header is truncated"));
    }
    let contours = read_i16(record, 0)?;
    if contours < -1 {
        return Err(invalid_subset("glyph has an invalid contour count"));
    }
    Ok(contours)
}

/// Return `(glyphIndex offset, component glyph ID)` for a composite glyph.
pub(super) fn composite_component_refs(record: &[u8]) -> Result<Vec<(usize, u16)>, FontError> {
    let mut cursor = 10usize;
    let mut components = Vec::new();
    let mut flags;
    loop {
        flags = read_u16(record, cursor)?;
        let glyph_offset = cursor
            .checked_add(2)
            .ok_or_else(|| invalid_subset("composite glyph offset overflows"))?;
        let component = read_u16(record, glyph_offset)?;
        if flags & 0xE000 != 0 {
            return Err(invalid_subset("composite component has reserved flags set"));
        }
        if flags & 0x1800 == 0x1800 {
            return Err(invalid_subset(
                "composite component has conflicting offset flags",
            ));
        }
        let scale_flags = flags & 0x00C8;
        if scale_flags.count_ones() > 1 {
            return Err(invalid_subset(
                "composite component has conflicting transform flags",
            ));
        }
        advance(record, &mut cursor, 4)?;
        advance(record, &mut cursor, if flags & 0x0001 != 0 { 4 } else { 2 })?;
        advance(
            record,
            &mut cursor,
            match scale_flags {
                0 => 0,
                0x0008 => 2,
                0x0040 => 4,
                0x0080 => 8,
                _ => unreachable!("conflicting scale flags were rejected"),
            },
        )?;
        components.push((glyph_offset, component));
        if flags & 0x0020 == 0 {
            break;
        }
    }
    if flags & 0x0100 != 0 {
        let instruction_length = usize::from(read_u16(record, cursor)?);
        advance(
            record,
            &mut cursor,
            instruction_length
                .checked_add(2)
                .ok_or_else(|| invalid_subset("composite instruction range overflows"))?,
        )?;
    }
    Ok(components)
}

fn advance(data: &[u8], cursor: &mut usize, count: usize) -> Result<(), FontError> {
    let next = cursor
        .checked_add(count)
        .ok_or_else(|| invalid_subset("composite glyph offset overflows"))?;
    if next > data.len() {
        return Err(invalid_subset("composite glyph record is truncated"));
    }
    *cursor = next;
    Ok(())
}
