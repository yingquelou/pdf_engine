//! name-keyed CFF 合成样本（从 `fixtures.rs` 拆出，保持单文件规模可控）。

use super::fixtures::{
    build_index, charstring_int, name_keyed_top_dict_full, private_dict, private_dict_with_subrs,
    rmoveto_rlineto_charstring,
};

enum NameKeyedCharset<'a> {
    /// 显式 charset 表字节（format 0/1/2）。
    Explicit(&'a [u8]),
    /// 预定义 charset 操作符（0/1/2）。
    Predefined(u16),
    /// 不写 charset 操作符（CFF 默认 ISOAdobe）。
    Missing,
}

/// 构造一个 name-keyed CFF 程序。
///
/// `strings` 依次占用 SID 391、392……；`charstrings` 的条数即字形数，
/// 且 GID 0 约定为 `.notdef`。各段顺序为 charset、Encoding、Private
///（+ Local Subrs）、CharStrings。
fn name_keyed_cff_impl(
    charset: NameKeyedCharset<'_>,
    strings: &[&[u8]],
    charstrings: &[Vec<u8>],
    with_encoding: bool,
    with_subrs: bool,
) -> Vec<u8> {
    let header = [1u8, 0, 4, 1];
    let name_index = build_index(&[b"NKTest".to_vec()]);
    let string_entries: Vec<Vec<u8>> = strings.iter().map(|name| name.to_vec()).collect();
    let string_index = build_index(&string_entries);
    let global_subrs = build_index(&[]);
    let encoding = vec![0, 1, 65]; // Encoding format 0，仅一个编码 65
    let local_subr = {
        let mut subr = charstring_int(50);
        subr.extend(charstring_int(50));
        subr.push(5); // rlineto
        subr.push(11); // return
        subr
    };
    let local_subrs = build_index(&[local_subr]);
    let private = if with_subrs {
        private_dict_with_subrs(500, 50, &local_subrs)
    } else {
        private_dict(500, 50)
    };
    // Top DICT 的 Private size 只覆盖 Private DICT 本身。
    let private_size = private.len() - if with_subrs { local_subrs.len() } else { 0 };
    let charstrings_index = build_index(charstrings);

    let (placeholder_operand, charset_bytes) = match &charset {
        NameKeyedCharset::Explicit(bytes) => (Some(0), Some(*bytes)),
        NameKeyedCharset::Predefined(id) => (Some(usize::from(*id)), None),
        NameKeyedCharset::Missing => (None, None),
    };
    let placeholder =
        name_keyed_top_dict_full(placeholder_operand, with_encoding.then_some(0), 0, 0, 0);
    let top_index_size = build_index(std::slice::from_ref(&placeholder)).len();
    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + global_subrs.len();

    let charset_len = charset_bytes.map_or(0, <[u8]>::len);
    let charset_offset = prefix;
    let encoding_offset = with_encoding.then_some(charset_offset + charset_len);
    let private_offset =
        charset_offset + charset_len + if with_encoding { encoding.len() } else { 0 };
    let char_strings_offset = private_offset + private.len();

    let charset_operand = match &charset {
        NameKeyedCharset::Explicit(_) => Some(charset_offset),
        NameKeyedCharset::Predefined(id) => Some(usize::from(*id)),
        NameKeyedCharset::Missing => None,
    };
    let top_dict = name_keyed_top_dict_full(
        charset_operand,
        encoding_offset,
        private_size,
        private_offset,
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
    if let Some(bytes) = charset_bytes {
        out.extend_from_slice(bytes);
    }
    if with_encoding {
        out.extend(encoding);
    }
    out.extend(private);
    out.extend(charstrings_index);
    out
}

/// 三个自定义字形名（SID 391..=393）。
const CUSTOM_GLYPH_NAMES: &[&[u8]] = &[b"Alpha", b"Beta", b"Gamma"];

/// 自定义字形名的四个 charstring（GID 0 为 `.notdef`）。
fn custom_charstrings() -> Vec<Vec<u8>> {
    vec![
        vec![14],
        rmoveto_rlineto_charstring(550),
        rmoveto_rlineto_charstring(560),
        rmoveto_rlineto_charstring(570),
    ]
}

/// charset format 0：GID 1 起逐个写入 SID（GID 0 隐含 SID 0）。
fn charset_format_0(sids: &[u16]) -> Vec<u8> {
    let mut out = vec![0];
    for sid in sids {
        out.extend_from_slice(&sid.to_be_bytes());
    }
    out
}

/// 带自定义字形名与显式 charset 的 name-keyed CFF。
///
/// `format` 为 `0`（逐个 SID）、`1`（`first` + `nLeft` u8）或 `2`
///（`first` + `nLeft` u16）；三者的 GID 1..=3 都对应 SID 391..=393。
pub(crate) fn name_keyed_cff_custom_charset(format: u8) -> Vec<u8> {
    let charset = match format {
        0 => charset_format_0(&[391, 392, 393]),
        1 => {
            let mut out = vec![1];
            out.extend_from_slice(&391u16.to_be_bytes());
            out.push(2); // nLeft：SID 391..=393
            out
        }
        2 => {
            let mut out = vec![2];
            out.extend_from_slice(&391u16.to_be_bytes());
            out.extend_from_slice(&2u16.to_be_bytes()); // nLeft：SID 391..=393
            out
        }
        other => panic!("unsupported charset fixture format {other}"),
    };
    name_keyed_cff_impl(
        NameKeyedCharset::Explicit(&charset),
        CUSTOM_GLYPH_NAMES,
        &custom_charstrings(),
        false,
        false,
    )
}

/// 同 [`name_keyed_cff_custom_charset`] 的 format 0，但 Top DICT 带
/// `Encoding`（format 0，单编码 65 → GID 1）。
pub(crate) fn name_keyed_cff_with_encoding() -> Vec<u8> {
    let charset = charset_format_0(&[391, 392, 393]);
    name_keyed_cff_impl(
        NameKeyedCharset::Explicit(&charset),
        CUSTOM_GLYPH_NAMES,
        &custom_charstrings(),
        true,
        false,
    )
}

/// 同 format 0 自定义 charset，但 Private DICT 带 Local Subr INDEX；
/// GID 3 的 charstring 通过 `callsubr` 调用子程序。
pub(crate) fn name_keyed_cff_with_local_subrs() -> Vec<u8> {
    let charset = charset_format_0(&[391, 392, 393]);
    let mut caller = Vec::new();
    caller.extend(charstring_int(570)); // width 操作数
    caller.extend(charstring_int(0));
    caller.extend(charstring_int(0));
    caller.push(21); // rmoveto
    caller.extend(charstring_int(25));
    caller.extend(charstring_int(0));
    caller.push(5); // rlineto
    caller.extend(charstring_int(-107)); // subr 0（bias 107）
    caller.push(10); // callsubr
    caller.push(14); // endchar
    let charstrings = vec![
        vec![14],
        rmoveto_rlineto_charstring(550),
        rmoveto_rlineto_charstring(560),
        caller,
    ];
    name_keyed_cff_impl(
        NameKeyedCharset::Explicit(&charset),
        CUSTOM_GLYPH_NAMES,
        &charstrings,
        false,
        true,
    )
}

/// 预定义 charset 1（Expert）或 2（ExpertSubset）的 name-keyed CFF。
///
/// String INDEX 为空，字形名来自标准字符串表；GID 1 为 `space`，
/// GID 2 在 Expert 下为 `exclamsmall`（SID 229）、在 ExpertSubset 下为
/// `dollaroldstyle`（SID 231）。`glyph_count` 不得超过对应 charset 的容量。
pub(crate) fn name_keyed_cff_predefined_charset(charset_id: u16, glyph_count: usize) -> Vec<u8> {
    assert!(
        (1..=2).contains(&charset_id),
        "fixture only covers Expert charsets"
    );
    let mut charstrings = vec![vec![14]];
    for gid in 1..glyph_count {
        charstrings.push(rmoveto_rlineto_charstring(550 + 10 * gid as i16));
    }
    name_keyed_cff_impl(
        NameKeyedCharset::Predefined(charset_id),
        &[],
        &charstrings,
        false,
        false,
    )
}

/// 不带 `charset` 操作符的 name-keyed CFF（CFF 默认 ISOAdobe）。
pub(crate) fn name_keyed_cff_without_charset() -> Vec<u8> {
    let charstrings = vec![vec![14], rmoveto_rlineto_charstring(550)];
    name_keyed_cff_impl(NameKeyedCharset::Missing, &[], &charstrings, false, false)
}
