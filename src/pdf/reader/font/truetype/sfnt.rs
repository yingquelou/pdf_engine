use std::{collections::BTreeMap, ops::Range};

use super::{
    super::FontError,
    cmap::{Cmap, CmapFormat},
    glyph::{GlyphLocations, TrueTypeGlyph},
    tables::TrueTypeMetadata,
};

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
    cmap: Cmap,
    glyph_locations: GlyphLocations,
}

impl TrueTypeFont {
    /// Parse an SFNT directory from decoded font program bytes.
    ///
    /// This validates the signature, directory boundaries, table tags,
    /// alignment, payload ranges, and required metadata tables. Unicode
    /// character mapping supports cmap formats 4 and 12.
    /// It does not validate table checksums or render glyph outlines.
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
        let cmap_data = tables
            .get(b"cmap")
            .map(|range| &data[range.clone()])
            .ok_or_else(|| invalid_sfnt("missing required `cmap` table"))?;
        let cmap = Cmap::parse(cmap_data, metadata.glyph_count)?;
        let loca = tables
            .get(b"loca")
            .map(|range| &data[range.clone()])
            .ok_or_else(|| invalid_sfnt("missing required `loca` table"))?;
        let glyf = tables
            .get(b"glyf")
            .map(|range| &data[range.clone()])
            .ok_or_else(|| invalid_sfnt("missing required `glyf` table"))?;
        let glyph_locations = GlyphLocations::parse(
            loca,
            glyf,
            metadata.glyph_count,
            metadata.index_to_loc_format,
        )?;

        Ok(Self {
            data,
            flavor,
            tables,
            metadata,
            cmap,
            glyph_locations,
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

    /// Return the selected Unicode character-map subtable format.
    pub fn cmap_format(&self) -> CmapFormat {
        self.cmap.format()
    }

    /// Map a Unicode codepoint to a glyph ID.
    ///
    /// Format 4 supports BMP codepoints; format 12 also supports supplementary
    /// Unicode planes. A codepoint absent from the selected map returns `None`.
    pub fn glyph_id_for_unicode(&self, codepoint: u32) -> Result<Option<u16>, FontError> {
        if char::from_u32(codepoint).is_none() {
            return Ok(None);
        }
        let cmap_data = self
            .table(b"cmap")
            .ok_or_else(|| invalid_sfnt("missing required `cmap` table"))?;
        self.cmap
            .glyph_id(cmap_data, codepoint, self.metadata.glyph_count)
    }

    /// Return a glyph record and its header bounds by glyph ID.
    ///
    /// Empty glyphs and IDs outside the font's glyph range return `None`.
    /// Composite glyph components are recursively expanded into the returned
    /// contour points; `data` retains the original encoded record.
    pub fn glyph(&self, glyph_id: u16) -> Result<Option<TrueTypeGlyph<'_>>, FontError> {
        let glyf = self
            .table(b"glyf")
            .ok_or_else(|| invalid_sfnt("missing required `glyf` table"))?;
        self.glyph_locations.glyph(glyf, glyph_id)
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
    use super::{CmapFormat, SfntFlavor, TrueTypeFont};

    fn simple_format4_cmap() -> Vec<u8> {
        let mut cmap = vec![0; 44];
        cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
        cmap[4..6].copy_from_slice(&3u16.to_be_bytes());
        cmap[6..8].copy_from_slice(&1u16.to_be_bytes());
        cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
        let subtable = &mut cmap[12..];
        subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
        subtable[2..4].copy_from_slice(&32u16.to_be_bytes());
        subtable[6..8].copy_from_slice(&4u16.to_be_bytes());
        subtable[8..10].copy_from_slice(&4u16.to_be_bytes());
        subtable[10..12].copy_from_slice(&1u16.to_be_bytes());
        subtable[14..16].copy_from_slice(&0x41u16.to_be_bytes());
        subtable[16..18].copy_from_slice(&u16::MAX.to_be_bytes());
        subtable[20..22].copy_from_slice(&0x41u16.to_be_bytes());
        subtable[22..24].copy_from_slice(&u16::MAX.to_be_bytes());
        subtable[24..26].copy_from_slice(&(-63i16).to_be_bytes());
        subtable[26..28].copy_from_slice(&1i16.to_be_bytes());
        cmap
    }

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
                (*b"head", 124, 54),
                (*b"maxp", 180, 32),
                (*b"hhea", 212, 36),
                (*b"hmtx", 248, 10),
                (*b"cmap", 260, 44),
                (*b"loca", 304, 8),
                (*b"glyf", 312, 34),
            ],
        );
        bytes[124 + 12..124 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        bytes[124 + 18..124 + 20].copy_from_slice(&2048u16.to_be_bytes());
        bytes[124 + 50..124 + 52].copy_from_slice(&0i16.to_be_bytes());
        bytes[180..184].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[184..186].copy_from_slice(&3u16.to_be_bytes());
        bytes[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[246..248].copy_from_slice(&2u16.to_be_bytes());
        bytes[248..250].copy_from_slice(&500u16.to_be_bytes());
        bytes[252..254].copy_from_slice(&600u16.to_be_bytes());
        let cmap = simple_format4_cmap();
        bytes[260..304].copy_from_slice(&cmap);
        bytes[304..306].copy_from_slice(&0u16.to_be_bytes());
        bytes[306..308].copy_from_slice(&9u16.to_be_bytes());
        bytes[308..310].copy_from_slice(&9u16.to_be_bytes());
        bytes[310..312].copy_from_slice(&17u16.to_be_bytes());
        bytes[312..314].copy_from_slice(&1i16.to_be_bytes());
        bytes[314..322].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
        bytes[322..324].copy_from_slice(&0u16.to_be_bytes());
        bytes[324..326].copy_from_slice(&0u16.to_be_bytes());
        bytes[326] = 0x37;
        bytes[327] = 1;
        bytes[328] = 2;
        bytes[330..332].copy_from_slice(&(-1i16).to_be_bytes());
        bytes[332..340].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
        bytes[340..342].copy_from_slice(&0x0002u16.to_be_bytes());
        bytes[342..344].copy_from_slice(&0u16.to_be_bytes());
        let font = TrueTypeFont::parse(bytes).unwrap();

        assert_eq!(font.flavor(), SfntFlavor::TrueType);
        assert_eq!(font.units_per_em(), 2048);
        assert_eq!(font.glyph_count(), 3);
        assert_eq!(font.horizontal_advance_width(0), Some(500));
        assert_eq!(font.horizontal_advance_width(1), Some(600));
        assert_eq!(font.horizontal_advance_width(2), Some(600));
        assert_eq!(font.horizontal_advance_width(3), None);
        assert_eq!(font.cmap_format(), CmapFormat::Format4);
        assert_eq!(font.glyph_id_for_unicode(0x41).unwrap(), Some(2));
        assert_eq!(font.glyph_id_for_unicode(0x42).unwrap(), None);
        let glyph = font.glyph(0).unwrap().unwrap();
        assert_eq!(glyph.bounds, [1, 2, 5, 8]);
        assert_eq!(glyph.points.len(), 1);
        assert!(font.glyph(1).unwrap().is_none());
        let composite = font.glyph(2).unwrap().unwrap();
        assert_eq!(composite.number_of_contours, -1);
        assert_eq!(composite.points.len(), 1);
        assert_eq!(font.table(b"head").unwrap().len(), 54);
        assert_eq!(font.table(b"maxp").unwrap().len(), 32);
        assert_eq!(font.table(b"hhea").unwrap().len(), 36);
        assert_eq!(font.table(b"hmtx").unwrap().len(), 10);
        assert_eq!(font.table(b"cmap").unwrap().len(), 44);
        assert_eq!(font.table(b"loca").unwrap().len(), 8);
        assert_eq!(font.table(b"glyf").unwrap().len(), 34);
        assert_eq!(font.table(b"name"), None);
        assert_eq!(font.table_tags().count(), 7);
    }

    #[test]
    fn recognizes_legacy_apple_true_signature() {
        let mut bytes = font_with_tables(
            *b"true",
            &[
                (*b"head", 124, 54),
                (*b"maxp", 180, 32),
                (*b"hhea", 212, 36),
                (*b"hmtx", 248, 4),
                (*b"cmap", 252, 44),
                (*b"loca", 296, 4),
                (*b"glyf", 300, 10),
            ],
        );
        bytes[124 + 12..124 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        bytes[124 + 18..124 + 20].copy_from_slice(&1000u16.to_be_bytes());
        bytes[180..184].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[184..186].copy_from_slice(&1u16.to_be_bytes());
        bytes[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[246..248].copy_from_slice(&1u16.to_be_bytes());
        bytes[252..296].copy_from_slice(&simple_format4_cmap());
        bytes[296..298].copy_from_slice(&0u16.to_be_bytes());
        bytes[298..300].copy_from_slice(&5u16.to_be_bytes());
        bytes[300..302].copy_from_slice(&1i16.to_be_bytes());
        bytes[302..310].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
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
