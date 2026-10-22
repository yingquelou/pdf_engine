use std::{collections::BTreeMap, ops::Range};

use super::{super::FontError, tables::TrueTypeMetadata};

/// Recognized SFNT signatures for TrueType-outline fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfntFlavor {
    /// The standard TrueType version 1 signature (`0x00010000`).
    TrueType,
    /// Apple's legacy TrueType signature (`true`).
    AppleTrue,
}

/// A decoded TrueType font with a validated SFNT table directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrueTypeFont {
    data: Vec<u8>,
    flavor: SfntFlavor,
    tables: BTreeMap<[u8; 4], Range<usize>>,
    metadata: TrueTypeMetadata,
}

impl TrueTypeFont {
    /// Parse an SFNT directory from decoded font program bytes.
    ///
    /// This validates the signature, directory boundaries, table tags,
    /// alignment, payload ranges, and the required `head`/`maxp` metadata.
    /// It does not validate table checksums or parse other tables.
    pub fn parse(data: Vec<u8>) -> Result<Self, FontError> {
        if data.len() < 12 {
            return Err(invalid_sfnt("header is truncated"));
        }

        let flavor = match &data[..4] {
            [0, 1, 0, 0] => SfntFlavor::TrueType,
            b"true" => SfntFlavor::AppleTrue,
            _ => return Err(invalid_sfnt("unsupported SFNT signature")),
        };
        let table_count = usize::from(u16::from_be_bytes([data[4], data[5]]));
        if table_count == 0 {
            return Err(invalid_sfnt("table directory is empty"));
        }
        let directory_length = table_count
            .checked_mul(16)
            .and_then(|length| 12usize.checked_add(length))
            .ok_or_else(|| invalid_sfnt("table directory length overflows"))?;
        if directory_length > data.len() {
            return Err(invalid_sfnt("table directory is truncated"));
        }

        let mut tables = BTreeMap::new();
        let mut ranges = Vec::with_capacity(table_count);
        for index in 0..table_count {
            let record_start = 12 + index * 16;
            let tag: [u8; 4] = data[record_start..record_start + 4]
                .try_into()
                .expect("table tag has a fixed-width slice");
            if tables.contains_key(&tag) {
                return Err(invalid_sfnt("table directory contains a duplicate tag"));
            }

            let offset = u32::from_be_bytes(
                data[record_start + 8..record_start + 12]
                    .try_into()
                    .expect("table offset has a fixed-width slice"),
            ) as usize;
            let length = u32::from_be_bytes(
                data[record_start + 12..record_start + 16]
                    .try_into()
                    .expect("table length has a fixed-width slice"),
            ) as usize;
            if offset % 4 != 0 {
                return Err(invalid_sfnt("table offset is not four-byte aligned"));
            }
            if offset < directory_length {
                return Err(invalid_sfnt("table data overlaps the SFNT directory"));
            }
            let end = offset
                .checked_add(length)
                .ok_or_else(|| invalid_sfnt("table range overflows"))?;
            if end > data.len() {
                return Err(invalid_sfnt("table data is outside the font program"));
            }
            tables.insert(tag, offset..end);
            if length > 0 {
                ranges.push((offset, end));
            }
        }

        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(invalid_sfnt("table data ranges overlap"));
        }

        let metadata = TrueTypeMetadata::parse(
            tables.get(b"head").map(|range| &data[range.clone()]),
            tables.get(b"maxp").map(|range| &data[range.clone()]),
            tables.get(b"hhea").map(|range| &data[range.clone()]),
            tables.get(b"hmtx").map(|range| &data[range.clone()]),
        )?;

        Ok(Self {
            data,
            flavor,
            tables,
            metadata,
        })
    }

    /// Return the recognized SFNT signature.
    pub fn flavor(&self) -> SfntFlavor {
        self.flavor
    }

    /// Return the font's units-per-em value from its `head` table.
    pub fn units_per_em(&self) -> u16 {
        self.metadata.units_per_em
    }

    /// Return the number of glyphs declared by the `maxp` table.
    pub fn glyph_count(&self) -> u16 {
        self.metadata.glyph_count
    }

    /// Return the horizontal advance width for a glyph ID.
    ///
    /// TrueType repeats the last long horizontal metric's advance width for
    /// glyphs beyond `hhea.numberOfHMetrics`.
    pub fn horizontal_advance_width(&self, glyph_id: u16) -> Option<u16> {
        if glyph_id >= self.metadata.glyph_count {
            return None;
        }
        let hmtx = self.table(b"hmtx")?;
        let metric_index = usize::from(glyph_id.min(self.metadata.horizontal_metric_count - 1));
        let offset = metric_index * 4;
        Some(u16::from_be_bytes(
            hmtx[offset..offset + 2]
                .try_into()
                .expect("validated hmtx metric slice"),
        ))
    }

    /// Return the tags present in the validated table directory.
    pub fn table_tags(&self) -> impl Iterator<Item = &[u8; 4]> {
        self.tables.keys()
    }

    /// Return the bytes for a table tag, if it exists.
    pub fn table(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.tables.get(tag).map(|range| &self.data[range.clone()])
    }
}

fn invalid_sfnt(message: &str) -> FontError {
    FontError::Invalid(format!("invalid TrueType SFNT font: {message}"))
}

#[cfg(test)]
mod tests {
    use super::{SfntFlavor, TrueTypeFont};

    fn font_with_tables(signature: [u8; 4], records: &[([u8; 4], u32, u32)]) -> Vec<u8> {
        let directory_end = 12 + records.len() * 16;
        let data_length = records
            .iter()
            .map(|(_, offset, length)| (*offset as usize) + (*length as usize))
            .max()
            .unwrap_or(directory_end)
            .max(directory_end);
        let mut bytes = vec![0; data_length];
        bytes[..4].copy_from_slice(&signature);
        bytes[4..6].copy_from_slice(&(records.len() as u16).to_be_bytes());
        for (index, (tag, offset, length)) in records.iter().enumerate() {
            let start = 12 + index * 16;
            bytes[start..start + 4].copy_from_slice(tag);
            bytes[start + 8..start + 12].copy_from_slice(&offset.to_be_bytes());
            bytes[start + 12..start + 16].copy_from_slice(&length.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn parses_and_exposes_sfnt_tables() {
        let mut bytes = font_with_tables(
            [0, 1, 0, 0],
            &[
                (*b"head", 76, 54),
                (*b"maxp", 132, 32),
                (*b"hhea", 164, 36),
                (*b"hmtx", 200, 10),
            ],
        );
        bytes[76 + 12..76 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        bytes[76 + 18..76 + 20].copy_from_slice(&2048u16.to_be_bytes());
        bytes[132..136].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[136..138].copy_from_slice(&3u16.to_be_bytes());
        bytes[164..168].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[198..200].copy_from_slice(&2u16.to_be_bytes());
        bytes[200..202].copy_from_slice(&500u16.to_be_bytes());
        bytes[204..206].copy_from_slice(&600u16.to_be_bytes());
        let font = TrueTypeFont::parse(bytes).unwrap();

        assert_eq!(font.flavor(), SfntFlavor::TrueType);
        assert_eq!(font.units_per_em(), 2048);
        assert_eq!(font.glyph_count(), 3);
        assert_eq!(font.horizontal_advance_width(0), Some(500));
        assert_eq!(font.horizontal_advance_width(1), Some(600));
        assert_eq!(font.horizontal_advance_width(2), Some(600));
        assert_eq!(font.horizontal_advance_width(3), None);
        assert_eq!(font.table(b"head").unwrap().len(), 54);
        assert_eq!(font.table(b"maxp").unwrap().len(), 32);
        assert_eq!(font.table(b"hhea").unwrap().len(), 36);
        assert_eq!(font.table(b"hmtx").unwrap().len(), 10);
        assert_eq!(font.table(b"name"), None);
        assert_eq!(font.table_tags().count(), 4);
    }

    #[test]
    fn recognizes_legacy_apple_true_signature() {
        let mut bytes = font_with_tables(
            *b"true",
            &[
                (*b"head", 76, 54),
                (*b"maxp", 132, 32),
                (*b"hhea", 164, 36),
                (*b"hmtx", 200, 4),
            ],
        );
        bytes[76 + 12..76 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        bytes[76 + 18..76 + 20].copy_from_slice(&1000u16.to_be_bytes());
        bytes[132..136].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[136..138].copy_from_slice(&1u16.to_be_bytes());
        bytes[164..168].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[198..200].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(
            TrueTypeFont::parse(bytes).unwrap().flavor(),
            SfntFlavor::AppleTrue
        );
    }

    #[test]
    fn rejects_malformed_sfnt_directories() {
        assert!(TrueTypeFont::parse(vec![0; 11]).is_err());

        let unsupported = font_with_tables(*b"OTTO", &[(*b"head", 28, 4)]);
        assert!(TrueTypeFont::parse(unsupported).is_err());

        let truncated = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        assert!(TrueTypeFont::parse(truncated).is_err());

        let duplicate = font_with_tables([0, 1, 0, 0], &[(*b"head", 44, 4), (*b"head", 48, 4)]);
        assert!(TrueTypeFont::parse(duplicate).is_err());

        let mut out_of_bounds = font_with_tables([0, 1, 0, 0], &[(*b"head", 28, 4)]);
        out_of_bounds[24..28].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(TrueTypeFont::parse(out_of_bounds).is_err());

        let unaligned = font_with_tables([0, 1, 0, 0], &[(*b"head", 29, 4)]);
        assert!(TrueTypeFont::parse(unaligned).is_err());

        let overlap = font_with_tables([0, 1, 0, 0], &[(*b"head", 44, 8), (*b"maxp", 48, 4)]);
        assert!(TrueTypeFont::parse(overlap).is_err());
    }
}
