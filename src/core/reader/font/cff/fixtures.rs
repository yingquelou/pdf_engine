//! Test-only minimal CFF1 programs and a matching OpenType wrapper.
//!
//! The fixtures are built byte by byte so that the tests do not depend on any
//! font file. All DICT integer operands use the 5-byte `29` (longint) encoding
//! and all offsets are therefore a fixed width, which lets the builders compute
//! absolute offsets in a single pass and assert that no size changed.

// name-keyed 样本拆到独立文件，这里重导出以保持既有 `fixtures::*` 路径。
pub(crate) use super::name_keyed_fixtures::*;

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
pub(super) fn dict_longint(value: i32) -> Vec<u8> {
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
pub(super) fn charstring_int(value: i16) -> Vec<u8> {
    let mut out = vec![28];
    out.extend_from_slice(&value.to_be_bytes());
    out
}

/// `defaultWidthX 20` / `nominalWidthX 21`.
pub(super) fn private_dict(default_width: i16, nominal_width: i16) -> Vec<u8> {
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
pub(super) fn rmoveto_rlineto_charstring(width_operand: i16) -> Vec<u8> {
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
    name_keyed_top_dict_full(
        Some(charset_offset),
        None,
        private_size,
        private_offset,
        char_strings_offset,
    )
}

/// name-keyed Top DICT，可选 `charset` 与 `Encoding` 条目。
///
/// `charset_operand` 为 `None` 时省略 `charset`；`encoding_offset` 为 `None`
/// 时省略 `Encoding`。所有整数操作数都用定长 longint，因此 Top DICT 的
/// 字节数不随偏移取值变化。
pub(super) fn name_keyed_top_dict_full(
    charset_operand: Option<usize>,
    encoding_offset: Option<usize>,
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
    if let Some(charset_offset) = charset_operand {
        out.extend(dict_longint(charset_offset as i32));
        out.push(15); // charset
    }
    if let Some(encoding_offset) = encoding_offset {
        out.extend(dict_longint(encoding_offset as i32));
        out.push(16); // Encoding
    }
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

/// `defaultWidthX`/`nominalWidthX` plus a `Subrs` offset pointing at the Local
/// Subr INDEX that immediately follows the Private DICT.
pub(super) fn private_dict_with_subrs(
    default_width: i16,
    nominal_width: i16,
    subrs: &[u8],
) -> Vec<u8> {
    let mut out = private_dict(default_width, nominal_width);
    // The Subrs offset is relative to the Private DICT start; the Local Subr
    // INDEX follows the Private DICT directly, so the offset is the length of
    // the DICT including this entry (fixed-width longint operand).
    let private_len = out.len() + dict_longint(0).len() + 1;
    out.extend(dict_longint(private_len as i32));
    out.push(19); // Subrs
    out.extend_from_slice(subrs);
    out
}

/// A CID-keyed CFF with five glyphs, three Font DICTs and subroutines.
///
/// * charset format 0 maps GID 1..=4 to CIDs 7, 9, 20 and 42;
/// * FDSelect format 3 assigns GID 0 to FD 0, GID 1..=2 to FD 1 and
///   GID 3..=4 to FD 2;
/// * FD 2 carries a Local Subr INDEX (`rlineto (50, 50); return`) which
///   GID 4 calls via `callsubr`;
/// * the Global Subr INDEX holds one subroutine (`rlineto (200, 0); return`)
///   which GID 2 calls via `callgsubr` (operand -107 = index 0, bias 107);
/// * GID 1 takes a 480 width operand (700/20 defaults in FD 1 → 500),
///   GID 3 takes FD 2's default width 600, GID 4 takes 300 + 30 = 330.
///
/// Subsetting to `{7}` retains FD 0/1 and drops FD 2, exercising FDArray
/// pruning and FDSelect renumbering.
pub(crate) fn cid_keyed_cff_multi() -> Vec<u8> {
    let header = [1u8, 0, 4, 1];
    let name_index = build_index(&[b"CIDMulti".to_vec()]);
    let string_index = build_index(&[]);
    let global_subr = {
        let mut subr = charstring_int(200);
        subr.extend(charstring_int(0));
        subr.push(5); // rlineto
        subr.push(11); // return
        subr
    };
    let global_subrs = build_index(&[global_subr]);
    let local_subr = {
        let mut subr = charstring_int(50);
        subr.extend(charstring_int(50));
        subr.push(5); // rlineto
        subr.push(11); // return
        subr
    };
    let local_subrs = build_index(&[local_subr]);
    let fd_privates = [
        private_dict(300, 10),
        private_dict(700, 20),
        private_dict_with_subrs(600, 30, &local_subrs),
    ];
    // charset format 0: GID 1..=4 -> CID 7, 9, 20, 42.
    let charset = vec![0, 0, 7, 0, 9, 0, 20, 0, 42];
    // FDSelect format 3: ranges [(0, FD 0), (1, FD 1), (3, FD 2)], sentinel 5.
    let fd_select = vec![3, 0, 3, 0, 0, 0, 0, 1, 1, 0, 3, 2, 0, 5];
    let gsubr_caller = {
        let mut cs = Vec::new();
        cs.extend(charstring_int(480)); // width operand
        cs.extend(charstring_int(100));
        cs.extend(charstring_int(0));
        cs.push(21); // rmoveto
        cs.extend(charstring_int(50));
        cs.extend(charstring_int(0));
        cs.push(5); // rlineto（直接绘制，保证 draws_path 检测为真）
        cs.extend(charstring_int(-107));
        cs.push(29); // callgsubr
        cs.push(14); // endchar
        cs
    };
    let lsubr_caller = {
        let mut cs = Vec::new();
        cs.extend(charstring_int(300)); // width operand
        cs.extend(charstring_int(0));
        cs.extend(charstring_int(0));
        cs.push(21); // rmoveto
        cs.extend(charstring_int(25));
        cs.extend(charstring_int(0));
        cs.push(5); // rlineto（直接绘制）
        cs.extend(charstring_int(-107));
        cs.push(10); // callsubr
        cs.push(14); // endchar
        cs
    };
    let charstrings = build_index(&[
        vec![14],
        rmoveto_rlineto_charstring(480),
        gsubr_caller,
        vec![14],
        lsubr_caller,
    ]);
    let fd_array_placeholder = build_index(&[font_dict(0, 0), font_dict(0, 0), font_dict(0, 0)]);

    let placeholder = cid_top_dict(Some(0), 0, 0, 0);
    let top_index_size = build_index(&[placeholder]).len();
    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + global_subrs.len();

    let charset_offset = prefix;
    let fd_select_offset = charset_offset + charset.len();
    let fd_array_offset = fd_select_offset + fd_select.len();
    let fd_private_base = fd_array_offset + fd_array_placeholder.len();
    let fd0_offset = fd_private_base;
    let fd1_offset = fd0_offset + fd_privates[0].len();
    let fd2_offset = fd1_offset + fd_privates[1].len();
    let char_strings_offset = fd2_offset + fd_privates[2].len();

    let fd_array = build_index(&[
        font_dict(fd_privates[0].len(), fd0_offset),
        font_dict(fd_privates[1].len(), fd1_offset),
        font_dict(fd_privates[2].len(), fd2_offset),
    ]);
    assert_eq!(
        fd_array.len(),
        fd_array_placeholder.len(),
        "FDArray size must not depend on offset values"
    );

    let top_dict = cid_top_dict(
        Some(charset_offset),
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
    out.extend(charset);
    out.extend(fd_select);
    out.extend(fd_array);
    for private in &fd_privates {
        out.extend_from_slice(private);
    }
    out.extend(charstrings);
    out
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
