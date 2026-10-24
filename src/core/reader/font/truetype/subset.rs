//! TrueType glyph subsetting for embedded font programs (ISO 32000-1 §9.6.4).
//!
//! [`subset_truetype`] keeps glyph 0, the requested glyph IDs, and every
//! glyph reachable from them through composite-glyph component references.
//! Retained glyphs are renumbered in ascending original-ID order, so glyph 0
//! remains glyph 0 in the subset.
//!
//! The following tables are rewritten: `head`, `hhea`, `maxp`, `hmtx`,
//! `cmap`, `loca`, and `glyf`. The character map is replaced by a single
//! format 4 subtable, recorded as Unicode BMP or as Microsoft Symbol when the
//! source used that encoding. Only character codes that already have a BMP
//! mapping to a retained glyph are copied; format 12 mappings that exist only
//! above U+FFFF are outside the scope of this increment and are dropped.
//!
//! The table whitelist is `head` `hhea` `maxp` `hmtx` `cmap` `loca` `glyf`
//! `name` `OS/2` `cvt ` `fpgm` `prep` `gasp`. The first seven are rewritten
//! as described above; `name`, `OS/2`, and the hinting tables are copied
//! verbatim when present. Tables that contain glyph IDs, glyph counts, or
//! glyph-indexed data are dropped: `post`, `vhea`, `vmtx`, `kern`, `GDEF`,
//! `GSUB`, and `GPOS`, along with every other table outside the whitelist.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use super::super::FontError;
use super::cmap::Cmap;

/// Signature of a standard TrueType-outline SFNT.
const TRUETYPE_SIGNATURE: [u8; 4] = [0, 1, 0, 0];
/// Signature of an Apple TrueType-outline SFNT.
const APPLE_TRUETYPE_SIGNATURE: [u8; 4] = *b"true";
const HEAD_MAGIC: u32 = 0x5F0F_3CF5;
const CHECKSUM_MAGIC: u32 = 0xB1B0_AFBA;
const MAXP_VERSION_1_0: u32 = 0x0001_0000;
const MAXP_VERSION_0_5: u32 = 0x0000_5000;
/// Largest byte offset representable by a short `loca` entry.
const MAX_SHORT_LOCA_OFFSET: usize = 0x1FFFE;

/// Tables that are copied unchanged when present.
const PRESERVED_TABLES: [&[u8; 4]; 13] = [
    b"head", b"hhea", b"maxp", b"hmtx", b"cmap", b"loca", b"glyf", b"name", b"OS/2", b"cvt ",
    b"fpgm", b"prep", b"gasp",
];

/// Tables rebuilt from the subset glyph set.
const REWRITTEN_TABLES: [&[u8; 4]; 7] = [
    b"head", b"hhea", b"maxp", b"hmtx", b"cmap", b"loca", b"glyf",
];

/// Build a TrueType subset containing `glyph_ids` and their composite closure.
///
/// Glyph 0 is always retained. Duplicate and unsorted IDs are accepted; a
/// requested ID outside the source font's glyph range returns
/// [`FontError::Invalid`]. Retained glyphs are renumbered from 0 in ascending
/// original-glyph order, and the rewritten `cmap`, `hmtx`, `loca`, and `glyf`
/// tables refer to those new IDs.
pub fn subset_truetype(data: &[u8], glyph_ids: &[u16]) -> Result<Vec<u8>, FontError> {
    let source = SourceFont::parse(data)?;
    let mapping = source.retained_glyphs(glyph_ids)?;
    let tables = source.build_tables(&mapping)?;
    assemble_sfnt(source.flavor, tables)
}

/// A validated view of the source font's directory and glyph-indexed tables.
struct SourceFont<'a> {
    flavor: [u8; 4],
    tables: BTreeMap<[u8; 4], &'a [u8]>,
    glyph_count: u16,
    index_to_loc_format: i16,
    glyph_ranges: Vec<Range<usize>>,
    glyf: &'a [u8],
    hmtx: &'a [u8],
    horizontal_metric_count: u16,
    cmap: Cmap,
    symbolic_cmap: bool,
}

impl<'a> SourceFont<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, FontError> {
        let (flavor, tables) = parse_directory(data)?;

        let head = required_table(&tables, b"head")?;
        if head.len() < 54 {
            return Err(invalid_subset("`head` table is truncated"));
        }
        if read_u32(head, 12)? != HEAD_MAGIC {
            return Err(invalid_subset("`head` table has an invalid magic number"));
        }
        if !(16..=16_384).contains(&read_u16(head, 18)?) {
            return Err(invalid_subset("`head` table has invalid units per em"));
        }
        let index_to_loc_format = read_i16(head, 50)?;
        if !matches!(index_to_loc_format, 0 | 1) {
            return Err(invalid_subset(
                "`head` table has an invalid indexToLocFormat",
            ));
        }

        let maxp = required_table(&tables, b"maxp")?;
        if maxp.len() < 6 {
            return Err(invalid_subset("`maxp` table is truncated"));
        }
        match read_u32(maxp, 0)? {
            MAXP_VERSION_1_0 => {
                if maxp.len() < 32 {
                    return Err(invalid_subset("`maxp` table is truncated"));
                }
            }
            // Version 0.5 is a six-byte table with only the glyph count.
            MAXP_VERSION_0_5 => {}
            _ => return Err(invalid_subset("`maxp` table has an unsupported version")),
        }
        let glyph_count = read_u16(maxp, 4)?;
        if glyph_count == 0 {
            return Err(invalid_subset("`maxp` table declares no glyphs"));
        }

        let hhea = required_table(&tables, b"hhea")?;
        if hhea.len() < 36 {
            return Err(invalid_subset("`hhea` table is truncated"));
        }
        if read_u32(hhea, 0)? != MAXP_VERSION_1_0 {
            return Err(invalid_subset("`hhea` table has an unsupported version"));
        }
        let horizontal_metric_count = read_u16(hhea, 34)?;
        if horizontal_metric_count == 0 || horizontal_metric_count > glyph_count {
            return Err(invalid_subset(
                "`hhea` table has an invalid numberOfHMetrics",
            ));
        }

        let hmtx = required_table(&tables, b"hmtx")?;
        let required_hmtx_length = usize::from(horizontal_metric_count) * 4
            + usize::from(glyph_count - horizontal_metric_count) * 2;
        if hmtx.len() < required_hmtx_length {
            return Err(invalid_subset("`hmtx` table is truncated"));
        }

        let loca = required_table(&tables, b"loca")?;
        let glyph_ranges = parse_loca(loca, glyph_count, index_to_loc_format)?;
        let glyf = required_table(&tables, b"glyf")?;
        if glyph_ranges
            .last()
            .is_some_and(|range| range.end > glyf.len())
        {
            return Err(invalid_subset("`loca` offsets exceed the `glyf` table"));
        }

        let cmap_data = required_table(&tables, b"cmap")?;
        let cmap = Cmap::parse(cmap_data, glyph_count)?;
        let symbolic_cmap = cmap.is_symbolic();

        Ok(Self {
            flavor,
            tables,
            glyph_count,
            index_to_loc_format,
            glyph_ranges,
            glyf,
            hmtx,
            horizontal_metric_count,
            cmap,
            symbolic_cmap,
        })
    }

    fn source_table(&self, tag: &[u8; 4]) -> &'a [u8] {
        self.tables
            .get(tag)
            .copied()
            .expect("required table was validated")
    }

    fn glyph_record(&self, glyph_id: u16) -> Result<&'a [u8], FontError> {
        let range = self
            .glyph_ranges
            .get(usize::from(glyph_id))
            .ok_or_else(|| invalid_subset("glyph ID is outside the font"))?;
        self.glyf
            .get(range.clone())
            .ok_or_else(|| invalid_subset("glyph record is outside the `glyf` table"))
    }

    /// Return the retained glyphs and their old- to new-glyph mapping.
    fn retained_glyphs(&self, requested: &[u16]) -> Result<BTreeMap<u16, u16>, FontError> {
        let mut retained = BTreeSet::new();
        retained.insert(0u16);
        for &glyph_id in requested {
            if glyph_id >= self.glyph_count {
                return Err(invalid_subset(format!(
                    "glyph ID {glyph_id} is outside the font's {} glyphs",
                    self.glyph_count
                )));
            }
            retained.insert(glyph_id);
        }

        let mut queue: Vec<u16> = retained.iter().copied().collect();
        let mut next = 0;
        while next < queue.len() {
            let glyph_id = queue[next];
            next += 1;
            let record = self.glyph_record(glyph_id)?;
            if record.is_empty() {
                continue;
            }
            if glyph_contour_count(record)? != -1 {
                continue;
            }
            for (_, component) in composite_component_refs(record)? {
                if component >= self.glyph_count {
                    return Err(invalid_subset(format!(
                        "composite glyph references glyph ID {component} outside the font"
                    )));
                }
                if retained.insert(component) {
                    queue.push(component);
                }
            }
        }

        Ok(retained
            .into_iter()
            .enumerate()
            .map(|(index, glyph_id)| {
                (
                    glyph_id,
                    u16::try_from(index).expect("subset glyph count fits u16"),
                )
            })
            .collect())
    }

    fn build_tables(
        &self,
        mapping: &BTreeMap<u16, u16>,
    ) -> Result<BTreeMap<[u8; 4], Vec<u8>>, FontError> {
        let order: Vec<u16> = mapping.keys().copied().collect();
        let glyph_count =
            u16::try_from(order.len()).map_err(|_| invalid_subset("subset has too many glyphs"))?;

        let (glyf, offsets) = self.build_glyf(&order, mapping)?;
        let (loca, loca_format) = build_loca(&offsets, self.index_to_loc_format, glyf.len())?;
        let hmtx = self.build_hmtx(&order)?;
        let cmap = self.build_cmap(mapping)?;

        let mut head = self.source_table(b"head").to_vec();
        head[8..12].fill(0);
        head[50..52].copy_from_slice(&loca_format.to_be_bytes());

        let mut maxp = self.source_table(b"maxp").to_vec();
        maxp[4..6].copy_from_slice(&glyph_count.to_be_bytes());

        let mut hhea = self.source_table(b"hhea").to_vec();
        hhea[34..36].copy_from_slice(&glyph_count.to_be_bytes());

        let mut tables = BTreeMap::new();
        for tag in PRESERVED_TABLES {
            if is_rewritten(tag) {
                continue;
            }
            if let Some(bytes) = self.tables.get(tag) {
                tables.insert(*tag, bytes.to_vec());
            }
        }
        for (tag, bytes) in [
            (*b"head", head),
            (*b"hhea", hhea),
            (*b"maxp", maxp),
            (*b"hmtx", hmtx),
            (*b"cmap", cmap),
            (*b"loca", loca),
            (*b"glyf", glyf),
        ] {
            tables.insert(tag, bytes);
        }
        Ok(tables)
    }

    fn build_glyf(
        &self,
        order: &[u16],
        mapping: &BTreeMap<u16, u16>,
    ) -> Result<(Vec<u8>, Vec<usize>), FontError> {
        let mut glyf = Vec::new();
        let mut offsets = Vec::with_capacity(order.len() + 1);

        for &glyph_id in order {
            let record = self.glyph_record(glyph_id)?;
            if record.is_empty() {
                offsets.push(glyf.len());
                continue;
            }
            let record = if glyph_contour_count(record)? == -1 {
                let mut rewritten = record.to_vec();
                for (position, component) in composite_component_refs(record)? {
                    let new_glyph_id = mapping.get(&component).ok_or_else(|| {
                        invalid_subset("composite component is missing from the subset")
                    })?;
                    rewritten[position..position + 2].copy_from_slice(&new_glyph_id.to_be_bytes());
                }
                rewritten
            } else {
                record.to_vec()
            };
            offsets.push(glyf.len());
            glyf.extend_from_slice(&record);
        }
        offsets.push(glyf.len());
        Ok((glyf, offsets))
    }

    fn horizontal_metric(&self, glyph_id: u16) -> Result<(u16, i16), FontError> {
        let metric_count = usize::from(self.horizontal_metric_count);
        let index = usize::from(glyph_id);
        let advance = if index < metric_count {
            read_u16(self.hmtx, index * 4)?
        } else {
            read_u16(self.hmtx, (metric_count - 1) * 4)?
        };
        let side_bearing = if index < metric_count {
            read_i16(self.hmtx, index * 4 + 2)?
        } else {
            read_i16(self.hmtx, metric_count * 4 + (index - metric_count) * 2)?
        };
        Ok((advance, side_bearing))
    }

    fn build_hmtx(&self, order: &[u16]) -> Result<Vec<u8>, FontError> {
        let mut hmtx = Vec::with_capacity(order.len() * 4);
        for &glyph_id in order {
            let (advance, side_bearing) = self.horizontal_metric(glyph_id)?;
            hmtx.extend_from_slice(&advance.to_be_bytes());
            hmtx.extend_from_slice(&side_bearing.to_be_bytes());
        }
        Ok(hmtx)
    }

    fn build_cmap(&self, mapping: &BTreeMap<u16, u16>) -> Result<Vec<u8>, FontError> {
        let cmap_data = self.source_table(b"cmap");
        let mut entries = Vec::new();
        for code in 0..=u32::from(u16::MAX) {
            let Some(glyph_id) = self.cmap.glyph_id(cmap_data, code, self.glyph_count)? else {
                continue;
            };
            // Missing characters and the reserved 0xFFFF sentinel are omitted.
            if glyph_id == 0 || code == 0xFFFF {
                continue;
            }
            if let Some(&new_glyph_id) = mapping.get(&glyph_id) {
                entries.push((code as u16, new_glyph_id));
            }
        }
        build_format4_cmap(&entries, self.symbolic_cmap)
    }
}

type TableMap<'a> = BTreeMap<[u8; 4], &'a [u8]>;

fn parse_directory(data: &[u8]) -> Result<([u8; 4], TableMap<'_>), FontError> {
    if data.len() < 12 {
        return Err(invalid_subset("SFNT header is truncated"));
    }
    let flavor: [u8; 4] = data[..4]
        .try_into()
        .expect("SFNT signature has a fixed-width slice");
    if flavor != TRUETYPE_SIGNATURE && flavor != APPLE_TRUETYPE_SIGNATURE {
        return Err(invalid_subset("unsupported SFNT signature"));
    }
    let table_count = usize::from(read_u16(data, 4)?);
    if table_count == 0 {
        return Err(invalid_subset("table directory is empty"));
    }
    let directory_length = table_count
        .checked_mul(16)
        .and_then(|length| length.checked_add(12))
        .ok_or_else(|| invalid_subset("table directory length overflows"))?;
    if directory_length > data.len() {
        return Err(invalid_subset("table directory is truncated"));
    }

    let mut tables = TableMap::new();
    let mut ranges = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let record = 12 + index * 16;
        let tag: [u8; 4] = data[record..record + 4]
            .try_into()
            .expect("table tag has a fixed-width slice");
        if tables.contains_key(&tag) {
            return Err(invalid_subset("table directory contains a duplicate tag"));
        }
        let offset = read_u32(data, record + 8)? as usize;
        let length = read_u32(data, record + 12)? as usize;
        if !offset.is_multiple_of(4) {
            return Err(invalid_subset("table offset is not four-byte aligned"));
        }
        if offset < directory_length {
            return Err(invalid_subset("table data overlaps the SFNT directory"));
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| invalid_subset("table range overflows"))?;
        if end > data.len() {
            return Err(invalid_subset("table data is outside the font program"));
        }
        if length > 0 {
            ranges.push((offset, end));
        }
        tables.insert(tag, &data[offset..end]);
    }

    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(invalid_subset("table data ranges overlap"));
    }
    Ok((flavor, tables))
}

fn required_table<'a>(
    tables: &BTreeMap<[u8; 4], &'a [u8]>,
    tag: &[u8; 4],
) -> Result<&'a [u8], FontError> {
    tables.get(tag).copied().ok_or_else(|| {
        invalid_subset(format!(
            "missing required `{}` table",
            String::from_utf8_lossy(tag)
        ))
    })
}

fn parse_loca(
    loca: &[u8],
    glyph_count: u16,
    index_to_loc_format: i16,
) -> Result<Vec<Range<usize>>, FontError> {
    let entry_size = match index_to_loc_format {
        0 => 2,
        1 => 4,
        _ => return Err(invalid_subset("unsupported indexToLocFormat")),
    };
    let entry_count = usize::from(glyph_count) + 1;
    let required_length = entry_count
        .checked_mul(entry_size)
        .ok_or_else(|| invalid_subset("`loca` table length overflows"))?;
    if loca.len() < required_length {
        return Err(invalid_subset("`loca` table is truncated"));
    }

    let mut offsets = Vec::with_capacity(entry_count);
    let mut previous_offset = 0usize;
    for index in 0..entry_count {
        let offset = match index_to_loc_format {
            0 => usize::from(read_u16(loca, index * 2)?) * 2,
            1 => usize::try_from(read_u32(loca, index * 4)?)
                .map_err(|_| invalid_subset("glyph offset does not fit this platform"))?,
            _ => unreachable!("indexToLocFormat was validated"),
        };
        if offset < previous_offset {
            return Err(invalid_subset("`loca` offsets are not monotonic"));
        }
        offsets.push(offset);
        previous_offset = offset;
    }

    Ok(offsets.windows(2).map(|pair| pair[0]..pair[1]).collect())
}

fn glyph_contour_count(record: &[u8]) -> Result<i16, FontError> {
    if record.len() < 10 {
        return Err(invalid_subset("glyph header is truncated"));
    }
    let contours = read_i16(record, 0)?;
    if contours < -1 {
        return Err(invalid_subset("glyph has an invalid contour count"));
    }
    Ok(contours)
}

/// Return `(glyphIndex offset, component glyph ID)` for a composite glyph.
fn composite_component_refs(record: &[u8]) -> Result<Vec<(usize, u16)>, FontError> {
    let mut cursor = 10usize;
    let mut components = Vec::new();
    let mut flags;
    loop {
        flags = read_u16(record, cursor)?;
        let glyph_offset = cursor
            .checked_add(2)
            .ok_or_else(|| invalid_subset("composite glyph offset overflows"))?;
        let component = read_u16(record, glyph_offset)?;
        if flags & 0xE000 != 0 {
            return Err(invalid_subset("composite component has reserved flags set"));
        }
        if flags & 0x1800 == 0x1800 {
            return Err(invalid_subset(
                "composite component has conflicting offset flags",
            ));
        }
        let scale_flags = flags & 0x00C8;
        if scale_flags.count_ones() > 1 {
            return Err(invalid_subset(
                "composite component has conflicting transform flags",
            ));
        }
        advance(record, &mut cursor, 4)?;
        advance(record, &mut cursor, if flags & 0x0001 != 0 { 4 } else { 2 })?;
        advance(
            record,
            &mut cursor,
            match scale_flags {
                0 => 0,
                0x0008 => 2,
                0x0040 => 4,
                0x0080 => 8,
                _ => unreachable!("conflicting scale flags were rejected"),
            },
        )?;
        components.push((glyph_offset, component));
        if flags & 0x0020 == 0 {
            break;
        }
    }
    if flags & 0x0100 != 0 {
        let instruction_length = usize::from(read_u16(record, cursor)?);
        advance(
            record,
            &mut cursor,
            instruction_length
                .checked_add(2)
                .ok_or_else(|| invalid_subset("composite instruction range overflows"))?,
        )?;
    }
    Ok(components)
}

fn advance(data: &[u8], cursor: &mut usize, count: usize) -> Result<(), FontError> {
    let next = cursor
        .checked_add(count)
        .ok_or_else(|| invalid_subset("composite glyph offset overflows"))?;
    if next > data.len() {
        return Err(invalid_subset("composite glyph record is truncated"));
    }
    *cursor = next;
    Ok(())
}

fn build_loca(
    offsets: &[usize],
    source_format: i16,
    glyf_length: usize,
) -> Result<(Vec<u8>, i16), FontError> {
    let use_short = source_format == 0
        && glyf_length <= MAX_SHORT_LOCA_OFFSET
        && offsets.iter().all(|offset| offset % 2 == 0);
    if use_short {
        let mut loca = Vec::with_capacity(offsets.len() * 2);
        for &offset in offsets {
            let short = u16::try_from(offset / 2)
                .map_err(|_| invalid_subset("short `loca` offset exceeds u16"))?;
            loca.extend_from_slice(&short.to_be_bytes());
        }
        Ok((loca, 0))
    } else {
        let mut loca = Vec::with_capacity(offsets.len() * 4);
        for &offset in offsets {
            let long = u32::try_from(offset)
                .map_err(|_| invalid_subset("long `loca` offset exceeds u32"))?;
            loca.extend_from_slice(&long.to_be_bytes());
        }
        Ok((loca, 1))
    }
}

fn build_format4_cmap(entries: &[(u16, u16)], symbolic: bool) -> Result<Vec<u8>, FontError> {
    let segment_count = entries.len() + 1;
    let length = segment_count
        .checked_mul(8)
        .and_then(|length| length.checked_add(16))
        .and_then(|length| length.checked_add(entries.len() * 2))
        .ok_or_else(|| invalid_subset("cmap format 4 subtable length overflows"))?;
    if length > usize::from(u16::MAX) {
        return Err(invalid_subset("cmap format 4 subtable exceeds 65535 bytes"));
    }

    let segment_count_x2 = u16::try_from(segment_count * 2)
        .map_err(|_| invalid_subset("cmap format 4 segment count exceeds u16"))?;
    let (search_range, entry_selector, range_shift) = cmap_search_parameters(segment_count)?;

    let mut subtable = vec![0u8; length];
    subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
    subtable[2..4].copy_from_slice(&(length as u16).to_be_bytes());
    // language stays zero
    subtable[6..8].copy_from_slice(&segment_count_x2.to_be_bytes());
    subtable[8..10].copy_from_slice(&search_range.to_be_bytes());
    subtable[10..12].copy_from_slice(&entry_selector.to_be_bytes());
    subtable[12..14].copy_from_slice(&range_shift.to_be_bytes());

    let end_codes = 14;
    let start_codes = 16 + segment_count * 2;
    let deltas = 16 + segment_count * 4;
    let range_offsets = 16 + segment_count * 6;
    let glyph_array = 16 + segment_count * 8;
    // Every real segment has exactly one glyphIdArray entry, so the distance
    // from idRangeOffset[i] to its entry is 2 * segCount for every index.
    let range_offset = segment_count_x2;

    for (index, (code, glyph_id)) in entries.iter().enumerate() {
        subtable[end_codes + index * 2..end_codes + index * 2 + 2]
            .copy_from_slice(&code.to_be_bytes());
        subtable[start_codes + index * 2..start_codes + index * 2 + 2]
            .copy_from_slice(&code.to_be_bytes());
        subtable[range_offsets + index * 2..range_offsets + index * 2 + 2]
            .copy_from_slice(&range_offset.to_be_bytes());
        subtable[glyph_array + index * 2..glyph_array + index * 2 + 2]
            .copy_from_slice(&glyph_id.to_be_bytes());
    }

    let sentinel = entries.len();
    subtable[end_codes + sentinel * 2..end_codes + sentinel * 2 + 2]
        .copy_from_slice(&u16::MAX.to_be_bytes());
    subtable[start_codes + sentinel * 2..start_codes + sentinel * 2 + 2]
        .copy_from_slice(&u16::MAX.to_be_bytes());
    // idDelta = 1 maps the sentinel to glyph 0; idRangeOffset stays zero.
    subtable[deltas + sentinel * 2..deltas + sentinel * 2 + 2].copy_from_slice(&1u16.to_be_bytes());

    let mut cmap = vec![0u8; 12 + subtable.len()];
    cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
    cmap[4..6].copy_from_slice(&3u16.to_be_bytes());
    // Keep the Microsoft Symbol encoding so single-byte codes still resolve
    // through the 0xF000-based fallback used by PDF's built-in encoding.
    cmap[6..8].copy_from_slice(&(if symbolic { 0u16 } else { 1u16 }).to_be_bytes());
    cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
    cmap[12..].copy_from_slice(&subtable);
    Ok(cmap)
}

fn cmap_search_parameters(segment_count: usize) -> Result<(u16, u16, u16), FontError> {
    let mut power = 1usize;
    let mut entry_selector = 0u16;
    while power * 2 <= segment_count {
        power *= 2;
        entry_selector += 1;
    }
    let search_range =
        u16::try_from(power * 2).map_err(|_| invalid_subset("cmap search range exceeds u16"))?;
    let range_shift = segment_count
        .checked_mul(2)
        .and_then(|value| value.checked_sub(usize::from(search_range)))
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| invalid_subset("cmap range shift exceeds u16"))?;
    Ok((search_range, entry_selector, range_shift))
}

fn assemble_sfnt(
    flavor: [u8; 4],
    tables: BTreeMap<[u8; 4], Vec<u8>>,
) -> Result<Vec<u8>, FontError> {
    let table_count = u16::try_from(tables.len())
        .map_err(|_| invalid_subset("too many SFNT tables in the subset"))?;
    let directory_length = 12usize
        .checked_add(usize::from(table_count) * 16)
        .ok_or_else(|| invalid_subset("table directory length overflows"))?;

    let mut records = Vec::with_capacity(tables.len());
    let mut bytes = vec![0u8; directory_length];
    let mut cursor = directory_length;
    for (tag, table) in &tables {
        let offset = align4(cursor);
        let length =
            u32::try_from(table.len()).map_err(|_| invalid_subset("table length exceeds u32"))?;
        let offset_u32 =
            u32::try_from(offset).map_err(|_| invalid_subset("table offset exceeds u32"))?;
        records.push((*tag, table_checksum(table), offset_u32, length));
        bytes.resize(offset, 0);
        bytes.extend_from_slice(table);
        cursor = bytes.len();
    }
    bytes.resize(align4(bytes.len()), 0);

    let (search_range, entry_selector, range_shift) =
        sfnt_search_parameters(usize::from(table_count));
    bytes[..4].copy_from_slice(&flavor);
    bytes[4..6].copy_from_slice(&table_count.to_be_bytes());
    bytes[6..8].copy_from_slice(&search_range.to_be_bytes());
    bytes[8..10].copy_from_slice(&entry_selector.to_be_bytes());
    bytes[10..12].copy_from_slice(&range_shift.to_be_bytes());

    let mut head_offset = None;
    for (index, (tag, checksum, offset, length)) in records.iter().enumerate() {
        let record = 12 + index * 16;
        bytes[record..record + 4].copy_from_slice(tag);
        bytes[record + 4..record + 8].copy_from_slice(&checksum.to_be_bytes());
        bytes[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
        bytes[record + 12..record + 16].copy_from_slice(&length.to_be_bytes());
        if tag == b"head" {
            head_offset = Some(*offset as usize);
        }
    }

    let adjustment = CHECKSUM_MAGIC.wrapping_sub(table_checksum(&bytes));
    let head_offset = head_offset.ok_or_else(|| invalid_subset("subset is missing `head`"))?;
    let adjustment_start = head_offset
        .checked_add(8)
        .ok_or_else(|| invalid_subset("`head` table offset overflows"))?;
    let adjustment_end = adjustment_start
        .checked_add(4)
        .ok_or_else(|| invalid_subset("`head` table offset overflows"))?;
    let destination = bytes
        .get_mut(adjustment_start..adjustment_end)
        .ok_or_else(|| invalid_subset("`head` table is truncated"))?;
    destination.copy_from_slice(&adjustment.to_be_bytes());
    Ok(bytes)
}

fn sfnt_search_parameters(table_count: usize) -> (u16, u16, u16) {
    let mut power = 1usize;
    let mut entry_selector = 0u16;
    while power * 2 <= table_count {
        power *= 2;
        entry_selector += 1;
    }
    let search_range = u16::try_from(power * 16).expect("subset table count fits u16");
    let range_shift =
        u16::try_from(table_count * 16 - power * 16).expect("subset table count fits u16");
    (search_range, entry_selector, range_shift)
}

fn table_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in data.chunks(4) {
        let mut padded = [0u8; 4];
        padded[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(padded));
    }
    sum
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

fn is_rewritten(tag: &[u8; 4]) -> bool {
    REWRITTEN_TABLES.contains(&tag)
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| invalid_subset("font offset overflows"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| invalid_subset("font data is truncated"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| invalid_subset("font offset overflows"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| invalid_subset("font data is truncated"))?;
    Ok(i16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| invalid_subset("font offset overflows"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| invalid_subset("font data is truncated"))?;
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

fn invalid_subset(message: impl Into<String>) -> FontError {
    FontError::Invalid(format!("invalid TrueType subset: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::super::TrueTypeFont;
    use super::subset_truetype;
    use crate::core::reader::font::FontError;

    const GLYPH_COUNT: u16 = 4;

    #[test]
    fn subsets_simple_glyph_and_remaps_cmap() {
        let bytes = fixture_font(1, true);
        let original = TrueTypeFont::parse(bytes.clone()).unwrap();
        let subset_bytes = subset_truetype(&bytes, &[3, 0, 3]).unwrap();
        let subset = TrueTypeFont::parse(subset_bytes).unwrap();

        assert_eq!(subset.glyph_count(), 2);
        assert_eq!(original.glyph_id_for_unicode(0x42).unwrap(), Some(3));
        assert_eq!(subset.glyph_id_for_unicode(0x42).unwrap(), Some(1));
        assert_ne!(subset.glyph_id_for_unicode(0x42).unwrap(), Some(3));
        assert_eq!(subset.glyph_id_for_unicode(0x41).unwrap(), None);

        let original_glyph = original.glyph(3).unwrap().unwrap();
        let subset_glyph = subset.glyph(1).unwrap().unwrap();
        assert_eq!(
            subset_glyph.number_of_contours,
            original_glyph.number_of_contours
        );
        assert_eq!(subset_glyph.bounds, original_glyph.bounds);
        assert_eq!(subset_glyph.points, original_glyph.points);
        assert_eq!(
            subset.horizontal_advance_width(1),
            original.horizontal_advance_width(3)
        );
        assert_eq!(
            subset.horizontal_advance_width(0),
            original.horizontal_advance_width(0)
        );
        assert!(subset.glyph(0).unwrap().is_some());

        let hhea = subset.table(b"hhea").unwrap();
        assert_eq!(
            u16::from_be_bytes(hhea[34..36].try_into().unwrap()),
            2,
            "numberOfHMetrics must equal the subset glyph count"
        );
    }

    #[test]
    fn includes_composite_component_closure() {
        let bytes = fixture_font(1, true);
        let original = TrueTypeFont::parse(bytes.clone()).unwrap();
        let subset_bytes = subset_truetype(&bytes, &[2]).unwrap();
        let subset = TrueTypeFont::parse(subset_bytes).unwrap();

        // Glyph 3 is not requested but is a component of glyph 2. Reaching it
        // also proves the component glyphIndex was patched to the new ID.
        assert_eq!(subset.glyph_count(), 3);
        let component = original.glyph(3).unwrap().unwrap();
        assert_eq!(subset.glyph(2).unwrap().unwrap().points, component.points);

        let original_composite = original.glyph(2).unwrap().unwrap();
        let subset_composite = subset.glyph(1).unwrap().unwrap();
        assert_eq!(subset_composite.number_of_contours, -1);
        assert_eq!(subset_composite.points, original_composite.points);
        assert_eq!(subset_composite.points.len(), 1);
        assert_eq!(
            (subset_composite.points[0].x, subset_composite.points[0].y),
            (7, 8)
        );
        assert_eq!(subset.glyph_id_for_unicode(0x41).unwrap(), Some(1));
    }

    #[test]
    fn rejects_out_of_range_glyphs_and_handles_empty_request() {
        let bytes = fixture_font(1, true);

        let error = subset_truetype(&bytes, &[GLYPH_COUNT]).unwrap_err();
        assert!(matches!(error, FontError::Invalid(_)));
        assert!(subset_truetype(&bytes, &[0, GLYPH_COUNT]).is_err());
        assert!(subset_truetype(&bytes, &[u16::MAX]).is_err());

        let subset_bytes = subset_truetype(&bytes, &[]).unwrap();
        let subset = TrueTypeFont::parse(subset_bytes).unwrap();
        assert_eq!(subset.glyph_count(), 1);
        assert_eq!(subset.glyph_id_for_unicode(0x41).unwrap(), None);
        assert_eq!(subset.glyph_id_for_unicode(0x42).unwrap(), None);
        assert!(subset.glyph(0).unwrap().is_some());
        assert_eq!(subset.horizontal_advance_width(0), Some(500));
    }

    #[test]
    fn rebuilds_aligned_directory_and_checksums() {
        let bytes = fixture_font(1, true);
        let subset = subset_truetype(&bytes, &[3]).unwrap();
        let table_count = usize::from(be_u16(&subset, 4));
        let directory_end = 12 + table_count * 16;
        let mut previous_tag = None;

        for index in 0..table_count {
            let record = 12 + index * 16;
            let tag: [u8; 4] = subset[record..record + 4].try_into().unwrap();
            if let Some(previous) = previous_tag {
                assert!(previous < tag, "table directory tags must be sorted");
            }
            previous_tag = Some(tag);

            let stored_checksum = be_u32(&subset, record + 4);
            let offset = be_u32(&subset, record + 8) as usize;
            let length = be_u32(&subset, record + 12) as usize;
            assert_eq!(offset % 4, 0, "table {tag:?} is not four-byte aligned");
            assert!(offset >= directory_end);
            assert!(offset + length <= subset.len());

            let table = &subset[offset..offset + length];
            let expected = if tag == *b"head" {
                let mut head = table.to_vec();
                head[8..12].fill(0);
                checksum(&head)
            } else {
                checksum(table)
            };
            assert_eq!(stored_checksum, expected, "bad checksum for {tag:?}");
        }

        assert_eq!(checksum(&subset), 0xB1B0_AFBA);
        let font = TrueTypeFont::parse(subset).unwrap();
        assert_eq!(font.glyph_count(), 2);
        let head = font.table(b"head").unwrap();
        assert_eq!(i16::from_be_bytes(head[50..52].try_into().unwrap()), 1);
    }

    #[test]
    fn writes_format4_segments_and_search_parameters() {
        let bytes = fixture_font(1, true);
        let subset = subset_truetype(&bytes, &[3]).unwrap();
        let cmap = table(&subset, b"cmap");
        let subtable = &cmap[12..];

        // One real segment ('B' -> new glyph 1) plus the 0xFFFF sentinel.
        assert_eq!(be_u16(subtable, 0), 4);
        assert_eq!(usize::from(be_u16(subtable, 2)), subtable.len());
        assert_eq!(be_u16(subtable, 4), 0, "format 4 language");
        assert_eq!(be_u16(subtable, 6), 4, "segCountX2");
        assert_eq!(be_u16(subtable, 8), 4, "searchRange");
        assert_eq!(be_u16(subtable, 10), 1, "entrySelector");
        assert_eq!(be_u16(subtable, 12), 0, "rangeShift");

        // Two segments place the arrays at these fixed offsets.
        assert_eq!(be_u16(subtable, 14), 0x42);
        assert_eq!(be_u16(subtable, 16), 0xFFFF);
        assert_eq!(be_u16(subtable, 20), 0x42);
        assert_eq!(be_u16(subtable, 22), 0xFFFF);
        assert_eq!(be_u16(subtable, 24), 0, "idDelta for the real segment");
        assert_eq!(be_u16(subtable, 26), 1, "idDelta for the sentinel");
        assert_eq!(
            be_u16(subtable, 28),
            4,
            "idRangeOffset points 2 * segCount bytes ahead"
        );
        assert_eq!(be_u16(subtable, 30), 0, "sentinel range offset");
        assert_eq!(be_u16(subtable, 32), 1, "glyphIdArray maps to the new ID");
    }

    #[test]
    fn drops_tables_that_depend_on_original_glyphs() {
        let bytes = fixture_font(1, true);
        let subset = subset_truetype(&bytes, &[3]).unwrap();
        let font = TrueTypeFont::parse(subset).unwrap();

        assert!(font.table(b"post").is_none());
        assert!(!font.table_tags().any(|tag| *tag == *b"post"));
        assert!(font.table(b"name").is_some());
    }

    #[test]
    fn preserves_short_loca_format() {
        let bytes = fixture_font(0, true);
        let subset_bytes = subset_truetype(&bytes, &[3]).unwrap();
        let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();

        assert_eq!(subset.glyph_count(), 2);
        let head = table(&subset_bytes, b"head");
        assert_eq!(
            i16::from_be_bytes(head[50..52].try_into().unwrap()),
            0,
            "short input loca should stay short"
        );
        assert_eq!(table(&subset_bytes, b"loca").len(), 6);
    }

    #[test]
    fn preserves_symbolic_cmap_encoding() {
        let bytes = fixture_font_with_cmap(cmap_table_for(3, 0, &[(0xF041, 2), (0xF042, 3)]));
        let subset_bytes = subset_truetype(&bytes, &[3]).unwrap();
        let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();

        // The (3, 0) symbol encoding survives, so 0xF000-based PDF byte
        // lookups keep working after subsetting.
        let cmap = table(&subset_bytes, b"cmap");
        assert_eq!(be_u16(cmap, 4), 3);
        assert_eq!(be_u16(cmap, 6), 0);
        assert_eq!(subset.glyph_id_for_unicode(0xF042).unwrap(), Some(1));
        assert_eq!(subset.glyph_id_for_unicode(0x42).unwrap(), None);
    }

    #[test]
    fn supports_maxp_version_half() {
        let bytes = fixture_font_with_maxp(1, false, half_maxp_table(GLYPH_COUNT));
        let subset_bytes = subset_truetype(&bytes, &[3]).unwrap();
        let maxp = table(&subset_bytes, b"maxp");

        // Version 0.5 is the six-byte maxp layout; it stays six bytes.
        assert_eq!(maxp.len(), 6);
        assert_eq!(be_u32(maxp, 0), 0x0000_5000);
        assert_eq!(be_u16(maxp, 4), 2);
    }

    fn fixture_font(index_to_loc_format: i16, include_post: bool) -> Vec<u8> {
        fixture_font_with_maxp(index_to_loc_format, include_post, maxp_table(GLYPH_COUNT))
    }

    fn fixture_font_with_maxp(
        index_to_loc_format: i16,
        include_post: bool,
        maxp: Vec<u8>,
    ) -> Vec<u8> {
        build_sfnt(fixture_tables(index_to_loc_format, include_post, maxp))
    }

    fn fixture_font_with_cmap(cmap: Vec<u8>) -> Vec<u8> {
        let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
        if let Some(entry) = tables.iter_mut().find(|(tag, _)| *tag == *b"cmap") {
            entry.1 = cmap;
        }
        build_sfnt(tables)
    }

    fn fixture_tables(
        index_to_loc_format: i16,
        include_post: bool,
        maxp: Vec<u8>,
    ) -> Vec<([u8; 4], Vec<u8>)> {
        let (glyf, loca) = glyf_and_loca(index_to_loc_format);
        let mut tables = vec![
            (*b"head", head_table(index_to_loc_format)),
            (*b"maxp", maxp),
            (*b"hhea", hhea_table()),
            (*b"hmtx", hmtx_table()),
            (*b"cmap", cmap_table()),
            (*b"loca", loca),
            (*b"glyf", glyf),
            (*b"name", b"subset-test".to_vec()),
        ];
        if include_post {
            tables.push((*b"post", vec![0; 32]));
        }
        tables
    }

    fn glyf_and_loca(index_to_loc_format: i16) -> (Vec<u8>, Vec<u8>) {
        let glyphs = [
            simple_glyph(1, 2),
            simple_glyph(10, 20),
            composite_glyph(3, 0, 0),
            simple_glyph(7, 8),
        ];
        let mut glyf = Vec::new();
        let mut offsets = Vec::new();
        for glyph in &glyphs {
            offsets.push(glyf.len());
            glyf.extend_from_slice(glyph);
        }
        offsets.push(glyf.len());

        let loca = if index_to_loc_format == 0 {
            let mut loca = Vec::with_capacity(offsets.len() * 2);
            for offset in offsets {
                loca.extend_from_slice(&(u16::try_from(offset / 2).unwrap()).to_be_bytes());
            }
            loca
        } else {
            let mut loca = Vec::with_capacity(offsets.len() * 4);
            for offset in offsets {
                loca.extend_from_slice(&(u32::try_from(offset).unwrap()).to_be_bytes());
            }
            loca
        };
        (glyf, loca)
    }

    fn simple_glyph(x: u8, y: u8) -> Vec<u8> {
        let mut glyph = vec![0u8; 18];
        glyph[..2].copy_from_slice(&1i16.to_be_bytes());
        glyph[2..4].copy_from_slice(&i16::from(x).to_be_bytes());
        glyph[4..6].copy_from_slice(&i16::from(y).to_be_bytes());
        glyph[6..8].copy_from_slice(&i16::from(x).to_be_bytes());
        glyph[8..10].copy_from_slice(&i16::from(y).to_be_bytes());
        glyph[10..12].copy_from_slice(&0u16.to_be_bytes());
        glyph[12..14].copy_from_slice(&0u16.to_be_bytes());
        glyph[14] = 0x37;
        glyph[15] = x;
        glyph[16] = y;
        glyph
    }

    fn composite_glyph(component: u16, dx: i16, dy: i16) -> Vec<u8> {
        let mut glyph = vec![0u8; 18];
        glyph[..2].copy_from_slice(&(-1i16).to_be_bytes());
        glyph[10..12].copy_from_slice(&0x0003u16.to_be_bytes());
        glyph[12..14].copy_from_slice(&component.to_be_bytes());
        glyph[14..16].copy_from_slice(&dx.to_be_bytes());
        glyph[16..18].copy_from_slice(&dy.to_be_bytes());
        glyph
    }

    fn head_table(index_to_loc_format: i16) -> Vec<u8> {
        let mut head = vec![0u8; 54];
        head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
        head[18..20].copy_from_slice(&1024u16.to_be_bytes());
        head[50..52].copy_from_slice(&index_to_loc_format.to_be_bytes());
        head
    }

    fn maxp_table(glyph_count: u16) -> Vec<u8> {
        let mut maxp = vec![0u8; 32];
        maxp[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&glyph_count.to_be_bytes());
        maxp
    }

    fn half_maxp_table(glyph_count: u16) -> Vec<u8> {
        let mut maxp = vec![0u8; 6];
        maxp[..4].copy_from_slice(&0x0000_5000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&glyph_count.to_be_bytes());
        maxp
    }

    fn hhea_table() -> Vec<u8> {
        let mut hhea = vec![0u8; 36];
        hhea[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        hhea[34..36].copy_from_slice(&2u16.to_be_bytes());
        hhea
    }

    fn hmtx_table() -> Vec<u8> {
        let mut hmtx = vec![0u8; 12];
        hmtx[0..2].copy_from_slice(&500u16.to_be_bytes());
        hmtx[2..4].copy_from_slice(&0i16.to_be_bytes());
        hmtx[4..6].copy_from_slice(&600u16.to_be_bytes());
        hmtx[6..8].copy_from_slice(&5i16.to_be_bytes());
        hmtx[8..10].copy_from_slice(&6i16.to_be_bytes());
        hmtx[10..12].copy_from_slice(&7i16.to_be_bytes());
        hmtx
    }

    fn cmap_table() -> Vec<u8> {
        cmap_table_for(3, 1, &[(0x41, 2), (0x42, 3)])
    }

    fn cmap_table_for(platform: u16, encoding: u16, mappings: &[(u16, u16)]) -> Vec<u8> {
        let subtable = format4_subtable(mappings);
        let mut cmap = vec![0u8; 12 + subtable.len()];
        cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
        cmap[4..6].copy_from_slice(&platform.to_be_bytes());
        cmap[6..8].copy_from_slice(&encoding.to_be_bytes());
        cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
        cmap[12..].copy_from_slice(&subtable);
        cmap
    }

    fn format4_subtable(mappings: &[(u16, u16)]) -> Vec<u8> {
        let segment_count = mappings.len() + 1;
        let length = 16 + segment_count * 8;
        let mut subtable = vec![0u8; length];
        subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
        subtable[2..4].copy_from_slice(&(length as u16).to_be_bytes());
        subtable[6..8].copy_from_slice(&((segment_count * 2) as u16).to_be_bytes());

        let mut power = 1usize;
        let mut entry_selector = 0u16;
        while power * 2 <= segment_count {
            power *= 2;
            entry_selector += 1;
        }
        subtable[8..10].copy_from_slice(&((power * 2) as u16).to_be_bytes());
        subtable[10..12].copy_from_slice(&entry_selector.to_be_bytes());
        subtable[12..14].copy_from_slice(&((segment_count * 2 - power * 2) as u16).to_be_bytes());

        let end_codes = 14;
        let start_codes = 16 + segment_count * 2;
        let deltas = 16 + segment_count * 4;
        for (index, (code, glyph_id)) in mappings.iter().enumerate() {
            subtable[end_codes + index * 2..end_codes + index * 2 + 2]
                .copy_from_slice(&code.to_be_bytes());
            subtable[start_codes + index * 2..start_codes + index * 2 + 2]
                .copy_from_slice(&code.to_be_bytes());
            subtable[deltas + index * 2..deltas + index * 2 + 2]
                .copy_from_slice(&glyph_id.wrapping_sub(*code).to_be_bytes());
        }
        let sentinel = mappings.len();
        subtable[end_codes + sentinel * 2..end_codes + sentinel * 2 + 2]
            .copy_from_slice(&u16::MAX.to_be_bytes());
        subtable[start_codes + sentinel * 2..start_codes + sentinel * 2 + 2]
            .copy_from_slice(&u16::MAX.to_be_bytes());
        subtable[deltas + sentinel * 2..deltas + sentinel * 2 + 2]
            .copy_from_slice(&1u16.to_be_bytes());
        subtable
    }

    fn build_sfnt(mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
        tables.sort_by_key(|(tag, _)| *tag);
        let directory_length = 12 + tables.len() * 16;
        let mut bytes = vec![0u8; directory_length];
        bytes[..4].copy_from_slice(&[0, 1, 0, 0]);
        bytes[4..6].copy_from_slice(&(tables.len() as u16).to_be_bytes());

        let mut cursor = directory_length;
        for (index, (tag, table)) in tables.iter().enumerate() {
            let offset = align4(cursor);
            let record = 12 + index * 16;
            bytes[record..record + 4].copy_from_slice(tag);
            bytes[record + 8..record + 12].copy_from_slice(&(offset as u32).to_be_bytes());
            bytes[record + 12..record + 16].copy_from_slice(&(table.len() as u32).to_be_bytes());
            bytes.resize(offset, 0);
            bytes.extend_from_slice(table);
            cursor = bytes.len();
        }
        bytes
    }

    fn table<'a>(sfnt: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
        let table_count = usize::from(be_u16(sfnt, 4));
        for index in 0..table_count {
            let record = 12 + index * 16;
            if sfnt[record..record + 4] == tag[..] {
                let offset = be_u32(sfnt, record + 8) as usize;
                let length = be_u32(sfnt, record + 12) as usize;
                return &sfnt[offset..offset + length];
            }
        }
        panic!("missing table {}", String::from_utf8_lossy(tag));
    }

    fn checksum(data: &[u8]) -> u32 {
        let mut sum = 0u32;
        for chunk in data.chunks(4) {
            let mut padded = [0u8; 4];
            padded[..chunk.len()].copy_from_slice(chunk);
            sum = sum.wrapping_add(u32::from_be_bytes(padded));
        }
        sum
    }

    fn align4(value: usize) -> usize {
        (value + 3) & !3
    }

    fn be_u16(data: &[u8], offset: usize) -> u16 {
        u16::from_be_bytes(data[offset..offset + 2].try_into().unwrap())
    }

    fn be_u32(data: &[u8], offset: usize) -> u32 {
        u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
    }
}
