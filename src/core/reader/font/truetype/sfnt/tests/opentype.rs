//! CFF 风味（OTTO）程序的接受条件与缺表拒绝。

use super::super::super::cmap::CmapFormat;
use super::super::{SfntFlavor, TrueTypeFont};
use super::support::*;

#[test]
fn parses_cff_flavored_opentype_without_glyph_tables() {
    let font = TrueTypeFont::parse(cff_flavored_font()).unwrap();

    assert_eq!(font.flavor(), SfntFlavor::Cff);
    assert_eq!(font.units_per_em(), 2048);
    assert_eq!(font.glyph_count(), 3);
    assert_eq!(font.horizontal_advance_width(0), Some(500));
    assert_eq!(font.horizontal_advance_width(1), Some(600));
    assert_eq!(font.horizontal_advance_width(2), Some(600));
    assert_eq!(font.horizontal_advance_width(3), None);
    assert_eq!(font.cmap_format(), CmapFormat::Format4);
    assert_eq!(font.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    assert_eq!(font.glyph_id_for_unicode(0x42).unwrap(), None);
    assert_eq!(font.unicode_for_glyph(2).unwrap(), Some(0x41));

    // Without `loca`/`glyf` there are no TrueType outlines to return.
    assert_eq!(font.glyph(0).unwrap(), None);
    assert_eq!(font.glyph(2).unwrap(), None);
    assert_eq!(font.table(b"loca"), None);
    assert_eq!(font.table(b"glyf"), None);
    assert_eq!(font.table(b"cmap").unwrap().len(), 44);
    assert_eq!(font.table_tags().count(), 5);

    let encoding = font.builtin_encoding().unwrap();
    assert_eq!(encoding.len(), 256);
    assert_eq!(encoding[0x41], Some('A'));
    assert_eq!(encoding[0x42], None);
}

#[test]
fn opentype_without_glyph_tables_exposes_vertical_metrics() {
    let mut records = opentype_records();
    records.push((*b"vhea", 304, 36));
    records.push((*b"vmtx", 340, 10));
    let mut bytes = font_with_tables(*b"OTTO", &records);
    write_three_glyph_metadata(&mut bytes);
    bytes[260..304].copy_from_slice(&simple_format4_cmap());
    // vhea: version 1.0 with numberOfVMetrics = 2 at offset 34.
    bytes[304..308].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[338..340].copy_from_slice(&2u16.to_be_bytes());
    // vmtx: advances 800 and 900, bearings 50, 60, then a trailing 70.
    bytes[340..342].copy_from_slice(&800u16.to_be_bytes());
    bytes[342..344].copy_from_slice(&50i16.to_be_bytes());
    bytes[344..346].copy_from_slice(&900u16.to_be_bytes());
    bytes[346..348].copy_from_slice(&60i16.to_be_bytes());
    bytes[348..350].copy_from_slice(&70i16.to_be_bytes());

    let font = TrueTypeFont::parse(bytes).unwrap();
    assert_eq!(font.flavor(), SfntFlavor::Cff);
    assert_eq!(font.vertical_advance(0), Some(800));
    assert_eq!(font.vertical_advance(2), Some(900));
    assert_eq!(font.top_side_bearing(0), Some(50));
    assert_eq!(font.top_side_bearing(2), Some(70));
    assert_eq!(font.glyph(0).unwrap(), None);
    assert_eq!(font.table(b"vmtx").unwrap().len(), 10);
}

#[test]
fn rejects_opentype_without_cmap() {
    let mut bytes = font_with_tables(
        *b"OTTO",
        &[
            (*b"head", 124, 54),
            (*b"maxp", 180, 32),
            (*b"hhea", 212, 36),
            (*b"hmtx", 248, 10),
        ],
    );
    write_three_glyph_metadata(&mut bytes);
    assert!(TrueTypeFont::parse(bytes).is_err());
}

#[test]
fn rejects_opentype_with_only_one_glyph_table() {
    let mut loca_only = font_with_tables(
        *b"OTTO",
        &[
            (*b"head", 124, 54),
            (*b"maxp", 180, 32),
            (*b"hhea", 212, 36),
            (*b"hmtx", 248, 10),
            (*b"cmap", 260, 44),
            (*b"loca", 304, 8),
        ],
    );
    write_three_glyph_metadata(&mut loca_only);
    loca_only[260..304].copy_from_slice(&simple_format4_cmap());
    assert!(TrueTypeFont::parse(loca_only).is_err());

    let mut glyf_only = font_with_tables(
        *b"OTTO",
        &[
            (*b"head", 124, 54),
            (*b"maxp", 180, 32),
            (*b"hhea", 212, 36),
            (*b"hmtx", 248, 10),
            (*b"cmap", 260, 44),
            (*b"glyf", 304, 8),
        ],
    );
    write_three_glyph_metadata(&mut glyf_only);
    glyf_only[260..304].copy_from_slice(&simple_format4_cmap());
    assert!(TrueTypeFont::parse(glyf_only).is_err());
}

#[test]
fn rejects_truetype_flavor_without_glyph_tables() {
    let mut no_glyph_tables = font_with_tables([0, 1, 0, 0], &opentype_records());
    write_three_glyph_metadata(&mut no_glyph_tables);
    no_glyph_tables[260..304].copy_from_slice(&simple_format4_cmap());
    assert!(TrueTypeFont::parse(no_glyph_tables).is_err());

    let mut records = opentype_records();
    records.push((*b"loca", 304, 8));
    let mut loca_only = font_with_tables([0, 1, 0, 0], &records);
    write_three_glyph_metadata(&mut loca_only);
    loca_only[260..304].copy_from_slice(&simple_format4_cmap());
    assert!(TrueTypeFont::parse(loca_only).is_err());
}
