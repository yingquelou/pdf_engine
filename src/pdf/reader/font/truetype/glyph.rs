use std::ops::Range;

use super::super::FontError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphPoint {
    pub x: i16,
    pub y: i16,
    pub on_curve: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrueTypeGlyph<'a> {
    /// Number of contours, or -1 for a composite glyph.
    pub number_of_contours: i16,
    /// Glyph bounds in font units: xMin, yMin, xMax, yMax.
    pub bounds: [i16; 4],
    /// The complete glyph record, including its 10-byte header.
    pub data: &'a [u8],
    /// End-point point indices for each simple-glyph contour.
    pub contour_end_points: Vec<u16>,
    /// Decoded simple-glyph points. Composite glyph components are not expanded.
    pub points: Vec<GlyphPoint>,
    /// TrueType hinting instructions for a simple glyph.
    pub instructions: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GlyphLocations {
    glyph_ranges: Vec<Range<usize>>,
}

impl GlyphLocations {
    pub fn parse(
        loca: &[u8],
        glyf: &[u8],
        glyph_count: u16,
        index_to_loc_format: i16,
    ) -> Result<Self, FontError> {
        let entry_count = usize::from(glyph_count) + 1;
        let entry_size = match index_to_loc_format {
            0 => 2,
            1 => 4,
            _ => return Err(invalid_glyph("unsupported indexToLocFormat")),
        };
        let required_length = entry_count
            .checked_mul(entry_size)
            .ok_or_else(|| invalid_glyph("loca table length overflows"))?;
        if loca.len() < required_length {
            return Err(invalid_glyph("loca table is truncated"));
        }

        let mut glyph_ranges = Vec::with_capacity(usize::from(glyph_count));
        let mut previous_offset = 0usize;
        for index in 0..entry_count {
            let offset = match index_to_loc_format {
                0 => usize::from(read_loca_u16(loca, index * 2)?) * 2,
                1 => usize::try_from(read_u32(loca, index * 4)?)
                    .map_err(|_| invalid_glyph("glyph offset does not fit this platform"))?,
                _ => unreachable!(),
            };
            if offset < previous_offset {
                return Err(invalid_glyph("loca offsets are not monotonic"));
            }
            if offset > glyf.len() {
                return Err(invalid_glyph("loca offset exceeds the glyf table"));
            }
            if index > 0 {
                glyph_ranges.push(previous_offset..offset);
            }
            previous_offset = offset;
        }

        Ok(Self { glyph_ranges })
    }

    pub fn glyph<'a>(
        &self,
        glyf: &'a [u8],
        glyph_id: u16,
    ) -> Result<Option<TrueTypeGlyph<'a>>, FontError> {
        let Some(range) = self.glyph_ranges.get(usize::from(glyph_id)) else {
            return Ok(None);
        };
        if range.is_empty() {
            return Ok(None);
        }
        let data = glyf
            .get(range.clone())
            .ok_or_else(|| invalid_glyph("glyph range is outside the glyf table"))?;
        if data.len() < 10 {
            return Err(invalid_glyph("glyph header is truncated"));
        }
        let number_of_contours =
            i16::from_be_bytes(data[..2].try_into().expect("fixed-width slice"));
        if number_of_contours < -1 {
            return Err(invalid_glyph("glyph has an invalid contour count"));
        }
        let bounds = [
            read_i16(data, 2)?,
            read_i16(data, 4)?,
            read_i16(data, 6)?,
            read_i16(data, 8)?,
        ];
        if bounds[0] > bounds[2] || bounds[1] > bounds[3] {
            return Err(invalid_glyph("glyph bounds are inverted"));
        }
        let (contour_end_points, points, instructions) = if number_of_contours >= 0 {
            parse_simple_glyph(data, number_of_contours as usize)?
        } else {
            (Vec::new(), Vec::new(), &data[10..10])
        };
        Ok(Some(TrueTypeGlyph {
            number_of_contours,
            bounds,
            data,
            contour_end_points,
            points,
            instructions,
        }))
    }
}

fn parse_simple_glyph(
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

fn read_loca_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_glyph("loca table is truncated"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn read_glyph_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_glyph("glyph data is truncated"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_glyph("loca table is truncated"))?;
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_glyph("glyph data is truncated"))?;
    Ok(i16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn invalid_glyph(message: &str) -> FontError {
    FontError::Invalid(format!("invalid TrueType glyph data: {message}"))
}

#[cfg(test)]
mod tests {
    use super::GlyphLocations;

    #[test]
    fn parses_short_loca_and_returns_glyph_headers() {
        let loca = [0u16, 9, 9, 15]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let mut glyf = vec![0; 30];
        glyf[..2].copy_from_slice(&1i16.to_be_bytes());
        glyf[2..10].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
        glyf[10..12].copy_from_slice(&0u16.to_be_bytes());
        glyf[12..14].copy_from_slice(&0u16.to_be_bytes());
        glyf[14] = 0x37;
        glyf[15] = 1;
        glyf[16] = 2;
        glyf[18..20].copy_from_slice(&(-1i16).to_be_bytes());
        glyf[20..28].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);

        let locations = GlyphLocations::parse(&loca, &glyf, 3, 0).unwrap();
        let first = locations.glyph(&glyf, 0).unwrap().unwrap();
        assert_eq!(first.number_of_contours, 1);
        assert_eq!(first.bounds, [1, 2, 5, 8]);
        assert_eq!(first.contour_end_points, [0]);
        assert_eq!(
            first.points,
            [super::GlyphPoint {
                x: 1,
                y: 2,
                on_curve: true
            }]
        );
        assert_eq!(first.instructions, []);
        assert_eq!(first.data.len(), 18);
        assert!(locations.glyph(&glyf, 1).unwrap().is_none());
        let composite = locations.glyph(&glyf, 2).unwrap().unwrap();
        assert_eq!(composite.number_of_contours, -1);
        assert_eq!(composite.bounds, [0, 0, 4, 4]);
        assert!(composite.points.is_empty());
        assert!(locations.glyph(&glyf, 3).unwrap().is_none());
    }

    #[test]
    fn parses_long_loca_offsets() {
        let loca = [0u32, 10, 20]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let mut glyf = vec![0; 20];
        glyf[..2].copy_from_slice(&(-1i16).to_be_bytes());
        let locations = GlyphLocations::parse(&loca, &glyf, 2, 1).unwrap();
        assert_eq!(locations.glyph(&glyf, 0).unwrap().unwrap().data.len(), 10);
    }

    #[test]
    fn decodes_coordinate_deltas_and_repeated_flags() {
        let mut glyph = vec![0; 22];
        glyph[..2].copy_from_slice(&1i16.to_be_bytes());
        glyph[10..12].copy_from_slice(&1u16.to_be_bytes());
        glyph[12..14].copy_from_slice(&2u16.to_be_bytes());
        glyph[14..16].copy_from_slice(&[0, 0]);
        glyph[16..18].copy_from_slice(&[0x0F, 1]);
        glyph[18..20].copy_from_slice(&[5, 7]);
        glyph[20..22].copy_from_slice(&[2, 3]);
        let loca = [0u16, 11]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &glyph, 1, 0).unwrap();
        let parsed = locations.glyph(&glyph, 0).unwrap().unwrap();
        assert_eq!(
            parsed.points,
            [
                super::GlyphPoint {
                    x: -5,
                    y: -2,
                    on_curve: true
                },
                super::GlyphPoint {
                    x: -12,
                    y: -5,
                    on_curve: true
                }
            ]
        );
        assert_eq!(parsed.instructions, [0, 0]);
    }

    #[test]
    fn rejects_invalid_loca_and_glyph_records() {
        assert!(GlyphLocations::parse(&[0, 0], &[], 1, 0).is_err());
        assert!(GlyphLocations::parse(&[0, 0, 0, 4], &[], 1, 0).is_err());
        assert!(GlyphLocations::parse(&[0, 4, 0, 2], &[0; 8], 2, 0).is_err());
        assert!(GlyphLocations::parse(&[0; 12], &[], 2, 2).is_err());

        let loca = [0u16, 4]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let glyph = [0; 8];
        let locations = GlyphLocations::parse(&loca, &glyph, 1, 0).unwrap();
        assert!(locations.glyph(&glyph, 0).is_err());

        let inverted = [0, 0, 0, 4, 0, 0, 0, 2, 0, 0];
        let loca = [0u16, 5]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &inverted, 1, 0).unwrap();
        assert!(locations.glyph(&inverted, 0).is_err());

        let mut malformed = vec![0; 18];
        malformed[..2].copy_from_slice(&1i16.to_be_bytes());
        malformed[10..12].copy_from_slice(&0u16.to_be_bytes());
        malformed[12..14].copy_from_slice(&0u16.to_be_bytes());
        malformed[14] = 0x08;
        malformed[15] = 1;
        let loca = [0u16, 9]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &malformed, 1, 0).unwrap();
        assert!(locations.glyph(&malformed, 0).is_err());
    }

    #[test]
    fn rejects_malformed_simple_glyph_contours_and_coordinates() {
        let mut unordered = vec![0; 14];
        unordered[..2].copy_from_slice(&2i16.to_be_bytes());
        unordered[10..14].copy_from_slice(&[0, 1, 0, 0]);
        let loca = [0u16, 7]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &unordered, 1, 0).unwrap();
        assert!(locations.glyph(&unordered, 0).is_err());

        let mut reserved_flag = vec![0; 16];
        reserved_flag[..2].copy_from_slice(&1i16.to_be_bytes());
        reserved_flag[14] = 0x80;
        let loca = [0u16, 8]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &reserved_flag, 1, 0).unwrap();
        assert!(locations.glyph(&reserved_flag, 0).is_err());

        let mut overflow = vec![0; 20];
        overflow[..2].copy_from_slice(&1i16.to_be_bytes());
        overflow[10..12].copy_from_slice(&1u16.to_be_bytes());
        overflow[14..16].copy_from_slice(&[0x21, 0x21]);
        overflow[16..20].copy_from_slice(&[0x7F, 0xFF, 0, 1]);
        let loca = [0u16, 10]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &overflow, 1, 0).unwrap();
        assert!(locations.glyph(&overflow, 0).is_err());
    }
}
