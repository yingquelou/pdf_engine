use crate::core::reader::font::FontError;

use super::error::MAX_EXPANDED_POINTS;
use super::error::invalid_glyph;
use super::metrics::{read_glyph_i16, read_glyph_u16, read_i8, read_i16};
use super::{GlyphLocations, GlyphPoint};

impl GlyphLocations {
    pub(super) fn parse_composite_glyph<'a>(
        &self,
        glyf: &'a [u8],
        data: &'a [u8],
        component_stack: &mut Vec<u16>,
    ) -> Result<(Vec<u16>, Vec<GlyphPoint>, &'a [u8]), FontError> {
        let mut cursor = 10usize;
        let mut contour_end_points = Vec::new();
        let mut points: Vec<GlyphPoint> = Vec::new();
        let mut flags;

        loop {
            flags = read_glyph_u16(data, cursor)?;
            let component_id = read_glyph_u16(data, cursor + 2)?;
            cursor += 4;
            if flags & 0xE000 != 0 {
                return Err(invalid_glyph("composite component has reserved flags set"));
            }
            if usize::from(component_id) >= self.glyph_ranges.len() {
                return Err(invalid_glyph(
                    "composite component references an unknown glyph",
                ));
            }
            let arg_words = flags & 0x0001 != 0;
            let args_are_xy = flags & 0x0002 != 0;
            let (arg1, arg2) = match (arg_words, args_are_xy) {
                (true, true) => {
                    let x = read_i16(data, cursor)?;
                    let y = read_i16(data, cursor + 2)?;
                    cursor += 4;
                    (i32::from(x), i32::from(y))
                }
                (true, false) => {
                    let first = i32::from(read_glyph_u16(data, cursor)?);
                    let second = i32::from(read_glyph_u16(data, cursor + 2)?);
                    cursor += 4;
                    (first, second)
                }
                (false, true) => {
                    let x = i32::from(read_i8(data, cursor)?);
                    let y = i32::from(read_i8(data, cursor + 1)?);
                    cursor += 2;
                    (x, y)
                }
                (false, false) => {
                    let first = i32::from(*data.get(cursor).ok_or_else(|| {
                        invalid_glyph("composite component arguments are truncated")
                    })?);
                    let second = i32::from(*data.get(cursor + 1).ok_or_else(|| {
                        invalid_glyph("composite component arguments are truncated")
                    })?);
                    cursor += 2;
                    (first, second)
                }
            };

            let transform = read_component_transform(data, &mut cursor, flags)?;
            let Some(component) = self.resolve_glyph(glyf, component_id, component_stack)? else {
                if !args_are_xy {
                    return Err(invalid_glyph(
                        "composite point attachment references an empty glyph",
                    ));
                }
                if flags & 0x0020 == 0 {
                    break;
                }
                continue;
            };
            let transformed_points = component
                .points
                .iter()
                .map(|point| transform_point(point, transform))
                .collect::<Result<Vec<_>, _>>()?;
            let (x_offset, y_offset) = if args_are_xy {
                let (x, y) = if flags & 0x0800 != 0 {
                    transform_vector(arg1, arg2, transform)?
                } else {
                    (arg1, arg2)
                };
                (x, y)
            } else {
                let parent_point = points
                    .get(
                        usize::try_from(arg1).map_err(|_| {
                            invalid_glyph("composite parent point index is invalid")
                        })?,
                    )
                    .ok_or_else(|| invalid_glyph("composite parent point index is out of range"))?;
                let child_point = transformed_points
                    .get(
                        usize::try_from(arg2)
                            .map_err(|_| invalid_glyph("composite child point index is invalid"))?,
                    )
                    .ok_or_else(|| invalid_glyph("composite child point index is out of range"))?;
                (
                    i32::from(parent_point.x) - i32::from(child_point.x),
                    i32::from(parent_point.y) - i32::from(child_point.y),
                )
            };

            let point_offset = points.len();
            let new_point_count = point_offset
                .checked_add(transformed_points.len())
                .ok_or_else(|| invalid_glyph("expanded composite point count overflows"))?;
            if new_point_count > MAX_EXPANDED_POINTS {
                return Err(invalid_glyph(
                    "expanded composite glyph has too many points",
                ));
            }
            for point in transformed_points {
                points.push(GlyphPoint {
                    x: i16::try_from(i32::from(point.x) + x_offset)
                        .map_err(|_| invalid_glyph("composite x coordinate exceeds i16"))?,
                    y: i16::try_from(i32::from(point.y) + y_offset)
                        .map_err(|_| invalid_glyph("composite y coordinate exceeds i16"))?,
                    on_curve: point.on_curve,
                });
            }
            for endpoint in component.contour_end_points {
                let adjusted = point_offset
                    .checked_add(usize::from(endpoint))
                    .ok_or_else(|| invalid_glyph("composite contour endpoint overflows"))?;
                contour_end_points.push(
                    u16::try_from(adjusted)
                        .map_err(|_| invalid_glyph("composite contour endpoint exceeds u16"))?,
                );
            }

            if flags & 0x0020 == 0 {
                break;
            }
        }

        let instructions = if flags & 0x0100 != 0 {
            let length = usize::from(read_glyph_u16(data, cursor)?);
            cursor += 2;
            let end = cursor
                .checked_add(length)
                .ok_or_else(|| invalid_glyph("composite instruction range overflows"))?;
            data.get(cursor..end)
                .ok_or_else(|| invalid_glyph("composite instructions are truncated"))?
        } else {
            &data[10..10]
        };

        Ok((contour_end_points, points, instructions))
    }
}

#[derive(Clone, Copy)]
struct ComponentTransform {
    xx: i16,
    xy: i16,
    yx: i16,
    yy: i16,
}

fn read_component_transform(
    data: &[u8],
    cursor: &mut usize,
    flags: u16,
) -> Result<ComponentTransform, FontError> {
    let scale_flags = flags & 0x00C8;
    if scale_flags.count_ones() > 1 {
        return Err(invalid_glyph(
            "composite component has conflicting transform flags",
        ));
    }
    if flags & 0x1800 == 0x1800 {
        return Err(invalid_glyph(
            "composite component has conflicting offset flags",
        ));
    }
    let mut transform = ComponentTransform {
        xx: 0x4000,
        xy: 0,
        yx: 0,
        yy: 0x4000,
    };
    match scale_flags {
        0 => {}
        0x0008 => {
            let scale = read_glyph_i16(data, *cursor)?;
            *cursor += 2;
            transform.xx = scale;
            transform.yy = scale;
        }
        0x0040 => {
            transform.xx = read_glyph_i16(data, *cursor)?;
            transform.yy = read_glyph_i16(data, *cursor + 2)?;
            *cursor += 4;
        }
        0x0080 => {
            transform.xx = read_glyph_i16(data, *cursor)?;
            transform.xy = read_glyph_i16(data, *cursor + 2)?;
            transform.yx = read_glyph_i16(data, *cursor + 4)?;
            transform.yy = read_glyph_i16(data, *cursor + 6)?;
            *cursor += 8;
        }
        _ => unreachable!(),
    }
    Ok(transform)
}

fn transform_vector(
    x: i32,
    y: i32,
    transform: ComponentTransform,
) -> Result<(i32, i32), FontError> {
    let transformed_x =
        i64::from(x) * i64::from(transform.xx) + i64::from(y) * i64::from(transform.xy);
    let transformed_y =
        i64::from(x) * i64::from(transform.yx) + i64::from(y) * i64::from(transform.yy);
    Ok((round_f2dot14(transformed_x)?, round_f2dot14(transformed_y)?))
}

fn transform_point(
    point: &GlyphPoint,
    transform: ComponentTransform,
) -> Result<GlyphPoint, FontError> {
    let (x, y) = transform_vector(i32::from(point.x), i32::from(point.y), transform)?;
    Ok(GlyphPoint {
        x: i16::try_from(x).map_err(|_| invalid_glyph("transformed x coordinate exceeds i16"))?,
        y: i16::try_from(y).map_err(|_| invalid_glyph("transformed y coordinate exceeds i16"))?,
        on_curve: point.on_curve,
    })
}

fn round_f2dot14(value: i64) -> Result<i32, FontError> {
    let rounded = if value < 0 {
        value.checked_sub(0x2000)
    } else {
        value.checked_add(0x2000)
    }
    .ok_or_else(|| invalid_glyph("composite transform overflows"))?
        / 0x4000;
    i32::try_from(rounded).map_err(|_| invalid_glyph("composite transform exceeds i32"))
}
