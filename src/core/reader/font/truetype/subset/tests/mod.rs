use super::super::TrueTypeFont;
use super::subset_truetype;
use crate::core::reader::font::FontError;

mod cmap;
mod subset;
mod vertical;

const GLYPH_COUNT: u16 = 4;

fn fixture_font(index_to_loc_format: i16, include_post: bool) -> Vec<u8> {
    fixture_font_with_maxp(index_to_loc_format, include_post, maxp_table(GLYPH_COUNT))
}

fn fixture_font_with_maxp(index_to_loc_format: i16, include_post: bool, maxp: Vec<u8>) -> Vec<u8> {
    build_sfnt(fixture_tables(index_to_loc_format, include_post, maxp))
}

fn fixture_font_with_cmap(cmap: Vec<u8>) -> Vec<u8> {
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    if let Some(entry) = tables.iter_mut().find(|(tag, _)| *tag == *b"cmap") {
        entry.1 = cmap;
    }
    build_sfnt(tables)
}

fn fixture_font_with_vertical_metrics(include_vhea: bool, include_vmtx: bool) -> Vec<u8> {
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    if include_vhea {
        tables.push((*b"vhea", vhea_table()));
    }
    if include_vmtx {
        tables.push((*b"vmtx", vmtx_table()));
    }
    build_sfnt(tables)
}

/// Four glyphs with distinct vertical bounds plus stale `vhea` summaries.
///
/// Retained glyphs: 0 (height 0), 1 (height 200), and the composite glyph
/// 3 (height 50). Glyph 2 is meant to be dropped and carries the most
/// extreme bearing and bounds in the font.
fn vertical_summary_fixture() -> Vec<u8> {
    vertical_summary_fixture_with_first_glyph(simple_glyph_bounds(0, 0))
}

fn vertical_summary_fixture_with_first_glyph(first_glyph: Vec<u8>) -> Vec<u8> {
    let glyphs = [
        first_glyph,
        simple_glyph_bounds(-20, 180),
        simple_glyph_bounds(-1000, 2000),
        composite_glyph_bounds(1, 10, 60),
    ];
    let (glyf, loca) = glyf_and_loca_for(&glyphs, 1);
    let mut tables = fixture_tables(1, false, maxp_table(GLYPH_COUNT));
    for (tag, data) in tables.iter_mut() {
        if *tag == *b"glyf" {
            *data = glyf.clone();
        } else if *tag == *b"loca" {
            *data = loca.clone();
        }
    }

    let mut vhea = vhea_table();
    vhea[4..6].copy_from_slice(&880i16.to_be_bytes());
    vhea[6..8].copy_from_slice(&(-120i16).to_be_bytes());
    vhea[8..10].copy_from_slice(&90i16.to_be_bytes());
    // Stale source summaries; the subset must replace all four.
    vhea[10..12].copy_from_slice(&60000u16.to_be_bytes());
    vhea[12..14].copy_from_slice(&30000i16.to_be_bytes());
    vhea[14..16].copy_from_slice(&(-30000i16).to_be_bytes());
    vhea[16..18].copy_from_slice(&(-20000i16).to_be_bytes());
    vhea[18..20].copy_from_slice(&1i16.to_be_bytes());
    vhea[22..24].copy_from_slice(&5i16.to_be_bytes());
    tables.push((*b"vhea", vhea));

    // Two long metrics plus two trailing top side bearings.
    let mut vmtx = vec![0u8; 12];
    vmtx[0..2].copy_from_slice(&1000u16.to_be_bytes());
    vmtx[2..4].copy_from_slice(&(-500i16).to_be_bytes());
    vmtx[4..6].copy_from_slice(&1200u16.to_be_bytes());
    vmtx[6..8].copy_from_slice(&(-40i16).to_be_bytes());
    vmtx[8..10].copy_from_slice(&(-300i16).to_be_bytes());
    vmtx[10..12].copy_from_slice(&(-10i16).to_be_bytes());
    tables.push((*b"vmtx", vmtx));

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
    glyf_and_loca_for(&glyphs, index_to_loc_format)
}

fn glyf_and_loca_for(glyphs: &[Vec<u8>], index_to_loc_format: i16) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Vec::new();
    let mut offsets = Vec::new();
    for glyph in glyphs {
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

/// Simple glyph whose record header stores the given vertical bounds.
fn simple_glyph_bounds(y_min: i16, y_max: i16) -> Vec<u8> {
    let mut glyph = simple_glyph(0, 0);
    glyph[4..6].copy_from_slice(&y_min.to_be_bytes());
    glyph[8..10].copy_from_slice(&y_max.to_be_bytes());
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

/// Composite glyph whose record header stores the given vertical bounds.
fn composite_glyph_bounds(component: u16, y_min: i16, y_max: i16) -> Vec<u8> {
    let mut glyph = composite_glyph(component, 0, 0);
    glyph[4..6].copy_from_slice(&y_min.to_be_bytes());
    glyph[8..10].copy_from_slice(&y_max.to_be_bytes());
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

/// `vhea` with two long vertical metrics and version 1.0.
fn vhea_table() -> Vec<u8> {
    let mut vhea = vec![0u8; 36];
    vhea[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    vhea[34..36].copy_from_slice(&2u16.to_be_bytes());
    vhea
}

/// Two long metrics then two trailing TSBs for the four fixture glyphs.
fn vmtx_table() -> Vec<u8> {
    let mut vmtx = vec![0u8; 12];
    vmtx[0..2].copy_from_slice(&800u16.to_be_bytes());
    vmtx[2..4].copy_from_slice(&50i16.to_be_bytes());
    vmtx[4..6].copy_from_slice(&900u16.to_be_bytes());
    vmtx[6..8].copy_from_slice(&60i16.to_be_bytes());
    vmtx[8..10].copy_from_slice(&70i16.to_be_bytes());
    vmtx[10..12].copy_from_slice(&80i16.to_be_bytes());
    vmtx
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

/// Format 12 cmap using the Windows Unicode full-repertoire encoding.
fn format12_cmap_table(groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let subtable = format12_subtable(groups);
    let mut cmap = vec![0u8; 12 + subtable.len()];
    cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
    cmap[4..6].copy_from_slice(&3u16.to_be_bytes());
    cmap[6..8].copy_from_slice(&10u16.to_be_bytes());
    cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
    cmap[12..].copy_from_slice(&subtable);
    cmap
}

fn format12_subtable(groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let length = 16 + groups.len() * 12;
    let mut subtable = vec![0u8; length];
    subtable[0..2].copy_from_slice(&12u16.to_be_bytes());
    subtable[4..8].copy_from_slice(&(length as u32).to_be_bytes());
    subtable[12..16].copy_from_slice(&(groups.len() as u32).to_be_bytes());
    for (index, (start, end, glyph)) in groups.iter().enumerate() {
        let group = 16 + index * 12;
        subtable[group..group + 4].copy_from_slice(&start.to_be_bytes());
        subtable[group + 4..group + 8].copy_from_slice(&end.to_be_bytes());
        subtable[group + 8..group + 12].copy_from_slice(&glyph.to_be_bytes());
    }
    subtable
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
    subtable[deltas + sentinel * 2..deltas + sentinel * 2 + 2].copy_from_slice(&1u16.to_be_bytes());
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

fn be_i16(data: &[u8], offset: usize) -> i16 {
    i16::from_be_bytes(data[offset..offset + 2].try_into().unwrap())
}

fn be_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap())
}
