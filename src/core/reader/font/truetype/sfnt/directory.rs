//! SFNT 头与表目录的读取与校验：签名、记录边界、重叠检查，
//! 以及已校验表的原始字节访问。

use std::collections::BTreeMap;

use super::{
    Cmap, FontError, GlyphLocations, SfntFlavor, TrueTypeFont, TrueTypeMetadata, invalid_sfnt,
};

impl TrueTypeFont {
    /// Parse an SFNT directory from decoded font program bytes.
    ///
    /// This validates the signature, directory boundaries, table tags,
    /// alignment, payload ranges, and required metadata tables. Unicode
    /// character mapping supports cmap formats 4 and 12 for every flavor.
    ///
    /// TrueType-flavored programs (`0x00010000` and `true`) require both the
    /// `loca` and `glyf` tables. A CFF-flavored OpenType program (`OTTO`) may
    /// omit both, in which case it is parsed for metrics and character
    /// mapping only and exposes no glyph outlines.
    ///
    /// It does not validate table checksums or render glyph outlines.
    pub fn parse(data: Vec<u8>) -> Result<Self, FontError> {
        if data.len() < 12 {
            return Err(invalid_sfnt("header is truncated"));
        }

        let flavor = match &data[..4] {
            [0, 1, 0, 0] => SfntFlavor::TrueType,
            b"true" => SfntFlavor::AppleTrue,
            b"OTTO" => SfntFlavor::Cff,
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
        let glyph_locations = match (tables.get(b"loca"), tables.get(b"glyf")) {
            (Some(loca), Some(glyf)) => Some(GlyphLocations::parse(
                &data[loca.clone()],
                &data[glyf.clone()],
                metadata.glyph_count,
                metadata.index_to_loc_format,
            )?),
            (None, None) if flavor == SfntFlavor::Cff => None,
            (None, Some(_)) => return Err(invalid_sfnt("missing required `loca` table")),
            (Some(_), None) => return Err(invalid_sfnt("missing required `glyf` table")),
            (None, None) => return Err(invalid_sfnt("missing required `loca` table")),
        };

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

    /// Return the tags present in the validated table directory.
    pub fn table_tags(&self) -> impl Iterator<Item = &[u8; 4]> {
        self.tables.keys()
    }

    /// Return the bytes for a table tag, if it exists.
    pub fn table(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.tables.get(tag).map(|range| &self.data[range.clone()])
    }
}
