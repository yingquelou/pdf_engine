use std::ops::Range;

use super::super::FontError;

use self::error::MAX_COMPOSITE_DEPTH;
use self::error::invalid_glyph;
use self::metrics::{read_i16, read_loca_u16, read_u32};
use self::outline::parse_simple_glyph;

mod composite;
mod error;
mod metrics;
mod outline;

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
    /// End-point point indices for each contour, including expanded components.
    pub contour_end_points: Vec<u16>,
    /// Decoded points. Composite glyph components are recursively expanded.
    pub points: Vec<GlyphPoint>,
    /// TrueType hinting instructions stored on the glyph.
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
        self.resolve_glyph(glyf, glyph_id, &mut Vec::new())
    }

    fn resolve_glyph<'a>(
        &self,
        glyf: &'a [u8],
        glyph_id: u16,
        component_stack: &mut Vec<u16>,
    ) -> Result<Option<TrueTypeGlyph<'a>>, FontError> {
        let Some(range) = self.glyph_ranges.get(usize::from(glyph_id)) else {
            return Ok(None);
        };
        if range.is_empty() {
            return Ok(None);
        }
        if component_stack.len() >= MAX_COMPOSITE_DEPTH {
            return Err(invalid_glyph("composite glyph nesting is too deep"));
        }
        if component_stack.contains(&glyph_id) {
            return Err(invalid_glyph("composite glyph contains a component cycle"));
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
            component_stack.push(glyph_id);
            let result = self.parse_composite_glyph(glyf, data, component_stack);
            component_stack.pop();
            result?
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

#[cfg(test)]
mod tests {
    use super::GlyphLocations;

    #[test]
    fn parses_short_loca_and_returns_glyph_headers() {
        let loca = [0u16, 9, 9, 17]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let mut glyf = vec![0; 34];
        glyf[..2].copy_from_slice(&1i16.to_be_bytes());
        glyf[2..10].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
        glyf[10..12].copy_from_slice(&0u16.to_be_bytes());
        glyf[12..14].copy_from_slice(&0u16.to_be_bytes());
        glyf[14] = 0x37;
        glyf[15] = 1;
        glyf[16] = 2;
        glyf[18..20].copy_from_slice(&(-1i16).to_be_bytes());
        glyf[20..28].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
        glyf[28..30].copy_from_slice(&0x0002u16.to_be_bytes());
        glyf[30..32].copy_from_slice(&0u16.to_be_bytes());

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
        assert_eq!(composite.contour_end_points, [0]);
        assert_eq!(composite.points, first.points);
        assert!(locations.glyph(&glyf, 3).unwrap().is_none());
    }

    #[test]
    fn parses_long_loca_offsets() {
        let loca = [0u32, 12, 12]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let glyf = vec![0; 12];
        let locations = GlyphLocations::parse(&loca, &glyf, 2, 1).unwrap();
        assert_eq!(locations.glyph(&glyf, 0).unwrap().unwrap().data.len(), 12);
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
    fn expands_scaled_and_point_attached_composite_components() {
        let mut glyf = vec![0; 36];
        glyf[..2].copy_from_slice(&1i16.to_be_bytes());
        glyf[10..12].copy_from_slice(&0u16.to_be_bytes());
        glyf[12..14].copy_from_slice(&0u16.to_be_bytes());
        glyf[14] = 0x37;
        glyf[15] = 1;
        glyf[16] = 2;
        glyf[18..20].copy_from_slice(&(-1i16).to_be_bytes());
        glyf[28..30].copy_from_slice(&0x080Au16.to_be_bytes());
        glyf[30..32].copy_from_slice(&0u16.to_be_bytes());
        glyf[32..34].copy_from_slice(&[4, 0xFE]);
        glyf[34..36].copy_from_slice(&0x2000i16.to_be_bytes());

        let loca = [0u16, 9, 18]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &glyf, 2, 0).unwrap();
        let scaled = locations.glyph(&glyf, 1).unwrap().unwrap();
        assert_eq!((scaled.points[0].x, scaled.points[0].y), (3, 0));

        let mut glyf = vec![0; 60];
        put_single_point_glyph(&mut glyf, 0, 5, 7);
        put_single_point_glyph(&mut glyf, 18, 10, 20);
        glyf[36..38].copy_from_slice(&(-1i16).to_be_bytes());
        glyf[46..48].copy_from_slice(&0x0022u16.to_be_bytes());
        glyf[48..50].copy_from_slice(&0u16.to_be_bytes());
        glyf[50..52].copy_from_slice(&[0, 0]);
        glyf[52..54].copy_from_slice(&1u16.to_be_bytes());
        glyf[54..56].copy_from_slice(&1u16.to_be_bytes());
        glyf[56..60].copy_from_slice(&[0, 0, 0, 0]);
        let loca = [0u16, 9, 18, 30]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &glyf, 3, 0).unwrap();
        let attached = locations.glyph(&glyf, 2).unwrap().unwrap();
        assert_eq!(attached.contour_end_points, [0, 1]);
        assert_eq!(
            attached
                .points
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
            [(5, 7), (5, 7)]
        );
    }

    #[test]
    fn rejects_composite_cycles_and_unknown_glyph_references() {
        let mut glyf = vec![0; 32];
        glyf[0..2].copy_from_slice(&(-1i16).to_be_bytes());
        glyf[10..12].copy_from_slice(&0x0002u16.to_be_bytes());
        glyf[12..14].copy_from_slice(&1u16.to_be_bytes());
        glyf[16..18].copy_from_slice(&(-1i16).to_be_bytes());
        glyf[26..28].copy_from_slice(&0x0002u16.to_be_bytes());
        glyf[28..30].copy_from_slice(&0u16.to_be_bytes());
        let loca = [0u16, 8, 16]
            .into_iter()
            .flat_map(|offset| offset.to_be_bytes())
            .collect::<Vec<_>>();
        let locations = GlyphLocations::parse(&loca, &glyf, 2, 0).unwrap();
        assert!(locations.glyph(&glyf, 0).is_err());

        glyf[12..14].copy_from_slice(&2u16.to_be_bytes());
        assert!(locations.glyph(&glyf, 0).is_err());
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

    fn put_single_point_glyph(data: &mut [u8], offset: usize, x: u8, y: u8) {
        data[offset..offset + 2].copy_from_slice(&1i16.to_be_bytes());
        data[offset + 10..offset + 12].copy_from_slice(&0u16.to_be_bytes());
        data[offset + 12..offset + 14].copy_from_slice(&0u16.to_be_bytes());
        data[offset + 14] = 0x37;
        data[offset + 15] = x;
        data[offset + 16] = y;
    }
}
