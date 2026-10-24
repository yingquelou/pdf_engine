//! Test-only minimal CFF1 programs and a matching OpenType wrapper.
//!
//! The fixtures are built byte by byte so that the tests do not depend on any
//! font file. All DICT integer operands use the 5-byte `29` (longint) encoding
//! and all offsets are therefore a fixed width, which lets the builders compute
//! absolute offsets in a single pass and assert that no size changed.

/// Build a CFF INDEX from object payloads.
pub(crate) fn build_index(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    if entries.is_empty() {
        return out;
    }

    let mut offsets = vec![1u32];
    let mut total = 0u32;
    for entry in entries {
        total += entry.len() as u32;
        offsets.push(total + 1);
    }
    let last = *offsets.last().expect("non-empty entries");
    let offset_size: u8 = if last <= 0xFF {
        1
    } else if last <= 0xFFFF {
        2
    } else if last <= 0xFF_FFFF {
        3
    } else {
        4
    };
    out.push(offset_size);
    for offset in &offsets {
        for byte in (0..usize::from(offset_size)).rev() {
            out.push((offset >> (byte * 8)) as u8);
        }
    }
    for entry in entries {
        out.extend_from_slice(entry);
    }
    out
}

/// Encode an integer using the DICT `29` (longint) format.
fn dict_longint(value: i32) -> Vec<u8> {
    let mut out = vec![29];
    out.extend_from_slice(&value.to_be_bytes());
    out
}

/// Encode a real number using the DICT `30` (nibble) format.
fn dict_real(text: &str) -> Vec<u8> {
    let mut nibbles = Vec::new();
    for character in text.chars() {
        nibbles.push(match character {
            '0'..='9' => character as u8 - b'0',
            '.' => 0xA,
            'E' => 0xB,
            '-' => 0xE,
            other => panic!("unsupported real number character {other:?}"),
        });
    }
    nibbles.push(0xF);
    if nibbles.len() % 2 == 1 {
        nibbles.push(0xF);
    }
    let mut out = vec![30];
    for pair in nibbles.chunks(2) {
        out.push((pair[0] << 4) | pair[1]);
    }
    out
}

/// Encode a charstring integer using the 3-byte `28` (shortint) format.
fn charstring_int(value: i16) -> Vec<u8> {
    let mut out = vec![28];
    out.extend_from_slice(&value.to_be_bytes());
    out
}

/// `defaultWidthX 20` / `nominalWidthX 21`.
fn private_dict(default_width: i16, nominal_width: i16) -> Vec<u8> {
    let mut out = charstring_int(default_width);
    out.push(20);
    out.extend(charstring_int(nominal_width));
    out.push(21);
    out
}

/// A Font DICT holding only `Private (size, offset)`.
fn font_dict(private_size: usize, private_offset: usize) -> Vec<u8> {
    let mut out = dict_longint(private_size as i32);
    out.extend(dict_longint(private_offset as i32));
    out.push(18);
    out
}

/// `rmoveto` (with an explicit width operand) followed by `rlineto` + `endchar`.
fn rmoveto_rlineto_charstring(width_operand: i16) -> Vec<u8> {
    let mut out = charstring_int(width_operand);
    out.extend(charstring_int(100)); // dx
    out.extend(charstring_int(0)); // dy
    out.push(21); // rmoveto
    out.extend(charstring_int(200)); // dx
    out.extend(charstring_int(0)); // dy
    out.push(5); // rlineto
    out.push(14); // endchar
    out
}

fn name_keyed_top_dict(
    charset_offset: usize,
    private_size: usize,
    private_offset: usize,
    char_strings_offset: usize,
) -> Vec<u8> {
    let mut out = Vec::new();
    for value in [-10, -20, 500, 700] {
        out.extend(dict_longint(value));
    }
    out.push(5); // FontBBox
    for text in ["0.0005", "0", "0", "0.0005", "0", "0"] {
        out.extend(dict_real(text));
    }
    out.extend([12, 7]); // FontMatrix
    out.extend(dict_longint(charset_offset as i32));
    out.push(15); // charset
    out.extend(dict_longint(private_size as i32));
    out.extend(dict_longint(private_offset as i32));
    out.push(18); // Private
    out.extend(dict_longint(char_strings_offset as i32));
    out.push(17); // CharStrings
    out
}

/// A name-keyed CFF with two glyphs.
///
/// * glyph 0: `endchar` only, so it takes `defaultWidthX` (500);
/// * glyph 1: width operand 550 + `nominalWidthX` 50 = 600, then
///   `rmoveto (100, 0)`, `rlineto (200, 0)`, `endchar`;
/// * `FontMatrix` is `0.0005`, so `unitsPerEm` derives to 2000;
/// * `FontBBox` is `[-10 -20 500 700]`;
/// * the charset is predefined ISOAdobe (offset 0).
pub(crate) fn name_keyed_cff() -> Vec<u8> {
    let header = [1u8, 0, 4, 1];
    let name_index = build_index(&[b"TestName".to_vec()]);
    let string_index = build_index(&[]);
    let global_subrs = build_index(&[]);
    let private = private_dict(500, 50);
    let charstrings = build_index(&[vec![14], rmoveto_rlineto_charstring(550)]);

    let placeholder = name_keyed_top_dict(0, 0, 0, 0);
    let top_index_size = build_index(&[placeholder]).len();
    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + global_subrs.len();

    let private_offset = prefix;
    let char_strings_offset = private_offset + private.len();
    let top_dict = name_keyed_top_dict(0, private.len(), private_offset, char_strings_offset);
    assert_eq!(
        build_index(std::slice::from_ref(&top_dict)).len(),
        top_index_size,
        "top DICT size must not depend on offset values"
    );

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend(name_index);
    out.extend(build_index(&[top_dict]));
    out.extend(string_index);
    out.extend(global_subrs);
    out.extend(private);
    out.extend(charstrings);
    out
}

enum CharsetFixture {
    /// charset format 0 with a real charset table at the given offset.
    Explicit(Vec<u8>),
    /// Predefined ISOAdobe (charset offset 0).
    Predefined,
    /// No charset operator at all.
    Missing,
}

impl CharsetFixture {
    fn is_present(&self) -> bool {
        !matches!(self, Self::Missing)
    }

    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Explicit(bytes) => Some(bytes),
            Self::Predefined | Self::Missing => None,
        }
    }
}

fn cid_top_dict(
    charset_offset: Option<usize>,
    fd_array_offset: usize,
    fd_select_offset: usize,
    char_strings_offset: usize,
) -> Vec<u8> {
    let mut out = Vec::new();
    // ROS: Registry SID 391, Ordering SID 392, Supplement 0.
    for value in [391, 392, 0] {
        out.extend(dict_longint(value));
    }
    out.extend([12, 30]); // ROS
    out.extend(dict_longint(100));
    out.extend([12, 34]); // CIDCount
    for text in ["0.0005", "0", "0", "0.0005", "0", "0"] {
        out.extend(dict_real(text));
    }
    out.extend([12, 7]); // FontMatrix
    if let Some(offset) = charset_offset {
        out.extend(dict_longint(offset as i32));
        out.push(15); // charset
    }
    out.extend(dict_longint(fd_array_offset as i32));
    out.extend([12, 36]); // FDArray
    out.extend(dict_longint(fd_select_offset as i32));
    out.extend([12, 37]); // FDSelect
    out.extend(dict_longint(char_strings_offset as i32));
    out.push(17); // CharStrings
    out
}

fn fd_select_format_0() -> Vec<u8> {
    vec![0, 0, 1]
}

fn fd_select_format_3() -> Vec<u8> {
    // format 3, two ranges [(0, FD 0), (1, FD 1)], sentinel 2.
    vec![3, 0, 2, 0, 0, 0, 0, 1, 1, 0, 2]
}

fn cid_keyed_cff_impl(charset: CharsetFixture, fd_select: Vec<u8>) -> Vec<u8> {
    let header = [1u8, 0, 4, 1];
    let name_index = build_index(&[b"CIDTest".to_vec()]);
    let string_index = build_index(&[]);
    let global_subrs = build_index(&[]);
    let fd_privates = [private_dict(300, 10), private_dict(700, 20)];
    let charstrings = build_index(&[vec![14], rmoveto_rlineto_charstring(480)]);
    let fd_array_placeholder = build_index(&[font_dict(0, 0), font_dict(0, 0)]);

    let placeholder_charset_offset = charset.is_present().then_some(0);
    let placeholder = cid_top_dict(placeholder_charset_offset, 0, 0, 0);
    let top_index_size = build_index(&[placeholder]).len();
    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + global_subrs.len();

    let charset_len = charset.bytes().map_or(0, |bytes| bytes.len());
    let charset_offset = prefix;
    let fd_select_offset = charset_offset + charset_len;
    let fd_array_offset = fd_select_offset + fd_select.len();
    let fd_private_base = fd_array_offset + fd_array_placeholder.len();
    let fd0_offset = fd_private_base;
    let fd1_offset = fd0_offset + fd_privates[0].len();
    let char_strings_offset = fd1_offset + fd_privates[1].len();

    let fd_array = build_index(&[
        font_dict(fd_privates[0].len(), fd0_offset),
        font_dict(fd_privates[1].len(), fd1_offset),
    ]);
    assert_eq!(
        fd_array.len(),
        fd_array_placeholder.len(),
        "FDArray size must not depend on offset values"
    );

    let charset_offset = match &charset {
        CharsetFixture::Explicit(_) => Some(charset_offset),
        CharsetFixture::Predefined => Some(0),
        CharsetFixture::Missing => None,
    };
    let top_dict = cid_top_dict(
        charset_offset,
        fd_array_offset,
        fd_select_offset,
        char_strings_offset,
    );
    assert_eq!(
        build_index(std::slice::from_ref(&top_dict)).len(),
        top_index_size,
        "top DICT size must not depend on offset values"
    );

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend(name_index);
    out.extend(build_index(&[top_dict]));
    out.extend(string_index);
    out.extend(global_subrs);
    if let Some(bytes) = charset.bytes() {
        out.extend_from_slice(bytes);
    }
    out.extend(fd_select);
    out.extend(fd_array);
    out.extend_from_slice(&fd_privates[0]);
    out.extend_from_slice(&fd_privates[1]);
    out.extend(charstrings);
    out
}

/// A CID-keyed CFF with two glyphs.
///
/// * charset format 0 maps GID 1 to CID 7 (CID 0 is `.notdef`);
/// * FDSelect format 0 assigns GID 0 to Font DICT 0 and GID 1 to Font DICT 1;
/// * Font DICT 0 has `defaultWidthX` 300 / `nominalWidthX` 10;
/// * Font DICT 1 has `defaultWidthX` 700 / `nominalWidthX` 20, and GID 1's
///   width operand 480 gives a 500-unit advance;
/// * `FontMatrix[0]` is `0.0005`, so `unitsPerEm` derives to 2000.
pub(crate) fn cid_keyed_cff() -> Vec<u8> {
    // charset format 0: GID 1 -> CID 7.
    cid_keyed_cff_impl(
        CharsetFixture::Explicit(vec![0, 0, 7]),
        fd_select_format_0(),
    )
}

/// The CID-keyed fixture with FDSelect format 3 instead of format 0.
pub(crate) fn cid_keyed_cff_with_fd_select_format_3() -> Vec<u8> {
    cid_keyed_cff_impl(
        CharsetFixture::Explicit(vec![0, 0, 7]),
        fd_select_format_3(),
    )
}

/// The CID-keyed fixture without the mandatory charset table.
pub(crate) fn cid_keyed_cff_without_charset() -> Vec<u8> {
    cid_keyed_cff_impl(CharsetFixture::Missing, fd_select_format_0())
}

/// The CID-keyed fixture using the predefined ISOAdobe charset (offset 0),
/// which the CFF specification forbids for CID fonts.
pub(crate) fn cid_keyed_cff_with_predefined_charset() -> Vec<u8> {
    cid_keyed_cff_impl(CharsetFixture::Predefined, fd_select_format_0())
}

/// Wrap a bare CFF program in a minimal `OTTO` SFNT container.
///
/// The `CFF ` table receives one trailing padding byte because `ttf-parser`'s
/// INDEX reader consumes one byte beyond the last INDEX payload.
pub(crate) fn opentype_with_cff(cff: &[u8]) -> Vec<u8> {
    let mut head = [0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magic
    head[18..20].copy_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
    head[48..50].copy_from_slice(&2i16.to_be_bytes()); // fontDirectionHint

    let mut hhea = [0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // version 1.0
    hhea[18..20].copy_from_slice(&1i16.to_be_bytes()); // caretSlopeRise
    hhea[34..36].copy_from_slice(&2u16.to_be_bytes()); // numberOfHMetrics

    let mut maxp = [0u8; 6];
    maxp[0..4].copy_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5
    maxp[4..6].copy_from_slice(&2u16.to_be_bytes()); // numGlyphs

    let mut hmtx = Vec::new();
    for advance in [500i16, 600] {
        hmtx.extend_from_slice(&(advance as u16).to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());
    }

    let mut cff_table = cff.to_vec();
    cff_table.push(0);

    let tables: [(&[u8; 4], &[u8]); 5] = [
        (b"CFF ", &cff_table),
        (b"head", &head),
        (b"hhea", &hhea),
        (b"hmtx", &hmtx),
        (b"maxp", &maxp),
    ];
    let mut out = vec![0u8; 12 + tables.len() * 16];
    out[0..4].copy_from_slice(b"OTTO");
    out[4..6].copy_from_slice(&(tables.len() as u16).to_be_bytes());
    let mut offset = out.len();
    for (index, (tag, data)) in tables.iter().enumerate() {
        let record = 12 + index * 16;
        out[record..record + 4].copy_from_slice(*tag);
        // ttf-parser does not validate table checksums.
        out[record + 8..record + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        out[record + 12..record + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        out.resize(offset, 0);
        out.extend_from_slice(data);
        offset += data.len();
        while !offset.is_multiple_of(4) {
            out.push(0);
            offset += 1;
        }
    }
    out
}
