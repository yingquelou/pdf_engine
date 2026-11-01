//! TrueType `cmap` 表：子表选择与各 format 的码→字形映射。
//!
//! 主类型与子表选择保留在本模块；具体实现按职责拆分：
//! - `raw`：大端读取、偏移溢出辅助与共享错误构造；
//! - `validate`：`Cmap::parse` 选中子表后的结构性校验；
//! - `lookup`：formats 0/2/4/6/8/10/12/13 的码→字形查询；
//! - `reverse`：字形→码点的反向映射。

mod lookup;
mod raw;
mod reverse;
mod validate;

#[cfg(test)]
mod tests;

use std::ops::Range;

use super::super::FontError;

use self::raw::{checked_add, invalid_cmap, read_u16, read_u32};

/// A supported TrueType character-map format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmapFormat {
    Format0,
    Format2,
    Format4,
    Format6,
    Format8,
    Format10,
    Format12,
    Format13,
}

/// First byte of the `subHeaderKeys[256]` array in a format 2 subtable.
const FORMAT2_SUBHEADER_KEYS_START: usize = 6;
/// First byte of the `subHeaders[]` array in a format 2 subtable.
const FORMAT2_SUBHEADERS_START: usize = FORMAT2_SUBHEADER_KEYS_START + 256 * 2;
/// Size of one format 2 `SubHeader` record.
const FORMAT2_SUBHEADER_SIZE: usize = 8;

/// Byte offset of the `is32[8192]` bitmap in a format 8 subtable.
const FORMAT8_IS32_START: usize = 12;
/// Byte offset of `nGroups` in a format 8 subtable.
const FORMAT8_GROUP_COUNT_START: usize = FORMAT8_IS32_START + 8192;
/// First byte of the group array in a format 8 subtable.
const FORMAT8_GROUPS_START: usize = FORMAT8_GROUP_COUNT_START + 4;
/// Size of one format 8/12/13 group record.
const GROUP_SIZE: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cmap {
    subtable: Range<usize>,
    format: CmapFormat,
    symbolic: bool,
}

impl Cmap {
    pub fn parse(data: &[u8], glyph_count: u16) -> Result<Self, FontError> {
        if data.len() < 4 {
            return Err(invalid_cmap("header is truncated"));
        }
        if read_u16(data, 0)? != 0 {
            return Err(invalid_cmap("unsupported cmap table version"));
        }
        let record_count = usize::from(read_u16(data, 2)?);
        let records_end = 4usize
            .checked_add(
                record_count
                    .checked_mul(8)
                    .ok_or_else(|| invalid_cmap("encoding record count overflows"))?,
            )
            .ok_or_else(|| invalid_cmap("encoding record range overflows"))?;
        if record_count == 0 || records_end > data.len() {
            return Err(invalid_cmap("encoding records are truncated or empty"));
        }

        let mut selected: Option<(u8, Range<usize>, CmapFormat, bool)> = None;
        for index in 0..record_count {
            let record = 4 + index * 8;
            let platform = read_u16(data, record)?;
            let encoding = read_u16(data, record + 2)?;
            let offset = usize::try_from(read_u32(data, record + 4)?)
                .map_err(|_| invalid_cmap("subtable offset does not fit this platform"))?;
            let format = read_u16(data, offset)?;
            let Some((priority, format)) = cmap_priority(platform, encoding, format) else {
                continue;
            };
            let symbolic = (platform, encoding) == (3, 0);
            let length = match format {
                CmapFormat::Format0 => 262,
                CmapFormat::Format2 => usize::from(read_u16(data, checked_add(offset, 2)?)?),
                CmapFormat::Format4 => usize::from(read_u16(data, checked_add(offset, 2)?)?),
                CmapFormat::Format6 => usize::from(read_u16(data, checked_add(offset, 2)?)?),
                CmapFormat::Format8 => usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                    .map_err(|_| invalid_cmap("format 8 length does not fit this platform"))?,
                CmapFormat::Format10 => {
                    usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                        .map_err(|_| invalid_cmap("format 10 length does not fit this platform"))?
                }
                CmapFormat::Format12 => {
                    usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                        .map_err(|_| invalid_cmap("format 12 length does not fit this platform"))?
                }
                CmapFormat::Format13 => {
                    usize::try_from(read_u32(data, checked_add(offset, 4)?)?)
                        .map_err(|_| invalid_cmap("format 13 length does not fit this platform"))?
                }
            };
            let end = offset
                .checked_add(length)
                .ok_or_else(|| invalid_cmap("subtable range overflows"))?;
            if offset < records_end || end > data.len() {
                return Err(invalid_cmap("subtable range is outside the cmap table"));
            }
            if selected
                .as_ref()
                .is_none_or(|(existing_priority, _, _, _)| priority < *existing_priority)
            {
                selected = Some((priority, offset..end, format, symbolic));
            }
        }
        let Some((_, subtable, format, symbolic)) = selected else {
            return Err(invalid_cmap(
                "no supported cmap format 0, 2, 4, 6, 8, 10, 12, or 13 subtable",
            ));
        };
        let cmap = Self {
            subtable,
            format,
            symbolic,
        };
        cmap.validate(data, glyph_count)?;
        Ok(cmap)
    }

    pub fn format(&self) -> CmapFormat {
        self.format
    }

    /// Return whether the selected subtable is the Microsoft Symbol (3, 0)
    /// encoding, whose codes commonly occupy `0xF000..=0xF0FF`.
    pub(super) fn is_symbolic(&self) -> bool {
        self.symbolic
    }
}

fn cmap_priority(platform: u16, encoding: u16, format: u16) -> Option<(u8, CmapFormat)> {
    let supported_format = match format {
        0 => CmapFormat::Format0,
        2 => CmapFormat::Format2,
        4 => CmapFormat::Format4,
        6 => CmapFormat::Format6,
        8 => CmapFormat::Format8,
        10 => CmapFormat::Format10,
        12 => CmapFormat::Format12,
        13 => CmapFormat::Format13,
        _ => return None,
    };
    let priority = match (platform, encoding, format) {
        (3, 10, 12) => 0,
        // Formats 8 and 10 cover the full repertoire like format 12 but are
        // legacy Apple encodings, so they rank just below it.
        (3, 10, 8) => 1,
        (3, 10, 10) => 2,
        // Format 13 follows the same platform rules as format 12 but is a
        // many-to-one last-resort map, so it ranks below the one-to-one maps.
        (3, 10, 13) => 3,
        (0, _, 12) => 4,
        (0, _, 8) => 5,
        (0, _, 10) => 6,
        (0, _, 13) => 7,
        (3, 1, 4) => 8,
        (0, _, 4) => 9,
        // Format 2 is a legacy high-byte map; prefer the full BMP map 4.
        (3, 1, 2) => 10,
        (0, _, 2) => 11,
        (3, 1, 0) => 12,
        (1, 0, 0) => 13,
        (0, _, 0) => 14,
        (3, 1, 6) => 15,
        (1, 0, 6) => 16,
        (0, _, 6) => 17,
        // Microsoft Symbol (3, 0) subtables rank below every Unicode subtable;
        // they are used only when no Unicode mapping is available. Their
        // relative order is unchanged.
        (3, 0, 12) => 18,
        (3, 0, 13) => 19,
        (3, 0, 8) => 20,
        (3, 0, 10) => 21,
        (3, 0, 4) => 22,
        (3, 0, 0) => 23,
        (3, 0, 6) => 24,
        _ => return None,
    };
    Some((priority, supported_format))
}
