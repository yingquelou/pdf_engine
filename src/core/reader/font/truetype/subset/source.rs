use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use super::directory::{parse_directory, read_i16, read_u16, read_u32, required_table};
use super::glyph::{composite_component_refs, glyph_contour_count};
use super::metrics::{build_loca, parse_loca, validate_vertical_metrics};
use super::{
    HEAD_MAGIC, MAXP_VERSION_0_5, MAXP_VERSION_1_0, PRESERVED_TABLES, REWRITTEN_TABLES,
    invalid_subset,
};
use crate::core::reader::font::FontError;

use super::super::cmap::Cmap;

/// A validated view of the source font's directory and glyph-indexed tables.
pub(super) struct SourceFont<'a> {
    pub(super) flavor: [u8; 4],
    pub(super) tables: BTreeMap<[u8; 4], &'a [u8]>,
    pub(super) glyph_count: u16,
    pub(super) index_to_loc_format: i16,
    pub(super) glyph_ranges: Vec<Range<usize>>,
    pub(super) glyf: &'a [u8],
    pub(super) hmtx: &'a [u8],
    pub(super) horizontal_metric_count: u16,
    /// `vhea.numberOfVMetrics`, present only when both vertical tables exist.
    pub(super) vertical_metric_count: Option<u16>,
    pub(super) cmap: Cmap,
    pub(super) symbolic_cmap: bool,
}

impl<'a> SourceFont<'a> {
    pub(super) fn parse(data: &'a [u8]) -> Result<Self, FontError> {
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

        // Vertical metrics are optional, but they are only usable as a pair.
        let vertical_metric_count = match (tables.get(b"vhea"), tables.get(b"vmtx")) {
            (Some(vhea), Some(vmtx)) => Some(validate_vertical_metrics(vhea, vmtx, glyph_count)?),
            _ => None,
        };

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
            vertical_metric_count,
            cmap,
            symbolic_cmap,
        })
    }

    pub(super) fn source_table(&self, tag: &[u8; 4]) -> &'a [u8] {
        self.tables
            .get(tag)
            .copied()
            .expect("required table was validated")
    }

    /// Return the retained glyphs and their old- to new-glyph mapping.
    pub(super) fn retained_glyphs(
        &self,
        requested: &[u16],
    ) -> Result<BTreeMap<u16, u16>, FontError> {
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

    pub(super) fn build_tables(
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
        let vertical_metrics = match self.vertical_metric_count {
            Some(_) => Some(self.build_vertical_metrics(&order)?),
            None => None,
        };

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
        if let Some((vhea, vmtx)) = vertical_metrics {
            tables.insert(*b"vhea", vhea);
            tables.insert(*b"vmtx", vmtx);
        }
        Ok(tables)
    }
}

fn is_rewritten(tag: &[u8; 4]) -> bool {
    REWRITTEN_TABLES.contains(&tag)
}
