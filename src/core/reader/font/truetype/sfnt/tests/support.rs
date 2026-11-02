//! `sfnt` 测试共享夹具：format 4 `cmap` 构造与最小字体程序的字节表装配。

pub(super) fn simple_format4_cmap() -> Vec<u8> {
    let mut cmap = vec![0; 44];
    cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
    cmap[4..6].copy_from_slice(&3u16.to_be_bytes());
    cmap[6..8].copy_from_slice(&1u16.to_be_bytes());
    cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
    let subtable = &mut cmap[12..];
    subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
    subtable[2..4].copy_from_slice(&32u16.to_be_bytes());
    subtable[6..8].copy_from_slice(&4u16.to_be_bytes());
    subtable[8..10].copy_from_slice(&4u16.to_be_bytes());
    subtable[10..12].copy_from_slice(&1u16.to_be_bytes());
    subtable[14..16].copy_from_slice(&0x41u16.to_be_bytes());
    subtable[16..18].copy_from_slice(&u16::MAX.to_be_bytes());
    subtable[20..22].copy_from_slice(&0x41u16.to_be_bytes());
    subtable[22..24].copy_from_slice(&u16::MAX.to_be_bytes());
    subtable[24..26].copy_from_slice(&(-63i16).to_be_bytes());
    subtable[26..28].copy_from_slice(&1i16.to_be_bytes());
    cmap
}

/// A Microsoft Symbol (3, 0) format 4 cmap that maps `0xF041` to glyph 2.
pub(super) fn symbolic_format4_cmap() -> Vec<u8> {
    format4_cmap(3, 0, &[(0xF041, 2)])
}

/// Build a format 4 `cmap` recorded under `platform`/`encoding` with one
/// segment per `(code, glyph)` pair, plus the required `0xFFFF` sentinel.
pub(super) fn format4_cmap(platform: u16, encoding: u16, mappings: &[(u16, u16)]) -> Vec<u8> {
    let segment_count = mappings.len() + 1;
    let mut end_codes = Vec::with_capacity(segment_count);
    let mut start_codes = Vec::with_capacity(segment_count);
    let mut deltas = Vec::with_capacity(segment_count);
    for (code, glyph) in mappings {
        end_codes.push(*code);
        start_codes.push(*code);
        deltas.push(glyph.wrapping_sub(*code));
    }
    end_codes.push(u16::MAX);
    start_codes.push(u16::MAX);
    // The sentinel maps 0xFFFF to glyph 0 through the idDelta wrap.
    deltas.push(1);

    let mut subtable = vec![0; 16 + segment_count * 8];
    let subtable_length = subtable.len() as u16;
    let write = |data: &mut [u8], offset: usize, values: &[u16]| {
        for (index, value) in values.iter().enumerate() {
            data[offset + index * 2..offset + index * 2 + 2].copy_from_slice(&value.to_be_bytes());
        }
    };
    let end_codes_offset = 14;
    let start_codes_offset = end_codes_offset + segment_count * 2 + 2;
    let deltas_offset = start_codes_offset + segment_count * 2;
    subtable[0..2].copy_from_slice(&4u16.to_be_bytes());
    subtable[2..4].copy_from_slice(&subtable_length.to_be_bytes());
    subtable[6..8].copy_from_slice(&((segment_count * 2) as u16).to_be_bytes());
    write(&mut subtable, end_codes_offset, &end_codes);
    write(&mut subtable, start_codes_offset, &start_codes);
    write(&mut subtable, deltas_offset, &deltas);

    let mut cmap = vec![0; 12 + subtable.len()];
    cmap[2..4].copy_from_slice(&1u16.to_be_bytes());
    cmap[4..6].copy_from_slice(&platform.to_be_bytes());
    cmap[6..8].copy_from_slice(&encoding.to_be_bytes());
    cmap[8..12].copy_from_slice(&12u32.to_be_bytes());
    cmap[12..].copy_from_slice(&subtable);
    cmap
}

/// Build a three-glyph font whose `cmap` table is `cmap`.
///
/// The `loca` and `glyf` tables follow the cmap table, so fixtures may use
/// cmaps larger than the 44-byte single-segment layout.
pub(super) fn font_with_cmap(cmap: Vec<u8>) -> Vec<u8> {
    let loca_offset = 260 + cmap.len().div_ceil(4) * 4;
    let glyf_offset = loca_offset + 8;
    let mut bytes = font_with_tables(
        [0, 1, 0, 0],
        &[
            (*b"head", 124, 54),
            (*b"maxp", 180, 32),
            (*b"hhea", 212, 36),
            (*b"hmtx", 248, 10),
            (*b"cmap", 260, cmap.len() as u32),
            (*b"loca", loca_offset as u32, 8),
            (*b"glyf", glyf_offset as u32, 34),
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
    bytes[260..260 + cmap.len()].copy_from_slice(&cmap);
    bytes[loca_offset..loca_offset + 2].copy_from_slice(&0u16.to_be_bytes());
    bytes[loca_offset + 2..loca_offset + 4].copy_from_slice(&9u16.to_be_bytes());
    bytes[loca_offset + 4..loca_offset + 6].copy_from_slice(&9u16.to_be_bytes());
    bytes[loca_offset + 6..loca_offset + 8].copy_from_slice(&17u16.to_be_bytes());
    bytes[glyf_offset..glyf_offset + 2].copy_from_slice(&1i16.to_be_bytes());
    bytes[glyf_offset + 2..glyf_offset + 10].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
    bytes[glyf_offset + 10..glyf_offset + 12].copy_from_slice(&0u16.to_be_bytes());
    bytes[glyf_offset + 12..glyf_offset + 14].copy_from_slice(&0u16.to_be_bytes());
    bytes[glyf_offset + 14] = 0x37;
    bytes[glyf_offset + 15] = 1;
    bytes[glyf_offset + 16] = 2;
    bytes[glyf_offset + 18..glyf_offset + 20].copy_from_slice(&(-1i16).to_be_bytes());
    bytes[glyf_offset + 20..glyf_offset + 28].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
    bytes[glyf_offset + 28..glyf_offset + 30].copy_from_slice(&0x0002u16.to_be_bytes());
    bytes[glyf_offset + 30..glyf_offset + 32].copy_from_slice(&0u16.to_be_bytes());
    bytes
}

pub(super) fn font_with_tables(signature: [u8; 4], records: &[([u8; 4], u32, u32)]) -> Vec<u8> {
    let directory_end = 12 + records.len() * 16;
    let data_length = records
        .iter()
        .map(|(_, offset, length)| (*offset as usize) + (*length as usize))
        .max()
        .unwrap_or(directory_end)
        .max(directory_end);
    let mut bytes = vec![0; data_length];
    bytes[..4].copy_from_slice(&signature);
    bytes[4..6].copy_from_slice(&(records.len() as u16).to_be_bytes());
    for (index, (tag, offset, length)) in records.iter().enumerate() {
        let start = 12 + index * 16;
        bytes[start..start + 4].copy_from_slice(tag);
        bytes[start + 8..start + 12].copy_from_slice(&offset.to_be_bytes());
        bytes[start + 12..start + 16].copy_from_slice(&length.to_be_bytes());
    }
    bytes
}

/// Write the `head`/`maxp`/`hhea`/`hmtx` payload used by the fixtures whose
/// tables start at offsets 124, 180, 212, and 248: three glyphs with the
/// horizontal advances 500 and 600.
pub(super) fn write_three_glyph_metadata(bytes: &mut [u8]) {
    bytes[124 + 12..124 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
    bytes[124 + 18..124 + 20].copy_from_slice(&2048u16.to_be_bytes());
    bytes[124 + 50..124 + 52].copy_from_slice(&0i16.to_be_bytes());
    bytes[180..184].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[184..186].copy_from_slice(&3u16.to_be_bytes());
    bytes[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[246..248].copy_from_slice(&2u16.to_be_bytes());
    bytes[248..250].copy_from_slice(&500u16.to_be_bytes());
    bytes[252..254].copy_from_slice(&600u16.to_be_bytes());
}

/// The five-table record list shared by the OpenType fixtures below.
pub(super) fn opentype_records() -> Vec<([u8; 4], u32, u32)> {
    vec![
        (*b"head", 124, 54),
        (*b"maxp", 180, 32),
        (*b"hhea", 212, 36),
        (*b"hmtx", 248, 10),
        (*b"cmap", 260, 44),
    ]
}

/// Build a minimal CFF-flavored (`OTTO`) program with no `loca`/`glyf`.
pub(super) fn cff_flavored_font() -> Vec<u8> {
    let mut bytes = font_with_tables(*b"OTTO", &opentype_records());
    write_three_glyph_metadata(&mut bytes);
    bytes[260..304].copy_from_slice(&simple_format4_cmap());
    bytes
}

pub(super) fn font_with_vertical_metrics() -> Vec<u8> {
    let mut bytes = font_with_tables(
        [0, 1, 0, 0],
        &[
            (*b"head", 156, 54),
            (*b"maxp", 212, 32),
            (*b"hhea", 244, 36),
            (*b"hmtx", 280, 10),
            (*b"cmap", 292, 44),
            (*b"loca", 336, 8),
            (*b"glyf", 344, 34),
            (*b"vhea", 380, 36),
            (*b"vmtx", 416, 10),
        ],
    );
    // head
    bytes[156 + 12..156 + 16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
    bytes[156 + 18..156 + 20].copy_from_slice(&2048u16.to_be_bytes());
    bytes[156 + 50..156 + 52].copy_from_slice(&0i16.to_be_bytes());
    // maxp: 3 glyphs
    bytes[212..216].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[216..218].copy_from_slice(&3u16.to_be_bytes());
    // hhea + hmtx (horizontal: 2 long metrics for 3 glyphs)
    bytes[244..248].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[278..280].copy_from_slice(&2u16.to_be_bytes());
    bytes[280..282].copy_from_slice(&500u16.to_be_bytes());
    bytes[284..286].copy_from_slice(&600u16.to_be_bytes());
    // cmap
    let cmap = simple_format4_cmap();
    bytes[292..336].copy_from_slice(&cmap);
    // loca + glyf
    bytes[336..338].copy_from_slice(&0u16.to_be_bytes());
    bytes[338..340].copy_from_slice(&9u16.to_be_bytes());
    bytes[340..342].copy_from_slice(&9u16.to_be_bytes());
    bytes[342..344].copy_from_slice(&17u16.to_be_bytes());
    bytes[344..346].copy_from_slice(&1i16.to_be_bytes());
    bytes[346..354].copy_from_slice(&[0, 1, 0, 2, 0, 5, 0, 8]);
    bytes[354..356].copy_from_slice(&0u16.to_be_bytes());
    bytes[356..358].copy_from_slice(&0u16.to_be_bytes());
    bytes[358] = 0x37;
    bytes[359] = 1;
    bytes[360] = 2;
    bytes[362..364].copy_from_slice(&(-1i16).to_be_bytes());
    bytes[364..372].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4]);
    bytes[372..374].copy_from_slice(&0x0002u16.to_be_bytes());
    bytes[374..376].copy_from_slice(&0u16.to_be_bytes());
    // vhea: version 1.0, numberOfVMetrics = 2
    bytes[380..384].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes[384..386].copy_from_slice(&800i16.to_be_bytes());
    bytes[386..388].copy_from_slice(&(-200i16).to_be_bytes());
    bytes[388..390].copy_from_slice(&0i16.to_be_bytes());
    bytes[390..392].copy_from_slice(&1000u16.to_be_bytes());
    bytes[414..416].copy_from_slice(&2u16.to_be_bytes());
    // vmtx: 2 long metrics (advance=4 bytes each) + 1 trailing tsb (2 bytes)
    //   glyph 0: advance=800, tsb=50
    //   glyph 1: advance=900, tsb=60
    //   glyph 2: trailing tsb=70
    bytes[416..418].copy_from_slice(&800u16.to_be_bytes());
    bytes[418..420].copy_from_slice(&50i16.to_be_bytes());
    bytes[420..422].copy_from_slice(&900u16.to_be_bytes());
    bytes[422..424].copy_from_slice(&60i16.to_be_bytes());
    bytes[424..426].copy_from_slice(&70i16.to_be_bytes());
    bytes
}
