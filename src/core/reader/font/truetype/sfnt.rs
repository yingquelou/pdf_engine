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
            tables.get(b"vhea").map(|range| &data[range.clone()]),
            tables.get(b"vmtx").map(|range| &data[range.clone()]),
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

    /// Return the vertical advance height for a glyph ID.
    ///
    /// Returns `None` when the font lacks `vhea`/`vmtx` tables or the glyph
    /// ID is out of range. Like horizontal metrics, TrueType reuses the last
    /// long vertical metric's advance height for glyphs beyond
    /// `vhea.numberOfVMetrics`.
    pub fn vertical_advance(&self, glyph_id: u16) -> Option<u16> {
        if self.metadata.vertical_metric_count == 0 || glyph_id >= self.metadata.glyph_count {
            return None;
        }
        let vmtx = self.table(b"vmtx")?;
        let metric_index = usize::from(glyph_id.min(self.metadata.vertical_metric_count - 1));
        let offset = metric_index * 4;
        Some(u16::from_be_bytes(
            vmtx[offset..offset + 2]
                .try_into()
                .expect("validated vmtx metric slice"),
        ))
    }

    /// Return the top side bearing for a glyph ID.
    ///
    /// Returns `None` when the font lacks `vhea`/`vmtx` tables or the glyph
    /// ID is out of range. For glyph IDs at or beyond `vhea.numberOfVMetrics`,
    /// the value is read from the trailing top-side-bearing array.
    pub fn top_side_bearing(&self, glyph_id: u16) -> Option<i16> {
        if self.metadata.vertical_metric_count == 0 || glyph_id >= self.metadata.glyph_count {
            return None;
        }
        let vmtx = self.table(b"vmtx")?;
        let vmetric_count = usize::from(self.metadata.vertical_metric_count);
        let metric_index = usize::from(glyph_id.min(self.metadata.vertical_metric_count - 1));
        let offset = if glyph_id < self.metadata.vertical_metric_count {
            metric_index * 4 + 2
        } else {
            vmetric_count * 4 + (usize::from(glyph_id) - vmetric_count) * 2
        };
        Some(i16::from_be_bytes(
            vmtx[offset..offset + 2]
                .try_into()
                .expect("validated vmtx bearing slice"),
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

    /// Map a glyph ID to its Unicode codepoint using the font's built-in encoding.
    ///
    /// Reverses the cmap to find which Unicode character a glyph represents.
    /// When multiple codepoints map to the same glyph, returns the first encountered.
    /// Returns `None` for glyph ID 0 (the .notdef glyph) or unmapped glyphs.
    pub fn unicode_for_glyph(&self, glyph_id: u16) -> Result<Option<u32>, FontError> {
        if glyph_id == 0 {
            return Ok(None);
        }
        let cmap_data = self
            .table(b"cmap")
            .ok_or_else(|| invalid_sfnt("missing required `cmap` table"))?;
        let reverse = self
            .cmap
            .reverse_cmap(cmap_data, self.metadata.glyph_count)?;
        Ok(reverse.get(&glyph_id).copied())
    }

    /// Return the built-in single-byte encoding derived from the `cmap` table.
    ///
    /// Entry `i` is the Unicode scalar for character code `i` (0–255), resolved
    /// through glyph ID the same way PDF maps character codes for TrueType fonts
    /// without an `/Encoding` entry (ISO 32000-1 §9.6.6.4, §9.10.2). Codes with
    /// no glyph, or glyphs with no reverse cmap entry, are `None`.
    ///
    /// Symbolic (3, 0) subtables commonly address their glyphs through one of
    /// three bases above the ASCII range: `0xF000`, `0xF100`, or `0xF200`.
    /// When the direct lookup for a code fails, those offsets are probed in
    /// that order and the first mapped glyph wins, so the widespread `0xF000`
    /// convention keeps priority over the `0xF100` and `0xF200` variants.
    pub fn builtin_encoding(&self) -> Result<Vec<Option<char>>, FontError> {
        let cmap_data = self
            .table(b"cmap")
            .ok_or_else(|| invalid_sfnt("missing required `cmap` table"))?;
        let reverse = self
            .cmap
            .reverse_cmap(cmap_data, self.metadata.glyph_count)?;
        let mut encoding = vec![None; 256];
        for code in 0..=u8::MAX {
            let glyph =
                match self
                    .cmap
                    .glyph_id(cmap_data, u32::from(code), self.metadata.glyph_count)?
                {
                    Some(glyph) => Some(glyph),
                    None if self.cmap.is_symbolic() => self.symbolic_glyph(cmap_data, code)?,
                    None => None,
                };
            if let Some(glyph) = glyph {
                encoding[usize::from(code)] =
                    reverse.get(&glyph).and_then(|cp| char::from_u32(*cp));
            }
        }
        Ok(encoding)
    }

    /// Resolve a symbolic character code through the common `0xF000`-based
    /// offsets, trying `0xF000`, `0xF100`, then `0xF200` and stopping at the
    /// first mapped glyph.
    fn symbolic_glyph(&self, cmap_data: &[u8], code: u8) -> Result<Option<u16>, FontError> {
        for base in [0xF000u32, 0xF100, 0xF200] {
            let glyph =
                self.cmap
                    .glyph_id(cmap_data, base | u32::from(code), self.metadata.glyph_count)?;
            if glyph.is_some() {
                return Ok(glyph);
            }
        }
        Ok(None)
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

    /// A Microsoft Symbol (3, 0) format 4 cmap that maps `0xF041` to glyph 2.
    fn symbolic_format4_cmap() -> Vec<u8> {
        format4_cmap(3, 0, &[(0xF041, 2)])
    }

    /// Build a format 4 `cmap` recorded under `platform`/`encoding` with one
    /// segment per `(code, glyph)` pair, plus the required `0xFFFF` sentinel.
    fn format4_cmap(platform: u16, encoding: u16, mappings: &[(u16, u16)]) -> Vec<u8> {
        let segment_count = mappings.len() + 1;
        let mut end_codes = Vec::with_capacity(segment_count);
        let mut start_codes = Vec::with_capacity(segment_count);
        let mut deltas = Vec::with_capacity(segment_count);
        for (code, glyph) in mappings {
            end_codes.push(*code);
            start_codes.push(*code);
            deltas.push(glyph.wrapping_sub(*code));
        }
        end_codes.push(u16::MAX);
        start_codes.push(u16::MAX);
        // The sentinel maps 0xFFFF to glyph 0 through the idDelta wrap.
        deltas.push(1);

        let mut subtable = vec![0; 16 + segment_count * 8];
        let subtable_length = subtable.len() as u16;
        let write = |data: &mut [u8], offset: usize, values: &[u16]| {
            for (index, value) in values.iter().enumerate() {
                data[offset + index * 2..offset + index * 2 + 2]
                    .copy_from_slice(&value.to_be_bytes());
            }
        };
        let end_codes_offset = 14;
        let start_codes_offset = end_codes_offset + segment_count * 2 + 2;
        let deltas_offset = start_codes_offset + segment_count * 2;
        subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
        subtable[2..4].copy_from_slice(&subtable_length.to_be_bytes());
        subtable[6..8].copy_from_slice(&((segment_count * 2) as u16).to_be_bytes());
        write(&mut subtable, end_codes_offset, &end_codes);
        write(&mut subtable, start_codes_offset, &start_codes);
        write(&mut subtable, deltas_offset, &deltas);

        let mut cmap = vec![0; 12 + subtable.len()];
        cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
        cmap[4..6].copy_from_slice(&platform.to_be_bytes());
        cmap[6..8].copy_from_slice(&encoding.to_be_bytes());
        cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
        cmap[12..].copy_from_slice(&subtable);
        cmap
    }

    /// Build a three-glyph font whose `cmap` table is `cmap`.
    ///
    /// The `loca` and `glyf` tables follow the cmap table, so fixtures may use
    /// cmaps larger than the 44-byte single-segment layout.
    fn font_with_cmap(cmap: Vec<u8>) -> Vec<u8> {
        let loca_offset = 260 + cmap.len().div_ceil(4) * 4;
        let glyf_offset = loca_offset + 8;
        let mut bytes = font_with_tables(
            [0, 1, 0, 0],
            &[
                (*b"head", 124, 54),
                (*b"maxp", 180, 32),
                (*b"hhea", 212, 36),
                (*b"hmtx", 248, 10),
                (*b"cmap", 260, cmap.len() as u32),
                (*b"loca", loca_offset as u32, 8),
                (*b"glyf", glyf_offset as u32, 34),
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
        bytes[260..260 + cmap.len()].copy_from_slice(&cmap);
        bytes[loca_offset..loca_offset + 2].copy_from_slice(&0u16.to_be_bytes());
        bytes[loca_offset + 2..loca_offset + 4].copy_from_slice(&9u16.to_be_bytes());
        bytes[loca_offset + 4..loca_offset + 6].copy_from_slice(&9u16.to_be_bytes());
        bytes[loca_offset + 6..loca_offset + 8].copy_from_slice(&17u16.to_be_bytes());
        bytes[glyf_offset..glyf_offset + 2].copy_from_slice(&1i16.to_be_bytes());
        bytes[glyf_offset + 2..glyf_offset + 10].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
        bytes[glyf_offset + 10..glyf_offset + 12].copy_from_slice(&0u16.to_be_bytes());
        bytes[glyf_offset + 12..glyf_offset + 14].copy_from_slice(&0u16.to_be_bytes());
        bytes[glyf_offset + 14] = 0x37;
        bytes[glyf_offset + 15] = 1;
        bytes[glyf_offset + 16] = 2;
        bytes[glyf_offset + 18..glyf_offset + 20].copy_from_slice(&(-1i16).to_be_bytes());
        bytes[glyf_offset + 20..glyf_offset + 28].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
        bytes[glyf_offset + 28..glyf_offset + 30].copy_from_slice(&0x0002u16.to_be_bytes());
        bytes[glyf_offset + 30..glyf_offset + 32].copy_from_slice(&0u16.to_be_bytes());
        bytes
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

    #[test]
    fn reverse_cmap_maps_glyphs_to_unicode() {
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

        // The simple_format4_cmap maps 0x41 ('A') to glyph 2
        // So reverse should map glyph 2 back to 0x41
        let reverse = font
            .cmap
            .reverse_cmap(font.table(b"cmap").unwrap(), font.metadata.glyph_count)
            .unwrap();

        assert_eq!(reverse.get(&2), Some(&0x41));
    }

    #[test]
    fn unicode_for_glyph_returns_codepoint() {
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

        // Glyph 0 should return None (.notdef)
        assert_eq!(font.unicode_for_glyph(0).unwrap(), None);

        // Glyph 2 should map back to 'A' (0x41)
        assert_eq!(font.unicode_for_glyph(2).unwrap(), Some(0x41));

        // Unmapped glyph should return None
        assert_eq!(font.unicode_for_glyph(99).unwrap(), None);
    }

    #[test]
    fn builtin_encoding_maps_codes_through_cmap() {
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
        let encoding = font.builtin_encoding().unwrap();

        // The fixture maps code 0x41 ('A') to glyph 2, which reverses to 0x41.
        assert_eq!(encoding.len(), 256);
        assert_eq!(encoding[0x41], Some('A'));
        assert_eq!(encoding[0x42], None);
        assert_eq!(encoding[0x00], None);
    }

    #[test]
    fn builtin_encoding_maps_codes_through_symbolic_cmap() {
        let font = TrueTypeFont::parse(font_with_cmap(symbolic_format4_cmap())).unwrap();
        assert!(font.cmap.is_symbolic());
        assert_eq!(font.cmap_format(), CmapFormat::Format4);

        let encoding = font.builtin_encoding().unwrap();

        // 0xF041 is absent, so code 0x41 falls back to 0xF000 | 0x41 and
        // reverses through glyph 2 back to U+F041.
        assert_eq!(encoding.len(), 256);
        assert_eq!(encoding[0x41], Some('\u{F041}'));
        assert_eq!(encoding[0x42], None);
        assert_eq!(encoding[0x00], None);
    }

    #[test]
    fn builtin_encoding_maps_codes_through_f100_symbolic_cmap() {
        let cmap = format4_cmap(3, 0, &[(0xF141, 2)]);
        let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
        assert!(font.cmap.is_symbolic());

        let encoding = font.builtin_encoding().unwrap();

        // 0x41 and 0xF041 are absent, so code 0x41 falls through to
        // 0xF100 | 0x41 and reverses through glyph 2 back to U+F141.
        assert_eq!(encoding.len(), 256);
        assert_eq!(encoding[0x41], Some('\u{F141}'));
        assert_eq!(encoding[0x42], None);
        assert_eq!(encoding[0x00], None);
    }

    #[test]
    fn builtin_encoding_maps_codes_through_f200_symbolic_cmap() {
        let cmap = format4_cmap(3, 0, &[(0xF241, 2)]);
        let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
        assert!(font.cmap.is_symbolic());

        let encoding = font.builtin_encoding().unwrap();

        // Only the 0xF200 offset is mapped, so code 0x41 resolves through it.
        assert_eq!(encoding.len(), 256);
        assert_eq!(encoding[0x41], Some('\u{F241}'));
        assert_eq!(encoding[0x42], None);
        assert_eq!(encoding[0x00], None);
    }

    #[test]
    fn builtin_encoding_prefers_f000_offset_over_f100() {
        // 0xF041 and 0xF141 map to different glyphs, so the reverse cmap
        // yields different Unicode scalars for the same character code.
        let cmap = format4_cmap(3, 0, &[(0xF041, 1), (0xF141, 2)]);
        let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
        assert!(font.cmap.is_symbolic());
        assert_eq!(font.unicode_for_glyph(1).unwrap(), Some(0xF041));
        assert_eq!(font.unicode_for_glyph(2).unwrap(), Some(0xF141));

        let encoding = font.builtin_encoding().unwrap();

        // Probing order is 0xF000, 0xF100, 0xF200, so 0xF041 wins even though
        // 0xF141 is also mapped for code 0x41.
        assert_eq!(encoding[0x41], Some('\u{F041}'));
    }

    #[test]
    fn builtin_encoding_ignores_offsets_for_unicode_cmap() {
        // A (3, 1) Unicode subtable that happens to map 0xF041 must not be
        // probed through symbolic offsets for code 0x41.
        let cmap = format4_cmap(3, 1, &[(0xF041, 2)]);
        let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
        assert!(!font.cmap.is_symbolic());

        let encoding = font.builtin_encoding().unwrap();

        assert_eq!(encoding[0x41], None);
        assert_eq!(encoding[0xF0], None);
    }

    fn font_with_vertical_metrics() -> Vec<u8> {
        let mut bytes = font_with_tables(
            [0, 1, 0, 0],
            &[
                (*b"head", 156, 54),
                (*b"maxp", 212, 32),
                (*b"hhea", 244, 36),
                (*b"hmtx", 280, 10),
                (*b"cmap", 292, 44),
                (*b"loca", 336, 8),
                (*b"glyf", 344, 34),
                (*b"vhea", 380, 36),
                (*b"vmtx", 416, 10),
            ],
        );
        // head
        bytes[156 + 12..156 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        bytes[156 + 18..156 + 20].copy_from_slice(&2048u16.to_be_bytes());
        bytes[156 + 50..156 + 52].copy_from_slice(&0i16.to_be_bytes());
        // maxp: 3 glyphs
        bytes[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[216..218].copy_from_slice(&3u16.to_be_bytes());
        // hhea + hmtx (horizontal: 2 long metrics for 3 glyphs)
        bytes[244..248].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[278..280].copy_from_slice(&2u16.to_be_bytes());
        bytes[280..282].copy_from_slice(&500u16.to_be_bytes());
        bytes[284..286].copy_from_slice(&600u16.to_be_bytes());
        // cmap
        let cmap = simple_format4_cmap();
        bytes[292..336].copy_from_slice(&cmap);
        // loca + glyf
        bytes[336..338].copy_from_slice(&0u16.to_be_bytes());
        bytes[338..340].copy_from_slice(&9u16.to_be_bytes());
        bytes[340..342].copy_from_slice(&9u16.to_be_bytes());
        bytes[342..344].copy_from_slice(&17u16.to_be_bytes());
        bytes[344..346].copy_from_slice(&1i16.to_be_bytes());
        bytes[346..354].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
        bytes[354..356].copy_from_slice(&0u16.to_be_bytes());
        bytes[356..358].copy_from_slice(&0u16.to_be_bytes());
        bytes[358] = 0x37;
        bytes[359] = 1;
        bytes[360] = 2;
        bytes[362..364].copy_from_slice(&(-1i16).to_be_bytes());
        bytes[364..372].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
        bytes[372..374].copy_from_slice(&0x0002u16.to_be_bytes());
        bytes[374..376].copy_from_slice(&0u16.to_be_bytes());
        // vhea: version 1.0, numberOfVMetrics = 2
        bytes[380..384].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[384..386].copy_from_slice(&800i16.to_be_bytes());
        bytes[386..388].copy_from_slice(&(-200i16).to_be_bytes());
        bytes[388..390].copy_from_slice(&0i16.to_be_bytes());
        bytes[390..392].copy_from_slice(&1000u16.to_be_bytes());
        bytes[414..416].copy_from_slice(&2u16.to_be_bytes());
        // vmtx: 2 long metrics (advance=4 bytes each) + 1 trailing tsb (2 bytes)
        //   glyph 0: advance=800, tsb=50
        //   glyph 1: advance=900, tsb=60
        //   glyph 2: trailing tsb=70
        bytes[416..418].copy_from_slice(&800u16.to_be_bytes());
        bytes[418..420].copy_from_slice(&50i16.to_be_bytes());
        bytes[420..422].copy_from_slice(&900u16.to_be_bytes());
        bytes[422..424].copy_from_slice(&60i16.to_be_bytes());
        bytes[424..426].copy_from_slice(&70i16.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_vertical_metrics_from_vhea_vmtx() {
        let bytes = font_with_vertical_metrics();
        let font = TrueTypeFont::parse(bytes).unwrap();

        assert_eq!(font.vertical_advance(0), Some(800));
        assert_eq!(font.vertical_advance(1), Some(900));
        assert_eq!(font.top_side_bearing(0), Some(50));
        assert_eq!(font.top_side_bearing(1), Some(60));
        assert_eq!(font.top_side_bearing(2), Some(70));
        assert_eq!(font.vertical_advance(3), None);
        assert_eq!(font.top_side_bearing(3), None);
    }

    #[test]
    fn vertical_advance_reuses_last_metric_for_overflow() {
        let bytes = font_with_vertical_metrics();
        let font = TrueTypeFont::parse(bytes).unwrap();

        // numberOfVMetrics=2, glyph 2 >= 2, so advance reuses glyph 1's value (900)
        assert_eq!(font.vertical_advance(2), Some(900));
    }

    #[test]
    fn missing_vhea_returns_none() {
        let mut bytes = font_with_tables(
            [0, 1, 0, 0],
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
        let font = TrueTypeFont::parse(bytes).unwrap();

        assert_eq!(font.vertical_advance(0), None);
        assert_eq!(font.top_side_bearing(0), None);
    }

    #[test]
    fn truncated_vmtx_returns_error() {
        // vhea declares numberOfVMetrics=2 (needs 10 bytes of vmtx for 3 glyphs)
        // but the vmtx table data is only 5 bytes.
        let mut bytes = font_with_tables(
            [0, 1, 0, 0],
            &[
                (*b"head", 124, 54),
                (*b"maxp", 180, 32),
                (*b"hhea", 212, 36),
                (*b"hmtx", 248, 4),
                (*b"cmap", 252, 44),
                (*b"loca", 296, 4),
                (*b"glyf", 300, 10),
                (*b"vhea", 312, 36),
                (*b"vmtx", 348, 12),
            ],
        );
        bytes[124 + 12..124 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        bytes[124 + 18..124 + 20].copy_from_slice(&1000u16.to_be_bytes());
        bytes[180..184].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[184..186].copy_from_slice(&3u16.to_be_bytes());
        bytes[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[246..248].copy_from_slice(&1u16.to_be_bytes());
        bytes[252..296].copy_from_slice(&simple_format4_cmap());
        bytes[296..298].copy_from_slice(&0u16.to_be_bytes());
        bytes[298..300].copy_from_slice(&5u16.to_be_bytes());
        bytes[300..302].copy_from_slice(&1i16.to_be_bytes());
        bytes[302..310].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
        // vhea: numberOfVMetrics=2 at offset 312+34=346
        bytes[312..316].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes[346..348].copy_from_slice(&2u16.to_be_bytes());
        // vmtx: only write 5 bytes (need 2*4 + 1*2 = 10)
        bytes[348..353].copy_from_slice(&[0; 5]);
        assert!(TrueTypeFont::parse(bytes).is_err());
    }
}
