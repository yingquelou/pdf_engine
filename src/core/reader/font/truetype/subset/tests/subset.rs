use super::*;

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
fn reports_old_to_new_glyph_mapping() {
    let bytes = fixture_font(1, true);

    let (subset_bytes, mapping) = subset_truetype_with_mapping(&bytes, &[3]).unwrap();
    assert_eq!(TrueTypeFont::parse(subset_bytes).unwrap().glyph_count(), 2);
    assert_eq!(mapping.get(&0), Some(&0));
    assert_eq!(mapping.get(&3), Some(&1));
    assert_eq!(mapping.len(), 2);

    // 复合字形会把组件一并纳入映射，即使调用方未请求该组件。
    let (composite_bytes, composite) = subset_truetype_with_mapping(&bytes, &[2]).unwrap();
    assert_eq!(
        TrueTypeFont::parse(composite_bytes).unwrap().glyph_count(),
        3
    );
    assert_eq!(composite.get(&0), Some(&0));
    assert_eq!(composite.get(&2), Some(&1));
    assert_eq!(composite.get(&3), Some(&2), "组件字形也应出现在映射中");
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
fn supports_maxp_version_half() {
    let bytes = fixture_font_with_maxp(1, false, half_maxp_table(GLYPH_COUNT));
    let subset_bytes = subset_truetype(&bytes, &[3]).unwrap();
    let maxp = table(&subset_bytes, b"maxp");

    // Version 0.5 is the six-byte maxp layout; it stays six bytes.
    assert_eq!(maxp.len(), 6);
    assert_eq!(be_u32(maxp, 0), 0x0000_5000);
    assert_eq!(be_u16(maxp, 4), 2);
}
