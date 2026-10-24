//! Synthetic `head`/`hhea`/`maxp`/`hmtx` tables handed to `ttf-parser`.

/// The `head` table fixes `unitsPerEm` to this range (OpenType specification).
const MIN_UNITS_PER_EM: u16 = 16;
const MAX_UNITS_PER_EM: u16 = 16384;

pub(super) fn units_per_em_from_matrix(matrix: &[f64; 6]) -> u16 {
    let scale = matrix[0];
    if !scale.is_finite() || scale <= 0.0 {
        return 1000;
    }
    let units = (1.0 / scale).round();
    if units.is_finite() {
        units.clamp(f64::from(MIN_UNITS_PER_EM), f64::from(MAX_UNITS_PER_EM)) as u16
    } else {
        1000
    }
}

/// The `head`/`hhea`/`maxp`/`hmtx` tables synthesized for `ttf-parser`.
#[derive(Debug, Clone)]
pub(super) struct SyntheticTables {
    pub(super) head: [u8; 54],
    pub(super) hhea: [u8; 36],
    pub(super) maxp: [u8; 6],
    pub(super) hmtx: Vec<u8>,
}

impl SyntheticTables {
    pub(super) fn new(units_per_em: u16, glyph_count: u16, font_bbox: Option<[f64; 4]>) -> Self {
        let mut head = [0u8; 54];
        head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
        head[4..8].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // fontRevision
        head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magic
        head[18..20].copy_from_slice(&units_per_em.to_be_bytes());
        let bbox = font_bbox.map_or([0i16; 4], |values| {
            let mut bbox = [0i16; 4];
            for (slot, value) in bbox.iter_mut().zip(values) {
                *slot = if value.is_finite() {
                    value
                        .round()
                        .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
                } else {
                    0
                };
            }
            bbox
        });
        for (index, value) in bbox.iter().enumerate() {
            head[36 + index * 2..38 + index * 2].copy_from_slice(&value.to_be_bytes());
        }
        head[48..50].copy_from_slice(&2i16.to_be_bytes()); // fontDirectionHint
        head[50..52].copy_from_slice(&0i16.to_be_bytes()); // indexToLocFormat: short

        let mut hhea = [0u8; 36];
        hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
        hhea[18..20].copy_from_slice(&1i16.to_be_bytes()); // caretSlopeRise
        hhea[34..36].copy_from_slice(&glyph_count.to_be_bytes());

        let mut maxp = [0u8; 6];
        maxp[0..4].copy_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5
        maxp[4..6].copy_from_slice(&glyph_count.to_be_bytes());

        // Advance width and left side bearing are zero; widths are taken from
        // the parsed CFF charstrings instead.
        let hmtx = vec![0u8; usize::from(glyph_count) * 4];

        Self {
            head,
            hhea,
            maxp,
            hmtx,
        }
    }
}
