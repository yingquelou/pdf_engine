use super::*;

#[test]
fn preserves_vertical_metrics_for_subset_glyphs() {
    let bytes = fixture_font_with_vertical_metrics(true, true);
    let original = TrueTypeFont::parse(bytes.clone()).unwrap();
    let subset_bytes = subset_truetype(&bytes, &[1, 3]).unwrap();
    let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();

    // New 0 <- old 0, new 1 <- old 1, new 2 <- old 3. The source has two
    // long metrics, so the last retained glyph reuses the final advance
    // height and reads its top side bearing from the trailing array.
    assert_eq!(subset.glyph_count(), 3);
    assert_eq!(subset.vertical_advance(0), original.vertical_advance(0));
    assert_eq!(subset.top_side_bearing(0), original.top_side_bearing(0));
    assert_eq!(subset.vertical_advance(1), original.vertical_advance(1));
    assert_eq!(subset.top_side_bearing(1), original.top_side_bearing(1));
    assert_eq!(subset.vertical_advance(2), original.vertical_advance(3));
    assert_eq!(subset.top_side_bearing(2), original.top_side_bearing(3));
    assert_eq!(subset.vertical_advance(2), Some(900));
    assert_eq!(subset.top_side_bearing(2), Some(80));

    let vhea = table(&subset_bytes, b"vhea");
    assert_eq!(vhea.len(), 36);
    assert_eq!(
        be_u32(vhea, 0),
        0x0001_0000,
        "other vhea fields are unchanged"
    );
    assert_eq!(
        be_u16(vhea, 34),
        2,
        "numberOfVMetrics is clamped to the retained glyph count"
    );
    // Retained heights are all zero in this fixture; glyph 3 reuses the
    // 900 advance height of the final long metric.
    assert_eq!(be_u16(vhea, 10), 900, "advanceHeightMax");
    assert_eq!(be_i16(vhea, 12), 50, "minTopSideBearing");
    assert_eq!(be_i16(vhea, 14), 750, "minBottomSideBearing");
    assert_eq!(be_i16(vhea, 16), 80, "yMaxExtent");
    assert_eq!(
        table(&subset_bytes, b"vmtx").len(),
        10,
        "two long metrics plus one trailing TSB"
    );

    // A single-glyph subset clamps numberOfVMetrics to one.
    let single_bytes = subset_truetype(&bytes, &[]).unwrap();
    let single = TrueTypeFont::parse(single_bytes.clone()).unwrap();
    assert_eq!(be_u16(table(&single_bytes, b"vhea"), 34), 1);
    assert_eq!(table(&single_bytes, b"vmtx").len(), 4);
    assert_eq!(single.vertical_advance(0), original.vertical_advance(0));
    assert_eq!(single.top_side_bearing(0), original.top_side_bearing(0));
    let single_vhea = table(&single_bytes, b"vhea");
    assert_eq!(be_u16(single_vhea, 10), 800);
    assert_eq!(be_i16(single_vhea, 12), 50);
    assert_eq!(be_i16(single_vhea, 14), 750);
    assert_eq!(be_i16(single_vhea, 16), 50);
}

#[test]
fn recomputes_vertical_summary_fields_from_retained_glyphs() {
    let bytes = vertical_summary_fixture();
    let subset_bytes = subset_truetype(&bytes, &[1, 3]).unwrap();
    let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();

    // Old glyphs 0, 1, and 3 survive as new glyphs 0, 1, and 2. New glyph
    // 2 carries only a top side bearing, so its advance height reuses the
    // final long metric of new glyph 1.
    assert_eq!(subset.glyph_count(), 3);
    assert_eq!(subset.vertical_advance(2), Some(1200));
    assert_eq!(subset.top_side_bearing(2), Some(-10));
    let vmtx = table(&subset_bytes, b"vmtx");
    assert_eq!(vmtx.len(), 10, "two long metrics plus one trailing TSB");
    assert_eq!(
        be_i16(vmtx, 8),
        -10,
        "trailing TSB follows the long metrics"
    );

    // Retained heights are 0 (new 0, bounds 0..0), 200 (new 1, bounds
    // -20..180), and 50 (new 2, composite bounds 10..60). The discarded
    // glyph 2 has extreme bearing and bounds and must not contribute.
    let vhea = table(&subset_bytes, b"vhea");
    assert_eq!(be_u16(vhea, 10), 1200, "advanceHeightMax");
    assert_eq!(be_i16(vhea, 12), -500, "minTopSideBearing keeps its sign");
    assert_eq!(
        be_i16(vhea, 14),
        1040,
        "minBottomSideBearing uses the rebuilt metrics and retained bounds"
    );
    assert_eq!(be_i16(vhea, 16), 160, "yMaxExtent");
    assert_eq!(be_u16(vhea, 34), 2, "numberOfVMetrics");

    // Version, typographic metrics, and caret parameters are untouched.
    assert_eq!(be_u32(vhea, 0), 0x0001_0000);
    assert_eq!(be_i16(vhea, 4), 880);
    assert_eq!(be_i16(vhea, 6), -120);
    assert_eq!(be_i16(vhea, 8), 90);
    assert_eq!(be_i16(vhea, 18), 1);
    assert_eq!(be_i16(vhea, 20), 0);
    assert_eq!(be_i16(vhea, 22), 5);
}

#[test]
fn ignores_removed_advances_when_recomputing_vertical_summaries() {
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    let mut vhea = vhea_table();
    vhea[34..36].copy_from_slice(&3u16.to_be_bytes());
    tables.push((*b"vhea", vhea));
    // Three long metrics; the discarded glyph 2 owns the 60000 advance.
    let mut vmtx = vec![0u8; 14];
    vmtx[0..2].copy_from_slice(&1000u16.to_be_bytes());
    vmtx[2..4].copy_from_slice(&0i16.to_be_bytes());
    vmtx[4..6].copy_from_slice(&1200u16.to_be_bytes());
    vmtx[6..8].copy_from_slice(&10i16.to_be_bytes());
    vmtx[8..10].copy_from_slice(&60000u16.to_be_bytes());
    vmtx[10..12].copy_from_slice(&20i16.to_be_bytes());
    vmtx[12..14].copy_from_slice(&30i16.to_be_bytes());
    tables.push((*b"vmtx", vmtx));

    let subset_bytes = subset_truetype(&build_sfnt(tables), &[1]).unwrap();
    let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();
    assert_eq!(subset.glyph_count(), 2);

    let vhea = table(&subset_bytes, b"vhea");
    assert_eq!(be_u32(vhea, 0), 0x0001_0000);
    assert_eq!(
        be_u16(vhea, 34),
        2,
        "numberOfVMetrics is clamped to the retained glyphs"
    );
    assert_eq!(
        be_u16(vhea, 10),
        1200,
        "the removed glyph's 60000 advance is ignored"
    );
    assert_eq!(be_i16(vhea, 12), 0);
    assert_eq!(be_i16(vhea, 14), 1000);
    assert_eq!(be_i16(vhea, 16), 10);
}

#[test]
fn recomputes_vertical_summaries_for_empty_request() {
    // Glyph 0 has no outline, so it contributes a zero-height box.
    let bytes = vertical_summary_fixture_with_first_glyph(Vec::new());
    let subset_bytes = subset_truetype(&bytes, &[]).unwrap();
    let subset = TrueTypeFont::parse(subset_bytes.clone()).unwrap();
    assert_eq!(subset.glyph_count(), 1);
    assert!(subset.glyph(0).unwrap().is_none());

    let vhea = table(&subset_bytes, b"vhea");
    assert_eq!(be_u16(vhea, 34), 1, "numberOfVMetrics is clamped to one");
    assert_eq!(table(&subset_bytes, b"vmtx").len(), 4);
    assert_eq!(be_u16(vhea, 10), 1000, "advanceHeightMax");
    assert_eq!(be_i16(vhea, 12), -500, "minTopSideBearing");
    assert_eq!(
        be_i16(vhea, 14),
        1500,
        "minBottomSideBearing treats the empty glyph as zero height"
    );
    assert_eq!(be_i16(vhea, 16), -500, "yMaxExtent keeps a negative extent");
}

#[test]
fn drops_incomplete_vertical_metrics() {
    for (include_vhea, include_vmtx) in [(true, false), (false, true)] {
        let bytes = fixture_font_with_vertical_metrics(include_vhea, include_vmtx);
        let subset_bytes = subset_truetype(&bytes, &[3]).unwrap();
        let subset = TrueTypeFont::parse(subset_bytes).unwrap();
        assert!(subset.table(b"vhea").is_none());
        assert!(subset.table(b"vmtx").is_none());
        assert!(
            !subset
                .table_tags()
                .any(|tag| *tag == *b"vhea" || *tag == *b"vmtx")
        );
    }
}

#[test]
fn rejects_malformed_vertical_metrics() {
    // vhea shorter than its fixed 36-byte layout.
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    let mut vhea = vhea_table();
    vhea.truncate(34);
    tables.push((*b"vhea", vhea));
    tables.push((*b"vmtx", vmtx_table()));
    let error = subset_truetype(&build_sfnt(tables), &[3]).unwrap_err();
    assert!(matches!(error, FontError::Invalid(_)));

    // numberOfVMetrics exceeds the glyph count.
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    let mut vhea = vhea_table();
    vhea[34..36].copy_from_slice(&(GLYPH_COUNT + 1).to_be_bytes());
    tables.push((*b"vhea", vhea));
    tables.push((*b"vmtx", vmtx_table()));
    assert!(subset_truetype(&build_sfnt(tables), &[3]).is_err());

    // numberOfVMetrics must not be zero.
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    let mut vhea = vhea_table();
    vhea[34..36].copy_from_slice(&0u16.to_be_bytes());
    tables.push((*b"vhea", vhea));
    tables.push((*b"vmtx", vmtx_table()));
    assert!(subset_truetype(&build_sfnt(tables), &[3]).is_err());

    // vmtx is shorter than the long metrics plus trailing TSBs require.
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    let mut vmtx = vmtx_table();
    vmtx.truncate(10);
    tables.push((*b"vhea", vhea_table()));
    tables.push((*b"vmtx", vmtx));
    assert!(subset_truetype(&build_sfnt(tables), &[3]).is_err());
}
