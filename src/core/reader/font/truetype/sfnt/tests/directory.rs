//! SFNT 头与表目录的解析、访问器与畸形目录拒绝。

use super::super::super::cmap::CmapFormat;
use super::super::{SfntFlavor, TrueTypeFont};
use super::support::*;

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

    let unsupported = font_with_tables(*b"wOFF", &[(*b"head", 28, 4)]);
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
