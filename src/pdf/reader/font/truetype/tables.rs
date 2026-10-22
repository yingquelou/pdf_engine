use super::super::FontError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TrueTypeMetadata {
    pub units_per_em: u16,
    pub glyph_count: u16,
    pub horizontal_metric_count: u16,
}

impl TrueTypeMetadata {
    pub fn parse(
        head: Option<&[u8]>,
        maxp: Option<&[u8]>,
        hhea: Option<&[u8]>,
        hmtx: Option<&[u8]>,
    ) -> Result<Self, FontError> {
        let head = head.ok_or_else(|| invalid_table("missing required `head` table"))?;
        if head.len() < 54 {
            return Err(invalid_table("`head` table is truncated"));
        }
        let magic = u32::from_be_bytes(head[12..16].try_into().expect("fixed-width slice"));
        if magic != 0x5F0F3CF5 {
            return Err(invalid_table("`head` table has an invalid magic number"));
        }
        let units_per_em = u16::from_be_bytes(head[18..20].try_into().expect("fixed-width slice"));
        if !(16..=16_384).contains(&units_per_em) {
            return Err(invalid_table("`head` table has invalid units per em"));
        }
        let index_to_loc_format =
            i16::from_be_bytes(head[50..52].try_into().expect("fixed-width slice"));
        if !matches!(index_to_loc_format, 0 | 1) {
            return Err(invalid_table(
                "`head` table has an invalid indexToLocFormat",
            ));
        }

        let maxp = maxp.ok_or_else(|| invalid_table("missing required `maxp` table"))?;
        if maxp.len() < 32 {
            return Err(invalid_table("`maxp` table is truncated"));
        }
        let version = u32::from_be_bytes(maxp[..4].try_into().expect("fixed-width slice"));
        if version != 0x0001_0000 {
            return Err(invalid_table("`maxp` table has an unsupported version"));
        }
        let glyph_count = u16::from_be_bytes(maxp[4..6].try_into().expect("fixed-width slice"));
        if glyph_count == 0 {
            return Err(invalid_table("`maxp` table declares no glyphs"));
        }

        let hhea = hhea.ok_or_else(|| invalid_table("missing required `hhea` table"))?;
        if hhea.len() < 36 {
            return Err(invalid_table("`hhea` table is truncated"));
        }
        let hhea_version = u32::from_be_bytes(hhea[..4].try_into().expect("fixed-width slice"));
        if hhea_version != 0x0001_0000 {
            return Err(invalid_table("`hhea` table has an unsupported version"));
        }
        let horizontal_metric_count =
            u16::from_be_bytes(hhea[34..36].try_into().expect("fixed-width slice"));
        if horizontal_metric_count == 0 || horizontal_metric_count > glyph_count {
            return Err(invalid_table(
                "`hhea` table has an invalid numberOfHMetrics",
            ));
        }

        let hmtx = hmtx.ok_or_else(|| invalid_table("missing required `hmtx` table"))?;
        let long_metrics_length = usize::from(horizontal_metric_count) * 4;
        let trailing_bearings_length = usize::from(glyph_count - horizontal_metric_count) * 2;
        let required_hmtx_length = long_metrics_length + trailing_bearings_length;
        if hmtx.len() < required_hmtx_length {
            return Err(invalid_table("`hmtx` table is truncated"));
        }

        Ok(Self {
            units_per_em,
            glyph_count,
            horizontal_metric_count,
        })
    }
}

fn invalid_table(message: &str) -> FontError {
    FontError::Invalid(format!("invalid TrueType tables: {message}"))
}

#[cfg(test)]
mod tests {
    use super::TrueTypeMetadata;

    #[test]
    fn parses_head_and_maxp_metadata() {
        let mut head = vec![0; 54];
        head[12..16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        head[18..20].copy_from_slice(&2048u16.to_be_bytes());
        let mut maxp = vec![0; 32];
        maxp[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&256u16.to_be_bytes());
        let mut hhea = vec![0; 36];
        hhea[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        hhea[34..36].copy_from_slice(&2u16.to_be_bytes());
        let hmtx = vec![0; 4 * 2 + 2 * 254];
        let metadata =
            TrueTypeMetadata::parse(Some(&head), Some(&maxp), Some(&hhea), Some(&hmtx)).unwrap();
        assert_eq!(metadata.units_per_em, 2048);
        assert_eq!(metadata.glyph_count, 256);
        assert_eq!(metadata.horizontal_metric_count, 2);
    }

    #[test]
    fn rejects_missing_or_invalid_required_tables() {
        assert!(TrueTypeMetadata::parse(None, None, None, None).is_err());

        let mut head = vec![0; 54];
        head[12..16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        let mut maxp = vec![0; 32];
        maxp[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&1u16.to_be_bytes());
        let mut hhea = vec![0; 36];
        hhea[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        hhea[34..36].copy_from_slice(&1u16.to_be_bytes());
        let hmtx = vec![0; 4];

        let mut invalid_head = head.clone();
        invalid_head[18..20].copy_from_slice(&15u16.to_be_bytes());
        assert!(
            TrueTypeMetadata::parse(Some(&invalid_head), Some(&maxp), Some(&hhea), Some(&hmtx))
                .is_err()
        );

        let mut invalid_maxp = maxp.clone();
        invalid_maxp[4..6].copy_from_slice(&0u16.to_be_bytes());
        assert!(
            TrueTypeMetadata::parse(Some(&head), Some(&invalid_maxp), Some(&hhea), Some(&hmtx))
                .is_err()
        );

        assert!(
            TrueTypeMetadata::parse(Some(&head[..53]), Some(&maxp), Some(&hhea), Some(&hmtx))
                .is_err()
        );
        assert!(
            TrueTypeMetadata::parse(Some(&head), Some(&maxp[..31]), Some(&hhea), Some(&hmtx))
                .is_err()
        );
        assert!(
            TrueTypeMetadata::parse(Some(&head), Some(&maxp), Some(&hhea[..35]), Some(&hmtx))
                .is_err()
        );
        assert!(
            TrueTypeMetadata::parse(Some(&head), Some(&maxp), Some(&hhea), Some(&hmtx[..3]))
                .is_err()
        );
    }

    #[test]
    fn rejects_invalid_horizontal_metric_count() {
        let mut head = vec![0; 54];
        head[12..16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        let mut maxp = vec![0; 32];
        maxp[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&2u16.to_be_bytes());
        let mut hhea = vec![0; 36];
        hhea[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        let hmtx = vec![0; 8];

        for count in [0u16, 3] {
            hhea[34..36].copy_from_slice(&count.to_be_bytes());
            assert!(
                TrueTypeMetadata::parse(Some(&head), Some(&maxp), Some(&hhea), Some(&hmtx))
                    .is_err()
            );
        }
    }
}
