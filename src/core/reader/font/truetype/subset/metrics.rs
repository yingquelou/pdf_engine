use std::ops::Range;

use super::MAX_SHORT_LOCA_OFFSET;
use super::directory::{read_i16, read_u16, read_u32};
use super::glyph::glyph_contour_count;
use super::invalid_subset;
use super::source::SourceFont;
use crate::core::reader::font::FontError;

impl<'a> SourceFont<'a> {
    fn horizontal_metric(&self, glyph_id: u16) -> Result<(u16, i16), FontError> {
        let metric_count = usize::from(self.horizontal_metric_count);
        let index = usize::from(glyph_id);
        let advance = if index < metric_count {
            read_u16(self.hmtx, index * 4)?
        } else {
            read_u16(self.hmtx, (metric_count - 1) * 4)?
        };
        let side_bearing = if index < metric_count {
            read_i16(self.hmtx, index * 4 + 2)?
        } else {
            read_i16(self.hmtx, metric_count * 4 + (index - metric_count) * 2)?
        };
        Ok((advance, side_bearing))
    }

    pub(super) fn build_hmtx(&self, order: &[u16]) -> Result<Vec<u8>, FontError> {
        let mut hmtx = Vec::with_capacity(order.len() * 4);
        for &glyph_id in order {
            let (advance, side_bearing) = self.horizontal_metric(glyph_id)?;
            hmtx.extend_from_slice(&advance.to_be_bytes());
            hmtx.extend_from_slice(&side_bearing.to_be_bytes());
        }
        Ok(hmtx)
    }

    fn vertical_metric(&self, glyph_id: u16) -> Result<(u16, i16), FontError> {
        let metric_count = self
            .vertical_metric_count
            .ok_or_else(|| invalid_subset("vertical metrics are missing"))?;
        read_vertical_metric(
            self.source_table(b"vmtx"),
            metric_count,
            usize::from(glyph_id),
        )
    }

    /// Height (`yMax - yMin`) of a retained glyph's `glyf` record.
    ///
    /// Empty glyphs (`loca` offsets equal) have no header and count as zero.
    /// Composite glyphs store their own bounds in the record header, so those
    /// bounds are used instead of the union of the component outlines.
    fn glyph_height(&self, glyph_id: u16) -> Result<i32, FontError> {
        let record = self.glyph_record(glyph_id)?;
        if record.is_empty() {
            return Ok(0);
        }
        // Validates the fixed ten-byte header with the same error path as the
        // glyph collection and outline rewriting code.
        glyph_contour_count(record)?;
        let y_min = i32::from(read_i16(record, 4)?);
        let y_max = i32::from(read_i16(record, 8)?);
        Ok(y_max - y_min)
    }

    /// Rebuild `vhea`/`vmtx` for the new glyph order.
    ///
    /// `numberOfVMetrics` becomes the smaller of the source count and the
    /// retained glyph count; the remaining glyphs keep only their top side
    /// bearing, exactly like the source layout. The `advanceHeightMax`,
    /// `minTopSideBearing`, `minBottomSideBearing`, and `yMaxExtent` summaries
    /// are recomputed from the rebuilt metrics and the retained glyph bounds,
    /// while the remaining `vhea` fields are copied from the source.
    pub(super) fn build_vertical_metrics(
        &self,
        order: &[u16],
    ) -> Result<(Vec<u8>, Vec<u8>), FontError> {
        let source_count = self
            .vertical_metric_count
            .ok_or_else(|| invalid_subset("vertical metrics are missing"))?;
        let retained_count =
            u16::try_from(order.len()).map_err(|_| invalid_subset("subset has too many glyphs"))?;
        let long_count = source_count.min(retained_count);

        let mut vmtx = Vec::with_capacity(order.len() * 4);
        for (index, &glyph_id) in order.iter().enumerate() {
            let (advance, side_bearing) = self.vertical_metric(glyph_id)?;
            if index < usize::from(long_count) {
                vmtx.extend_from_slice(&advance.to_be_bytes());
                vmtx.extend_from_slice(&side_bearing.to_be_bytes());
            } else {
                vmtx.extend_from_slice(&side_bearing.to_be_bytes());
            }
        }

        // Summaries cover the retained glyphs only. They are derived from the
        // rebuilt `vmtx`, so glyphs past the long metrics contribute the final
        // advance height that their trailing top side bearing reuses.
        let mut advance_height_max = 0u16;
        let mut min_top_side_bearing = i32::from(i16::MAX);
        let mut min_bottom_side_bearing = i32::from(i16::MAX);
        let mut y_max_extent = i32::from(i16::MIN);
        for (index, &glyph_id) in order.iter().enumerate() {
            let (advance, top_side_bearing) = read_vertical_metric(&vmtx, long_count, index)?;
            let height = self.glyph_height(glyph_id)?;
            let top = i32::from(top_side_bearing);
            advance_height_max = advance_height_max.max(advance);
            min_top_side_bearing = min_top_side_bearing.min(top);
            min_bottom_side_bearing =
                min_bottom_side_bearing.min(i32::from(advance) - top - height);
            y_max_extent = y_max_extent.max(top + height);
        }

        let mut vhea = self.source_table(b"vhea").to_vec();
        vhea[10..12].copy_from_slice(&advance_height_max.to_be_bytes());
        vhea[12..14].copy_from_slice(&clamp_to_i16(min_top_side_bearing).to_be_bytes());
        vhea[14..16].copy_from_slice(&clamp_to_i16(min_bottom_side_bearing).to_be_bytes());
        vhea[16..18].copy_from_slice(&clamp_to_i16(y_max_extent).to_be_bytes());
        vhea[34..36].copy_from_slice(&long_count.to_be_bytes());

        Ok((vhea, vmtx))
    }
}

/// Validate an optional `vhea`/`vmtx` pair and return `numberOfVMetrics`.
pub(super) fn validate_vertical_metrics(
    vhea: &[u8],
    vmtx: &[u8],
    glyph_count: u16,
) -> Result<u16, FontError> {
    if vhea.len() < 36 {
        return Err(invalid_subset("`vhea` table is truncated"));
    }
    let metric_count = read_u16(vhea, 34)?;
    if metric_count == 0 || metric_count > glyph_count {
        return Err(invalid_subset(
            "`vhea` table has an invalid numberOfVMetrics",
        ));
    }
    let required_length =
        usize::from(metric_count) * 4 + usize::from(glyph_count - metric_count) * 2;
    if vmtx.len() < required_length {
        return Err(invalid_subset("`vmtx` table is truncated"));
    }
    Ok(metric_count)
}

/// Read one `(advanceHeight, topSideBearing)` pair from `vmtx`.
///
/// Glyphs at or beyond `long_count` reuse the final long advance height and
/// take their top side bearing from the trailing array, mirroring the source
/// layout. `long_count` must be at least one.
fn read_vertical_metric(
    vmtx: &[u8],
    long_count: u16,
    index: usize,
) -> Result<(u16, i16), FontError> {
    let long_count = usize::from(long_count);
    debug_assert!(long_count >= 1, "long metrics were validated as non-empty");
    let advance = if index < long_count {
        read_u16(vmtx, index * 4)?
    } else {
        read_u16(vmtx, (long_count - 1) * 4)?
    };
    let side_bearing = if index < long_count {
        read_i16(vmtx, index * 4 + 2)?
    } else {
        read_i16(vmtx, long_count * 4 + (index - long_count) * 2)?
    };
    Ok((advance, side_bearing))
}

/// Saturate a computed summary value to the `i16` range of a `vhea` field.
fn clamp_to_i16(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(if value < 0 { i16::MIN } else { i16::MAX })
}

pub(super) fn parse_loca(
    loca: &[u8],
    glyph_count: u16,
    index_to_loc_format: i16,
) -> Result<Vec<Range<usize>>, FontError> {
    let entry_size = match index_to_loc_format {
        0 => 2,
        1 => 4,
        _ => return Err(invalid_subset("unsupported indexToLocFormat")),
    };
    let entry_count = usize::from(glyph_count) + 1;
    let required_length = entry_count
        .checked_mul(entry_size)
        .ok_or_else(|| invalid_subset("`loca` table length overflows"))?;
    if loca.len() < required_length {
        return Err(invalid_subset("`loca` table is truncated"));
    }

    let mut offsets = Vec::with_capacity(entry_count);
    let mut previous_offset = 0usize;
    for index in 0..entry_count {
        let offset = match index_to_loc_format {
            0 => usize::from(read_u16(loca, index * 2)?) * 2,
            1 => usize::try_from(read_u32(loca, index * 4)?)
                .map_err(|_| invalid_subset("glyph offset does not fit this platform"))?,
            _ => unreachable!("indexToLocFormat was validated"),
        };
        if offset < previous_offset {
            return Err(invalid_subset("`loca` offsets are not monotonic"));
        }
        offsets.push(offset);
        previous_offset = offset;
    }

    Ok(offsets.windows(2).map(|pair| pair[0]..pair[1]).collect())
}

pub(super) fn build_loca(
    offsets: &[usize],
    source_format: i16,
    glyf_length: usize,
) -> Result<(Vec<u8>, i16), FontError> {
    let use_short = source_format == 0
        && glyf_length <= MAX_SHORT_LOCA_OFFSET
        && offsets.iter().all(|offset| offset % 2 == 0);
    if use_short {
        let mut loca = Vec::with_capacity(offsets.len() * 2);
        for &offset in offsets {
            let short = u16::try_from(offset / 2)
                .map_err(|_| invalid_subset("short `loca` offset exceeds u16"))?;
            loca.extend_from_slice(&short.to_be_bytes());
        }
        Ok((loca, 0))
    } else {
        let mut loca = Vec::with_capacity(offsets.len() * 4);
        for &offset in offsets {
            let long = u32::try_from(offset)
                .map_err(|_| invalid_subset("long `loca` offset exceeds u32"))?;
            loca.extend_from_slice(&long.to_be_bytes());
        }
        Ok((loca, 1))
    }
}
