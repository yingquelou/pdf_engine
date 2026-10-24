use super::*;

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
fn preserves_supplementary_cmap_in_format12() {
    let bytes = fixture_font_with_cmap(format12_cmap_table(&[
        (0x41, 0x41, 2),
        (0x42, 0x42, 3),
        (0x1F600, 0x1F600, 3),
    ]));
    let subset_bytes = subset_truetype(&bytes, &[3]).unwrap();
    let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();

    assert_eq!(subset.glyph_count(), 2);
    assert_eq!(subset.glyph_id_for_unicode(0x1F600).unwrap(), Some(1));
    assert_eq!(subset.glyph_id_for_unicode(0x42).unwrap(), Some(1));
    assert_eq!(subset.glyph_id_for_unicode(0x41).unwrap(), None);

    let cmap = table(&subset_bytes, b"cmap");
    assert_eq!(be_u16(cmap, 2), 2, "two encoding records");
    let formats: Vec<u16> = (0..2)
        .map(|index| {
            let offset = be_u32(cmap, 4 + index * 8 + 4) as usize;
            be_u16(cmap, offset)
        })
        .collect();
    assert_eq!(formats, [4, 12]);
    assert_eq!(be_u16(cmap, 4), 3, "format 4 platform");
    assert_eq!(be_u16(cmap, 6), 1, "format 4 encoding stays Unicode BMP");
    assert_eq!(be_u16(cmap, 12), 3, "format 12 platform");
    assert_eq!(be_u16(cmap, 14), 10, "format 12 full-repertoire encoding");

    // The preferred format 12 subtable repeats the BMP mapping next to the
    // supplementary group.
    let format12_offset = be_u32(cmap, 16) as usize;
    assert_eq!(be_u16(cmap, format12_offset), 12);
    assert_eq!(be_u32(cmap, format12_offset + 12), 2, "group count");
    assert_eq!(be_u32(cmap, format12_offset + 16), 0x42);
    assert_eq!(be_u32(cmap, format12_offset + 20), 0x42);
    assert_eq!(be_u32(cmap, format12_offset + 24), 1);
    assert_eq!(be_u32(cmap, format12_offset + 28), 0x1F600);
    assert_eq!(be_u32(cmap, format12_offset + 32), 0x1F600);
    assert_eq!(be_u32(cmap, format12_offset + 36), 1);
}

#[test]
fn omits_supplementary_mappings_for_dropped_glyphs() {
    let bytes = fixture_font_with_cmap(format12_cmap_table(&[
        (0x42, 0x42, 3),
        (0x1F600, 0x1F600, 3),
        (0x1F602, 0x1F602, 2),
    ]));

    // U+1F602 maps to glyph 2, which the subset drops.
    let subset_bytes = subset_truetype(&bytes, &[3]).unwrap();
    let subset = TrueTypeFont::parse(subset_bytes).unwrap();
    assert_eq!(subset.glyph_id_for_unicode(0x1F600).unwrap(), Some(1));
    assert_eq!(subset.glyph_id_for_unicode(0x1F602).unwrap(), None);

    // Glyph 1 has no cmap entries, so no format 12 subtable is needed and
    // the format 4 table stays the first and only subtable.
    let subset_bytes = subset_truetype(&bytes, &[1]).unwrap();
    let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();
    assert_eq!(subset.glyph_id_for_unicode(0x1F600).unwrap(), None);
    assert_eq!(subset.glyph_id_for_unicode(0x42).unwrap(), None);
    let cmap = table(&subset_bytes, b"cmap");
    assert_eq!(be_u16(cmap, 2), 1);
    assert_eq!(be_u16(cmap, 12), 4);
}
