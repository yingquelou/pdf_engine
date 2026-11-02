//! `vhea`/`vmtx` 垂直量度：解析、末度量复用与缺表/截断行为。

use super::super::TrueTypeFont;
use super::support::*;

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
