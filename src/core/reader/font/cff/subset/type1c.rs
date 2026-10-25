//! name-keyed（无 `ROS`）CFF1 字体程序的子集化（ISO 32000-1 §9.6.4 / §9.9）。
//!
//! [`subset_cff_type1c`] 面向 PDF `/FontFile3`（`/Subtype /Type1C`）承载的
//! name-keyed CFF1 程序：按请求的字形名裁剪 CharStrings 并重排布局，输出
//! 仍是一个可被 `cff::raw` 解析层重新解析的合法 name-keyed CFF。
//!
//! 子集语义：
//!
//! * GID 0（`.notdef`）始终保留；请求字形按原 GID 升序重编号；
//! * 字形名经 charset（GID → SID）与 String INDEX（SID → 名字）解析；
//! * charset 统一重写为显式 format 0 表，保留字形的 SID 原样写入；
//!   String INDEX 与 Name INDEX 整组保留，因此 SID 的有效性不变；
//! * CharStrings INDEX 只含保留字形；Top DICT 的 `charset`/`Private`/
//!   `CharStrings` 绝对偏移按新布局重算，其余条目逐字节拷贝；
//! * Private DICT 与其 Local Subr INDEX 整组保留（Subrs 编号保持有效），
//!   `Subrs` 相对偏移按新 Private DICT 长度重写；Global Subr INDEX 整组
//!   保留，charstring 内的 `callsubr`/`callgsubr` 编号不变。
//!
//! 取舍（务必阅读）：
//!
//! * **仅支持 name-keyed CFF**。带 `ROS` 的 CID-keyed 程序返回
//!   [`FontError::Invalid`]，请改用 [`subset_cff`](super::subset_cff)；
//!   带 `FDArray`/`FDSelect` 的程序同样按 name-keyed 结构不合法拒绝。
//! * Top DICT 的 `Encoding` **直接删除**。字形重编号后旧内建编码的
//!   code → GID 对应关系不再成立，沿用它会产生错位字形；删除后输出
//!   退回 CFF 默认的 StandardEncoding（此编码不是输出本身的重点，PDF
//!   简单字体由字体字典的 `/Encoding` 决定字符码映射）。
//! * 预定义 charset 0（ISOAdobe）、1（Expert）与 2（ExpertSubset）都
//!   展开成显式 SID 表；显式 charset format 0/1/2 均可解析。缺省
//!   charset（无 `charset` 操作符）按 CFF 默认的 ISOAdobe 处理。
//! * 未知字形名、SID 越界/非法字符串、截断或畸形输入返回
//!   [`FontError::Invalid`]，不 panic；全程迭代，无递归。
//!
//! [`FontError::Invalid`]: crate::core::reader::font::FontError::Invalid

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::dict::{dict_longint, rewrite_dict, rewrite_dict_skipping, walk_dict};
use super::strings::{
    EXPERT_CHARSET_SIDS, EXPERT_SUBSET_CHARSET_SIDS, ISO_ADOBE_CHARSET_LEN, STANDARD_STRINGS,
};
use super::{
    OP_CHAR_STRINGS, OP_CHARSET, OP_ENCODING, OP_PRIVATE, OP_SUBRS, build_charset, invalid_subset,
    write_index,
};
use crate::core::reader::font::FontResult;
use crate::core::reader::font::cff::raw::parse::{parse_charset, parse_index, parse_top_dict};

/// 把一个 name-keyed CFF 程序裁剪为只含 `glyph_names`（外加 `.notdef`）。
///
/// `glyph_names` 是源程序 charset 中的字形名（如 `"A"`、`"space"`），
/// 重复项可接受；`.notdef` 无论是否请求都会保留。保留字形按原 GID 升序
/// 重编号为连续的 GID 序列。
///
/// 截断/畸形输入、CID-keyed（含 `ROS`）程序、未知字形名返回
/// [`FontError::Invalid`]；空请求合法（只保留 `.notdef`）。
pub fn subset_cff_type1c(data: &[u8], glyph_names: &[&str]) -> FontResult<Vec<u8>> {
    let source = Source::parse(data)?;
    let retained = source.retained(glyph_names)?;
    source.build(&retained)
}

/// 源程序解析结果：只记录子集化需要的布局信息。
struct Source<'a> {
    data: &'a [u8],
    /// 头部长度（`hdrSize`）。
    header_size: usize,
    /// Name INDEX 的原始字节段。
    name_bytes: &'a [u8],
    /// String INDEX 的原始字节段。
    string_bytes: &'a [u8],
    /// String INDEX 各字符串（名字）的字节段，SID 391 起有效。
    strings: Vec<Range<usize>>,
    /// Global Subr INDEX 的原始字节段。
    gsubr_bytes: &'a [u8],
    /// Top DICT 的原始字节。
    top_dict_bytes: &'a [u8],
    /// GID → SID 表（index 0 恒为 0）。
    charset: Vec<u16>,
    /// CharStrings INDEX 各字形 charstring 的字节段。
    charstrings: Vec<Range<usize>>,
    /// Top DICT 的 `Private (size, offset)`。
    private: Option<(usize, usize)>,
}

impl<'a> Source<'a> {
    fn parse(data: &'a [u8]) -> FontResult<Self> {
        if data.len() < 4 {
            return Err(invalid_subset("truncated CFF header"));
        }
        if data[0] != 1 {
            return Err(invalid_subset(format!(
                "unsupported CFF major version {}; only CFF 1.x is supported",
                data[0]
            )));
        }
        let header_size = usize::from(data[2]);
        if header_size < 4 || header_size > data.len() {
            return Err(invalid_subset(format!(
                "invalid CFF header size {header_size}"
            )));
        }

        let name_index = parse_index(data, header_size)?;
        let name_bytes = &data[header_size..name_index.end];
        let top_index = parse_index(data, name_index.end)?;
        let top_dict_range = top_index
            .entries
            .first()
            .ok_or_else(|| invalid_subset("CFF Top DICT INDEX is empty"))?;
        let top_dict_bytes = &data[top_dict_range.clone()];
        let top_dict = parse_top_dict(top_dict_bytes)?;
        let string_index = parse_index(data, top_index.end)?;
        let string_bytes = &data[top_index.end..string_index.end];
        let gsubr_index = parse_index(data, string_index.end)?;
        let gsubr_bytes = &data[string_index.end..gsubr_index.end];

        // 本入口只处理 name-keyed 程序；CID-keyed 交给 `subset_cff`。
        if top_dict.ros.is_some() {
            return Err(invalid_subset(
                "CFF program is CID-keyed (has ROS); use subset_cff instead",
            ));
        }
        // FDArray/FDSelect 是 CID-keyed 专用结构；出现在 name-keyed 程序中
        // 时无法安全改写其偏移。
        if top_dict.fd_array.is_some() || top_dict.fd_select.is_some() {
            return Err(invalid_subset(
                "name-keyed CFF must not contain FDArray or FDSelect",
            ));
        }

        let char_strings_offset = top_dict
            .char_strings
            .ok_or_else(|| invalid_subset("CFF Top DICT is missing the CharStrings offset"))?;
        let char_strings_index = parse_index(data, char_strings_offset)?;
        let glyph_count = char_strings_index.entries.len();
        if glyph_count == 0 || glyph_count > usize::from(u16::MAX) {
            return Err(invalid_subset(format!(
                "CFF CharStrings INDEX has {glyph_count} glyphs"
            )));
        }
        let glyph_count = glyph_count as u16;

        // 无 charset 操作符时按 CFF 默认的 ISOAdobe（charset 0）处理。
        let charset = match top_dict.charset {
            None => predefined_charset(0, glyph_count)?,
            Some(id @ 0..=2) => predefined_charset(id as u8, glyph_count)?,
            Some(offset) => parse_charset(data, offset, glyph_count)?,
        };

        Ok(Self {
            data,
            header_size,
            name_bytes,
            string_bytes,
            strings: string_index.entries,
            gsubr_bytes,
            top_dict_bytes,
            charset,
            charstrings: char_strings_index.entries,
            private: top_dict.private,
        })
    }

    /// 解析 GID 的字形名：SID < 391 查标准字符串，否则查 String INDEX。
    fn glyph_name(&self, gid: usize) -> FontResult<String> {
        let sid = usize::from(
            *self
                .charset
                .get(gid)
                .ok_or_else(|| invalid_subset(format!("CFF charset has no SID for GID {gid}")))?,
        );
        if let Some(name) = STANDARD_STRINGS.get(sid) {
            return Ok((*name).to_string());
        }
        let index = sid - STANDARD_STRINGS.len();
        let range = self.strings.get(index).ok_or_else(|| {
            invalid_subset(format!(
                "CFF SID {sid} is outside the String INDEX ({} strings)",
                self.strings.len()
            ))
        })?;
        std::str::from_utf8(&self.data[range.clone()])
            .map(str::to_owned)
            .map_err(|_| invalid_subset(format!("CFF glyph name for SID {sid} is not valid UTF-8")))
    }

    /// 计算保留集合：GID 0 加上按名字命中的 GID，按原 GID 升序排列。
    fn retained(&self, glyph_names: &[&str]) -> FontResult<Retained> {
        let mut retained: BTreeSet<u16> = BTreeSet::new();
        retained.insert(0);
        if !glyph_names.is_empty() {
            // 只在有请求时才枚举全部字形名，空请求无需解析 String INDEX。
            let names: Vec<String> = (0..self.charset.len())
                .map(|gid| self.glyph_name(gid))
                .collect::<FontResult<_>>()?;
            let mut by_name: BTreeMap<&str, u16> = BTreeMap::new();
            for (gid, name) in names.iter().enumerate() {
                // 同名重复时取最小 GID，保证确定性。
                by_name.entry(name.as_str()).or_insert(gid as u16);
            }
            for &name in glyph_names {
                let gid = by_name.get(name).copied().ok_or_else(|| {
                    invalid_subset(format!(
                        "requested glyph name `{name}` is not present in the CFF charset"
                    ))
                })?;
                retained.insert(gid);
            }
        }
        let old_gids: Vec<u16> = retained.iter().copied().collect();
        let sids: Vec<u16> = old_gids
            .iter()
            .map(|&gid| self.charset[usize::from(gid)])
            .collect();
        Ok(Retained { old_gids, sids })
    }

    /// 按保留集合重建整个 CFF 程序。
    ///
    /// 布局：头部、Name、Top DICT INDEX、String、Global Subr，随后是
    /// charset、Private(+Local Subrs)、CharStrings。所有偏移条目以定长
    /// longint 重写，先用占位偏移求长度，再回填真实偏移（长度不变）。
    fn build(&self, retained: &Retained) -> FontResult<Vec<u8>> {
        // 1. 与偏移无关的各段字节。
        let charset_bytes = build_charset(&retained.sids);
        let charstring_entries: Vec<&[u8]> = retained
            .old_gids
            .iter()
            .map(|&gid| &self.data[self.charstrings[usize::from(gid)].clone()])
            .collect();
        // Private blob 与 Private DICT 自身长度（后者才是 Top DICT 的 size）。
        let private = match self.private {
            Some((size, offset)) => Some(self.rewrite_private(size, offset)?),
            None => None,
        };

        // 2. 第一趟：占位偏移求 Top DICT INDEX 长度。
        let placeholder_top =
            self.rewrite_top_dict(0, private.as_ref().map(|(_, dict_len)| (*dict_len, 0)), 0)?;
        let top_index_len = write_index(&[&placeholder_top]).len();

        // 3. 计算真实偏移。
        let mut cursor = self.header_size
            + self.name_bytes.len()
            + top_index_len
            + self.string_bytes.len()
            + self.gsubr_bytes.len();
        let charset_offset = cursor;
        cursor += charset_bytes.len();
        let private_offset = private.as_ref().map(|(blob, _)| {
            let offset = cursor;
            cursor += blob.len();
            offset
        });
        let char_strings_offset = cursor;

        // 4. 第二趟：回填真实偏移并拼接。
        let top_dict = self.rewrite_top_dict(
            charset_offset,
            private
                .as_ref()
                .map(|(_, dict_len)| (*dict_len, private_offset.expect("Private 偏移已求得"))),
            char_strings_offset,
        )?;
        debug_assert_eq!(
            write_index(&[&top_dict]).len(),
            top_index_len,
            "Top DICT INDEX 长度应与首趟一致"
        );

        let mut out = Vec::with_capacity(cursor + 4);
        out.extend_from_slice(&self.data[..self.header_size]);
        out.extend_from_slice(self.name_bytes);
        out.extend_from_slice(&write_index(&[&top_dict]));
        out.extend_from_slice(self.string_bytes);
        out.extend_from_slice(self.gsubr_bytes);
        out.extend_from_slice(&charset_bytes);
        if let Some((blob, _)) = &private {
            out.extend_from_slice(blob);
        }
        out.extend_from_slice(&write_index(&charstring_entries));
        Ok(out)
    }

    /// 重写 Top DICT：替换 `charset`/`Private`/`CharStrings` 偏移，删除
    /// `Encoding`，其余条目逐字节拷贝；源缺少 `charset` 时补写显式条目。
    fn rewrite_top_dict(
        &self,
        charset: usize,
        private: Option<(usize, usize)>,
        char_strings: usize,
    ) -> FontResult<Vec<u8>> {
        let entries = walk_dict(self.top_dict_bytes)?;
        let has_charset = entries.iter().any(|entry| entry.operator == OP_CHARSET);
        let mut out = rewrite_dict_skipping(
            self.top_dict_bytes,
            &entries,
            |entry| entry.operator == OP_ENCODING,
            |entry| match entry.operator {
                OP_CHARSET => Some(vec![charset]),
                OP_CHAR_STRINGS => Some(vec![char_strings]),
                OP_PRIVATE => private.map(|(size, offset)| vec![size, offset]),
                _ => None,
            },
        );
        if !has_charset {
            // CFF 默认 charset 是预定义 ISOAdobe；子集后 SID 顺序已变，
            // 必须写出显式 charset 表。
            out.extend_from_slice(&dict_longint(charset));
            out.push(OP_CHARSET as u8);
        }
        Ok(out)
    }

    /// 重写一个 Private DICT 并附加其 Local Subr INDEX。
    ///
    /// 返回 `(完整 blob, Private DICT 自身长度)`：Top DICT 的 `Private`
    /// size 只覆盖 DICT 本身，Local Subr INDEX 紧随其后但不计入 size。
    fn rewrite_private(&self, size: usize, offset: usize) -> FontResult<(Vec<u8>, usize)> {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| invalid_subset("CFF Private DICT range overflows"))?;
        let private = self.data.get(offset..end).ok_or_else(|| {
            invalid_subset(format!(
                "CFF Private DICT range {offset}..{end} is outside the program"
            ))
        })?;
        let entries = walk_dict(private)?;
        let Some(subrs_entry) = entries
            .iter()
            .rev()
            .find(|entry| entry.operator == OP_SUBRS)
        else {
            return Ok((private.to_vec(), private.len()));
        };
        let relative = match subrs_entry.operands.as_slice() {
            [value] => non_negative_integer(*value)
                .ok_or_else(|| invalid_subset("CFF Private DICT Subrs offset is invalid"))?,
            _ => {
                return Err(invalid_subset(
                    "CFF Private DICT Subrs requires one non-negative integer operand",
                ));
            }
        };
        let subrs_offset = offset
            .checked_add(relative)
            .ok_or_else(|| invalid_subset("CFF Local Subrs offset overflows"))?;
        let subrs_index = parse_index(self.data, subrs_offset)?;
        let subrs_bytes = &self.data[subrs_offset..subrs_index.end];

        // 第一趟：占位偏移求新 Private DICT 长度；第二趟回填（长度不变）。
        let placeholder = rewrite_dict(private, &entries, |entry| {
            (entry.operator == OP_SUBRS).then(|| vec![0])
        });
        let rewritten = rewrite_dict(private, &entries, |entry| {
            (entry.operator == OP_SUBRS).then(|| vec![placeholder.len()])
        });
        debug_assert_eq!(rewritten.len(), placeholder.len());
        let dict_len = rewritten.len();
        let mut blob = rewritten;
        blob.extend_from_slice(subrs_bytes);
        Ok((blob, dict_len))
    }
}

/// 子集保留集合：新 GID 顺序由原 GID 升序决定。
struct Retained {
    /// 新 GID → 源 GID。
    old_gids: Vec<u16>,
    /// 新 GID → SID（index 0 恒为 SID 0）。
    sids: Vec<u16>,
}

/// 展开预定义 charset `id`（0/1/2）的前 `glyph_count` 个 GID。
fn predefined_charset(id: u8, glyph_count: u16) -> FontResult<Vec<u16>> {
    let count = usize::from(glyph_count);
    let (sids, name): (&[u16], &str) = match id {
        0 => {
            if count > ISO_ADOBE_CHARSET_LEN {
                return Err(invalid_subset(format!(
                    "ISOAdobe charset covers {ISO_ADOBE_CHARSET_LEN} glyphs but the program has {count}"
                )));
            }
            // ISOAdobe：GID i 的 SID 即 i。
            return Ok((0..glyph_count).collect());
        }
        1 => (EXPERT_CHARSET_SIDS, "Expert"),
        2 => (EXPERT_SUBSET_CHARSET_SIDS, "ExpertSubset"),
        _ => {
            return Err(invalid_subset(format!(
                "unsupported predefined CFF charset {id}"
            )));
        }
    };
    if count > sids.len() {
        return Err(invalid_subset(format!(
            "{name} charset covers {} glyphs but the program has {count}",
            sids.len()
        )));
    }
    Ok(sids[..count].to_vec())
}

/// 把一个 DICT 数字操作数转换为非负整数偏移。
fn non_negative_integer(value: f64) -> Option<usize> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= f64::from(u32::MAX) {
        Some(value as usize)
    } else {
        None
    }
}
