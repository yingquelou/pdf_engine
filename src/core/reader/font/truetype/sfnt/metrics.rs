//! 已由 `TrueTypeMetadata` 与 `hmtx`/`vmtx` 表派生的量度查询。

use super::TrueTypeFont;

impl TrueTypeFont {
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
}
