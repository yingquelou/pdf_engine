use crate::core::reader::font::FontError;

use super::GlyphPoint;
use super::error::invalid_glyph;
use super::metrics::{read_glyph_u16, read_i16};

pub(super) fn parse_simple_glyph(
    data: &[u8],
    contour_count: usize,
) -> Result<(Vec<u16>, Vec<GlyphPoint>, &[u8]), FontError> {
    let end_points_end = 10usize
        .checked_add(
            contour_count
                .checked_mul(2)
                .ok_or_else(|| invalid_glyph("contour count overflows"))?,
        )
        .ok_or_else(|| invalid_glyph("contour endpoint range overflows"))?;
    if end_points_end > data.len() {
        return Err(invalid_glyph("contour endpoints are truncated"));
    }

    let mut contour_end_points = Vec::with_capacity(contour_count);
    let mut previous_endpoint = None;
    for index in 0..contour_count {
        let endpoint = read_glyph_u16(data, 10 + index * 2)?;
        if previous_endpoint.is_some_and(|previous| endpoint <= previous) {
            return Err(invalid_glyph(
                "contour endpoint indices are not strictly increasing",
            ));
        }
        contour_end_points.push(endpoint);
        previous_endpoint = Some(endpoint);
    }
    let point_count = contour_end_points
        .last()
        .map_or(0, |endpoint| usize::from(*endpoint) + 1);

    let instruction_length = usize::from(read_glyph_u16(data, end_points_end)?);
    let instructions_start = end_points_end + 2;
    let instructions_end = instructions_start
        .checked_add(instruction_length)
        .ok_or_else(|| invalid_glyph("instruction range overflows"))?;
    let instructions = data
        .get(instructions_start..instructions_end)
        .ok_or_else(|| invalid_glyph("glyph instructions are truncated"))?;

    let mut flags = Vec::with_capacity(point_count);
    let mut cursor = instructions_end;
    while flags.len() < point_count {
        let flag = *data
            .get(cursor)
            .ok_or_else(|| invalid_glyph("point flags are truncated"))?;
        cursor += 1;
        if flag & 0x80 != 0 {
            return Err(invalid_glyph("point flag has its reserved bit set"));
        }
        flags.push(flag);
        if flag & 0x08 != 0 {
            let repeat = usize::from(
                *data
                    .get(cursor)
                    .ok_or_else(|| invalid_glyph("point flag repeat count is truncated"))?,
            );
            cursor += 1;
            let repeated_count = flags
                .len()
                .checked_add(repeat)
                .ok_or_else(|| invalid_glyph("point flag count overflows"))?;
            if repeated_count > point_count {
                return Err(invalid_glyph("point flag repeat exceeds point count"));
            }
            flags.resize(repeated_count, flag);
        }
    }

    let mut x_coordinates = Vec::with_capacity(point_count);
    let mut coordinate = 0i32;
    for flag in &flags {
        let delta = if flag & 0x02 != 0 {
            let magnitude = i32::from(
                *data
                    .get(cursor)
                    .ok_or_else(|| invalid_glyph("x coordinate deltas are truncated"))?,
            );
            cursor += 1;
            if flag & 0x10 != 0 {
                magnitude
            } else {
                -magnitude
            }
        } else if flag & 0x10 != 0 {
            0
        } else {
            let delta = read_i16(data, cursor)?;
            cursor += 2;
            i32::from(delta)
        };
        coordinate = coordinate
            .checked_add(delta)
            .ok_or_else(|| invalid_glyph("x coordinate accumulation overflows"))?;
        x_coordinates.push(
            i16::try_from(coordinate).map_err(|_| invalid_glyph("x coordinate exceeds i16"))?,
        );
    }

    let mut points = Vec::with_capacity(point_count);
    coordinate = 0;
    for (index, flag) in flags.iter().enumerate() {
        let delta = if flag & 0x04 != 0 {
            let magnitude = i32::from(
                *data
                    .get(cursor)
                    .ok_or_else(|| invalid_glyph("y coordinate deltas are truncated"))?,
            );
            cursor += 1;
            if flag & 0x20 != 0 {
                magnitude
            } else {
                -magnitude
            }
        } else if flag & 0x20 != 0 {
            0
        } else {
            let delta = read_i16(data, cursor)?;
            cursor += 2;
            i32::from(delta)
        };
        coordinate = coordinate
            .checked_add(delta)
            .ok_or_else(|| invalid_glyph("y coordinate accumulation overflows"))?;
        let y = i16::try_from(coordinate).map_err(|_| invalid_glyph("y coordinate exceeds i16"))?;
        points.push(GlyphPoint {
            x: x_coordinates[index],
            y,
            on_curve: flag & 0x01 != 0,
        });
    }

    Ok((contour_end_points, points, instructions))
}
