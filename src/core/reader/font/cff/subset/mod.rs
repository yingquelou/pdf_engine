//! CID-keyed CFF 字体程序的子集化（ISO 32000-1 §9.7.6 / §9.9）。
//!
//! [`subset_cff`] 面向 PDF `/FontFile3`（`/Subtype /CIDFontType0C`）承载的
//! CID-keyed CFF1 程序（带 `ROS`）：按请求的 CID 列表裁剪字形并重排布局，
//! 输出仍是一个可被 `cff::raw` 解析层重新解析的合法 CID-keyed CFF。
//!
//! 子集语义：
//!
//! * CID 0（`.notdef`）始终保留；请求的 CID 去重后按升序保留；
//! * 新 GID 按保留 CID 升序重编号（CID 0 仍是 GID 0）；
//! * charset 统一重写为显式 format 0 表（GID → CID）；
//! * FDSelect 统一重写为 format 3 区间表（新 GID → 新 FD 索引）；
//! * FDArray 只保留被保留字形引用的 Font DICT 并重编号；每个保留 FD 的
//!   Private DICT 与其 Local Subr INDEX 整组保留（不裁剪 Subrs，控制
//!   复杂度），但 Private 内的 `Subrs` 相对偏移与 Font DICT 内的
//!   `Private (size, offset)` 都按新布局重写；
//! * Global Subr INDEX、Name INDEX、String INDEX 逐字节保留，因此
//!   charstring 内的 `callsubr`/`callgsubr` 编号保持有效；
//! * Top DICT 的 `charset`/`CharStrings`/`FDArray`/`FDSelect` 绝对偏移
//!   按新布局重算，其余条目（`ROS`、`CIDCount`、`FontMatrix` 等）逐字节
//!   保留。
//!
//! 取舍（务必阅读）：
//!
//! * **仅支持 CID-keyed CFF**。非 CID-keyed（无 `ROS`，如 `/Type1C`）
//!   返回 [`FontError::Invalid`]；name-keyed 子集请改用
//!   [`subset_cff_type1c`]（同目录的 `type1c` 模块）。
//! * CID-keyed 的 Top DICT 按规范不含 `Encoding`/`Private`；若实际出现，
//!   同样返回 [`FontError::Invalid`]（规范上不允许，且偏移改写无意义）。
//! * 请求的 CID 超出 `CIDCount` 或不在 charset 中返回 [`FontError::Invalid`]；
//!   空请求合法（只保留 CID 0）。
//!
//! 所有解析对截断/越界返回类型化错误，不 panic；全程迭代，无递归。

use std::collections::BTreeSet;

use super::raw::parse::{
    parse_charset, parse_fd_select, parse_font_dict, parse_index, parse_top_dict,
};
use crate::core::reader::font::{FontError, FontResult};

mod dict;
#[cfg(test)]
mod tests;
mod type1c;

use dict::{rewrite_dict, walk_dict};

pub use type1c::subset_cff_type1c;

/// Top DICT `charset` 偏移操作符。
pub(super) const OP_CHARSET: u16 = 15;
/// Top DICT `Encoding` 偏移操作符（CID-keyed 规范禁止）。
pub(super) const OP_ENCODING: u16 = 16;
/// Top DICT `CharStrings` 偏移操作符。
pub(super) const OP_CHAR_STRINGS: u16 = 17;
/// Top DICT `Private` 操作符（CID-keyed 规范禁止出现在顶层）。
pub(super) const OP_PRIVATE: u16 = 18;
/// Private DICT `Subrs`（相对偏移）操作符。
pub(super) const OP_SUBRS: u16 = 19;
/// Top DICT `FDArray` 偏移操作符（12 36）。
const OP_FD_ARRAY: u16 = 0x0C24;
/// Top DICT `FDSelect` 偏移操作符（12 37）。
const OP_FD_SELECT: u16 = 0x0C25;

/// 把一个 CID-keyed CFF 程序裁剪为只含 `cids`（外加 CID 0）的子集。
///
/// 语义与取舍见模块文档。输入截断、非 CID-keyed、CID 越界或未知时返回
/// [`FontError::Invalid`]；重复与未排序的 CID 列表均可接受。
pub fn subset_cff(data: &[u8], cids: &[u16]) -> FontResult<Vec<u8>> {
    let source = Source::parse(data)?;
    let retained = source.retained(cids)?;
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
    /// Global Subr INDEX 的原始字节段。
    gsubr_bytes: &'a [u8],
    /// Top DICT 的原始字节。
    top_dict_bytes: &'a [u8],
    /// GID → CID 表（index 0 恒为 0）。
    charset: Vec<u16>,
    /// GID → FD 索引表。
    fd_select: Vec<u8>,
    /// FDArray 各 Font DICT 的字节段。
    fd_array_entries: Vec<std::ops::Range<usize>>,
    /// CharStrings INDEX 各字形 charstring 的字节段。
    charstrings: Vec<std::ops::Range<usize>>,
    /// Top DICT 声明的 `CIDCount`。
    cid_count: Option<u16>,
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

        // 仅支持 CID-keyed：必须有 ROS、显式 charset、FDArray、FDSelect。
        if top_dict.ros.is_none() {
            return Err(invalid_subset(
                "CFF program is not CID-keyed (no ROS); only CID-keyed programs (/CIDFontType0C) can be subsetted",
            ));
        }
        let entries = walk_dict(top_dict_bytes)?;
        if entries.iter().any(|entry| entry.operator == OP_ENCODING) {
            return Err(invalid_subset(
                "CID-keyed CFF must not contain an Encoding operator",
            ));
        }
        if entries.iter().any(|entry| entry.operator == OP_PRIVATE) {
            return Err(invalid_subset(
                "CID-keyed CFF must not contain a Top DICT Private operator",
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

        let charset_offset = match top_dict.charset {
            Some(0..=2) => {
                return Err(invalid_subset(
                    "CID-keyed CFF uses a predefined charset; the specification forbids it",
                ));
            }
            Some(offset) => offset,
            None => {
                return Err(invalid_subset(
                    "CID-keyed CFF is missing the charset offset",
                ));
            }
        };
        let charset = parse_charset(data, charset_offset, glyph_count as u16)?;
        let fd_array_offset = top_dict
            .fd_array
            .ok_or_else(|| invalid_subset("CID-keyed CFF is missing the FDArray offset"))?;
        let fd_select_offset = top_dict
            .fd_select
            .ok_or_else(|| invalid_subset("CID-keyed CFF is missing the FDSelect offset"))?;
        let fd_array = parse_index(data, fd_array_offset)?;
        if fd_array.entries.is_empty() {
            return Err(invalid_subset("CFF FDArray INDEX is empty"));
        }
        let fd_select = parse_fd_select(data, fd_select_offset, glyph_count as u16)?;
        if let Some(&fd) = fd_select
            .iter()
            .find(|&&fd| usize::from(fd) >= fd_array.entries.len())
        {
            return Err(invalid_subset(format!(
                "CFF FDSelect references Font DICT {fd} but FDArray has {} entries",
                fd_array.entries.len()
            )));
        }
        let cid_count = top_dict.cid_count;

        Ok(Self {
            data,
            header_size,
            name_bytes,
            string_bytes,
            gsubr_bytes,
            top_dict_bytes,
            charset,
            fd_select,
            fd_array_entries: fd_array.entries,
            charstrings: char_strings_index.entries,
            cid_count,
        })
    }

    /// 计算保留集合：CID 0 加上请求的 CID，按 CID 升序得到新 GID 顺序。
    fn retained(&self, cids: &[u16]) -> FontResult<Retained> {
        let mut retained: BTreeSet<u16> = BTreeSet::new();
        retained.insert(0);
        for &cid in cids {
            if let Some(cid_count) = self.cid_count
                && cid >= cid_count
            {
                return Err(invalid_subset(format!(
                    "CID {cid} is out of range (CIDCount {cid_count})"
                )));
            }
            if !self.charset.contains(&cid) {
                return Err(invalid_subset(format!(
                    "CID {cid} is not present in the CFF charset"
                )));
            }
            retained.insert(cid);
        }
        let new_cids: Vec<u16> = retained.iter().copied().collect();
        let mut old_gids = Vec::with_capacity(new_cids.len());
        let mut old_fds = Vec::with_capacity(new_cids.len());
        for &cid in &new_cids {
            let gid = self
                .charset
                .iter()
                .position(|&value| value == cid)
                .expect("CID checked above") as u16;
            old_gids.push(gid);
            old_fds.push(self.fd_select[usize::from(gid)]);
        }
        // 被引用 FD 的旧索引去重排序 → 新索引。
        let used_fds: BTreeSet<u8> = old_fds.iter().copied().collect();
        let fd_map: Vec<u8> = used_fds.iter().copied().collect();
        let new_fds: Vec<u8> = old_fds
            .iter()
            .map(|fd| {
                fd_map
                    .iter()
                    .position(|old| old == fd)
                    .expect("fd collected above") as u8
            })
            .collect();
        Ok(Retained {
            new_cids,
            old_gids,
            new_fds,
            fd_map,
        })
    }

    /// 按保留集合重建整个 CFF 程序。
    ///
    /// 布局：头部、Name、Top DICT INDEX、String、Global Subr，随后是
    /// charset、FDSelect、FDArray、各保留 FD 的 Private(+Local Subrs)、
    /// CharStrings。所有 DICT 偏移改写为定长 longint，先用占位偏移
    /// 计算各段长度，再回填真实偏移（长度不变）。
    fn build(&self, retained: &Retained) -> FontResult<Vec<u8>> {
        // 1. 各变长段的字节内容（与偏移无关的部分）。
        let charset_bytes = build_charset(&retained.new_cids);
        let fd_select_bytes = build_fd_select(&retained.new_fds)?;
        let charstring_entries: Vec<&[u8]> = retained
            .old_gids
            .iter()
            .map(|&gid| &self.data[self.charstrings[usize::from(gid)].clone()])
            .collect();

        // 2. 各保留 FD 的 Private(+Local Subrs) 重写字节；None 表示无 Private。
        let mut private_blobs: Vec<Option<Vec<u8>>> = Vec::with_capacity(retained.fd_map.len());
        for &old_fd in &retained.fd_map {
            let entry = &self.data[self.fd_array_entries[usize::from(old_fd)].clone()];
            let font_dict = parse_font_dict(entry)?;
            private_blobs.push(match font_dict.private {
                Some((size, offset)) => Some(self.rewrite_private(size, offset)?),
                None => None,
            });
        }

        // 3. 第一趟：占位偏移求 Top DICT 与 FDArray 长度。
        let placeholder_top = self.rewrite_top_dict(0, 0, 0, 0)?;
        let top_index_len = write_index(&[&placeholder_top]).len();
        let placeholder_offsets = vec![0usize; private_blobs.len()];
        let placeholder_fd_array =
            self.build_fd_array(retained, &private_blobs, &placeholder_offsets)?;
        let fd_array_len = placeholder_fd_array.len();

        // 4. 计算真实偏移。
        let mut cursor = self.header_size
            + self.name_bytes.len()
            + top_index_len
            + self.string_bytes.len()
            + self.gsubr_bytes.len();
        let charset_offset = cursor;
        cursor += charset_bytes.len();
        let fd_select_offset = cursor;
        cursor += fd_select_bytes.len();
        let fd_array_offset = cursor;
        cursor += fd_array_len;
        let mut private_offsets = Vec::with_capacity(private_blobs.len());
        for blob in private_blobs.iter().flatten() {
            private_offsets.push(cursor);
            cursor += blob.len();
        }
        let char_strings_offset = cursor;

        // 5. 第二趟：回填真实偏移并拼接。
        let top_dict = self.rewrite_top_dict(
            charset_offset,
            fd_select_offset,
            fd_array_offset,
            char_strings_offset,
        )?;
        let fd_array = self.build_fd_array(retained, &private_blobs, &private_offsets)?;
        debug_assert_eq!(fd_array.len(), fd_array_len, "FDArray 长度应与首趟一致");
        debug_assert_eq!(
            write_index(&[&top_dict]).len(),
            top_index_len,
            "Top DICT INDEX 长度应与首趟一致"
        );

        let mut out = Vec::with_capacity(cursor);
        out.extend_from_slice(&self.data[..self.header_size]);
        out.extend_from_slice(self.name_bytes);
        out.extend_from_slice(&write_index(&[&top_dict]));
        out.extend_from_slice(self.string_bytes);
        out.extend_from_slice(self.gsubr_bytes);
        out.extend_from_slice(&charset_bytes);
        out.extend_from_slice(&fd_select_bytes);
        out.extend_from_slice(&fd_array);
        for blob in private_blobs.iter().flatten() {
            out.extend_from_slice(blob);
        }
        out.extend_from_slice(&write_index(&charstring_entries));
        Ok(out)
    }

    /// 重写 Top DICT：只替换四个绝对偏移条目，其余逐字节拷贝。
    fn rewrite_top_dict(
        &self,
        charset: usize,
        fd_select: usize,
        fd_array: usize,
        char_strings: usize,
    ) -> FontResult<Vec<u8>> {
        let entries = walk_dict(self.top_dict_bytes)?;
        Ok(rewrite_dict(
            self.top_dict_bytes,
            &entries,
            |entry| match entry.operator {
                OP_CHARSET => Some(vec![charset]),
                OP_FD_SELECT => Some(vec![fd_select]),
                OP_FD_ARRAY => Some(vec![fd_array]),
                OP_CHAR_STRINGS => Some(vec![char_strings]),
                _ => None,
            },
        ))
    }

    /// 重建 FDArray INDEX：保留 FD 的 Font DICT 中 `Private (size, offset)`
    /// 按新布局重写，其余条目逐字节拷贝。
    fn build_fd_array(
        &self,
        retained: &Retained,
        private_blobs: &[Option<Vec<u8>>],
        private_offsets: &[usize],
    ) -> FontResult<Vec<u8>> {
        let mut entries: Vec<Vec<u8>> = Vec::with_capacity(retained.fd_map.len());
        let mut private_index = 0usize;
        for (new_fd, &old_fd) in retained.fd_map.iter().enumerate() {
            let raw = &self.data[self.fd_array_entries[usize::from(old_fd)].clone()];
            let dict_entries = walk_dict(raw)?;
            let blob = private_blobs[new_fd].as_deref();
            // 有 Private 的 FD 按出现顺序消耗一个偏移槽位。
            let private_offset = blob.map(|_| {
                let offset = private_offsets[private_index];
                private_index += 1;
                offset
            });
            let rewritten = rewrite_dict(raw, &dict_entries, |entry| {
                // `parse_font_dict` 与这里的游走器看到同一批操作符：有
                // Private 条目必然已解析出 Private DICT，反之亦然。
                match (entry.operator == OP_PRIVATE, blob, private_offset) {
                    (true, Some(blob), Some(offset)) => Some(vec![blob.len(), offset]),
                    _ => None,
                }
            });
            entries.push(rewritten);
        }
        let refs: Vec<&[u8]> = entries.iter().map(Vec::as_slice).collect();
        Ok(write_index(&refs))
    }

    /// 重写一个 Private DICT 并附加其 Local Subr INDEX。
    ///
    /// Local Subrs 整组保留；`Subrs` 相对偏移改写为新 Private DICT 长度
    ///（Local Subr INDEX 紧随其后）。`Subrs` 条目以定长 longint 重建，
    /// 因此新长度可先用占位偏移求得。
    fn rewrite_private(&self, size: usize, offset: usize) -> FontResult<Vec<u8>> {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| invalid_subset("CFF Private DICT range overflows"))?;
        let private = self.data.get(offset..end).ok_or_else(|| {
            invalid_subset(format!(
                "CFF Private DICT range {offset}..{end} is outside the program"
            ))
        })?;
        let entries = walk_dict(private)?;
        let subrs_entry = entries
            .iter()
            .rev()
            .find(|entry| entry.operator == OP_SUBRS);
        let Some(subrs_entry) = subrs_entry else {
            return Ok(private.to_vec());
        };
        let relative = match subrs_entry.operands.as_slice() {
            [value] if *value >= 0.0 && value.fract() == 0.0 => *value as usize,
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
        let mut blob = rewritten;
        blob.extend_from_slice(subrs_bytes);
        Ok(blob)
    }
}

/// 子集保留集合：新 GID 顺序由保留 CID 升序决定。
struct Retained {
    /// 新 GID → CID（index 0 恒为 CID 0）。
    new_cids: Vec<u16>,
    /// 新 GID → 源 GID。
    old_gids: Vec<u16>,
    /// 新 GID → 新 FD 索引。
    new_fds: Vec<u8>,
    /// 被保留的旧 FD 索引（升序），其位置即新 FD 索引。
    fd_map: Vec<u8>,
}

/// 重写 charset 为显式 format 0（GID 0 隐含为 0，不写入表内）。
pub(super) fn build_charset(new_cids: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 2 * new_cids.len());
    out.push(0);
    for &cid in &new_cids[1..] {
        out.extend_from_slice(&cid.to_be_bytes());
    }
    out
}

/// 重写 FDSelect 为 format 3 区间表。
fn build_fd_select(new_fds: &[u8]) -> FontResult<Vec<u8>> {
    let glyph_count = u16::try_from(new_fds.len())
        .map_err(|_| invalid_subset("subset glyph count exceeds 65535"))?;
    let mut ranges: Vec<(u16, u8)> = Vec::new();
    for (gid, &fd) in new_fds.iter().enumerate() {
        let gid = gid as u16;
        match ranges.last_mut() {
            Some(last) if last.1 == fd => {}
            _ => ranges.push((gid, fd)),
        }
    }
    let mut out = Vec::with_capacity(5 + 3 * ranges.len());
    out.push(3);
    out.extend_from_slice(&(ranges.len() as u16).to_be_bytes());
    for &(first, fd) in &ranges {
        out.extend_from_slice(&first.to_be_bytes());
        out.push(fd);
    }
    out.extend_from_slice(&glyph_count.to_be_bytes());
    Ok(out)
}

/// 序列化一个 CFF INDEX。
pub(super) fn write_index(entries: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    if entries.is_empty() {
        return out;
    }
    let mut offsets = vec![1usize];
    let mut total = 1usize;
    for entry in entries {
        total += entry.len();
        offsets.push(total);
    }
    let last = *offsets.last().expect("non-empty entries");
    let off_size: u8 = if last <= 0xFF {
        1
    } else if last <= 0xFFFF {
        2
    } else if last <= 0xFF_FFFF {
        3
    } else {
        4
    };
    out.push(off_size);
    for offset in &offsets {
        for byte in (0..usize::from(off_size)).rev() {
            out.push((offset >> (byte * 8)) as u8);
        }
    }
    for entry in entries {
        out.extend_from_slice(entry);
    }
    out
}

pub(super) fn invalid_subset(message: impl Into<String>) -> FontError {
    FontError::Invalid(format!("invalid CFF subset: {}", message.into()))
}
