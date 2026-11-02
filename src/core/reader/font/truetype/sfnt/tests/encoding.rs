//! 反向 cmap、`unicode_for_glyph` 与内置单字节编码（含 Symbol 偏移探测）。

use super::super::super::cmap::CmapFormat;
use super::super::TrueTypeFont;
use super::support::*;

#[test]
fn reverse_cmap_maps_glyphs_to_unicode() {
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

    // The simple_format4_cmap maps 0x41 ('A') to glyph 2
    // So reverse should map glyph 2 back to 0x41
    let reverse = font
        .cmap
        .reverse_cmap(font.table(b"cmap").unwrap(), font.metadata.glyph_count)
        .unwrap();

    assert_eq!(reverse.get(&2), Some(&0x41));
}

#[test]
fn unicode_for_glyph_returns_codepoint() {
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

    // Glyph 0 should return None (.notdef)
    assert_eq!(font.unicode_for_glyph(0).unwrap(), None);

    // Glyph 2 should map back to 'A' (0x41)
    assert_eq!(font.unicode_for_glyph(2).unwrap(), Some(0x41));

    // Unmapped glyph should return None
    assert_eq!(font.unicode_for_glyph(99).unwrap(), None);
}

#[test]
fn builtin_encoding_maps_codes_through_cmap() {
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
    let encoding = font.builtin_encoding().unwrap();

    // The fixture maps code 0x41 ('A') to glyph 2, which reverses to 0x41.
    assert_eq!(encoding.len(), 256);
    assert_eq!(encoding[0x41], Some('A'));
    assert_eq!(encoding[0x42], None);
    assert_eq!(encoding[0x00], None);
}

#[test]
fn builtin_encoding_maps_codes_through_symbolic_cmap() {
    let font = TrueTypeFont::parse(font_with_cmap(symbolic_format4_cmap())).unwrap();
    assert!(font.cmap.is_symbolic());
    assert_eq!(font.cmap_format(), CmapFormat::Format4);

    let encoding = font.builtin_encoding().unwrap();

    // 0xF041 is absent, so code 0x41 falls back to 0xF000 | 0x41 and
    // reverses through glyph 2 back to U+F041.
    assert_eq!(encoding.len(), 256);
    assert_eq!(encoding[0x41], Some('\u{F041}'));
    assert_eq!(encoding[0x42], None);
    assert_eq!(encoding[0x00], None);
}

#[test]
fn builtin_encoding_maps_codes_through_f100_symbolic_cmap() {
    let cmap = format4_cmap(3, 0, &[(0xF141, 2)]);
    let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
    assert!(font.cmap.is_symbolic());

    let encoding = font.builtin_encoding().unwrap();

    // 0x41 and 0xF041 are absent, so code 0x41 falls through to
    // 0xF100 | 0x41 and reverses through glyph 2 back to U+F141.
    assert_eq!(encoding.len(), 256);
    assert_eq!(encoding[0x41], Some('\u{F141}'));
    assert_eq!(encoding[0x42], None);
    assert_eq!(encoding[0x00], None);
}

#[test]
fn builtin_encoding_maps_codes_through_f200_symbolic_cmap() {
    let cmap = format4_cmap(3, 0, &[(0xF241, 2)]);
    let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
    assert!(font.cmap.is_symbolic());

    let encoding = font.builtin_encoding().unwrap();

    // Only the 0xF200 offset is mapped, so code 0x41 resolves through it.
    assert_eq!(encoding.len(), 256);
    assert_eq!(encoding[0x41], Some('\u{F241}'));
    assert_eq!(encoding[0x42], None);
    assert_eq!(encoding[0x00], None);
}

#[test]
fn builtin_encoding_prefers_f000_offset_over_f100() {
    // 0xF041 and 0xF141 map to different glyphs, so the reverse cmap
    // yields different Unicode scalars for the same character code.
    let cmap = format4_cmap(3, 0, &[(0xF041, 1), (0xF141, 2)]);
    let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
    assert!(font.cmap.is_symbolic());
    assert_eq!(font.unicode_for_glyph(1).unwrap(), Some(0xF041));
    assert_eq!(font.unicode_for_glyph(2).unwrap(), Some(0xF141));

    let encoding = font.builtin_encoding().unwrap();

    // Probing order is 0xF000, 0xF100, 0xF200, so 0xF041 wins even though
    // 0xF141 is also mapped for code 0x41.
    assert_eq!(encoding[0x41], Some('\u{F041}'));
}

#[test]
fn builtin_encoding_ignores_offsets_for_unicode_cmap() {
    // A (3, 1) Unicode subtable that happens to map 0xF041 must not be
    // probed through symbolic offsets for code 0x41.
    let cmap = format4_cmap(3, 1, &[(0xF041, 2)]);
    let font = TrueTypeFont::parse(font_with_cmap(cmap)).unwrap();
    assert!(!font.cmap.is_symbolic());

    let encoding = font.builtin_encoding().unwrap();

    assert_eq!(encoding[0x41], None);
    assert_eq!(encoding[0xF0], None);
}
