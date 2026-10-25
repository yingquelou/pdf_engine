//! CFF1 structure parsing: INDEX, DICT, charset, FDSelect and width helpers.

use std::ops::Range;

use crate::core::reader::font::{FontError, FontResult};

/// Top DICT `FontBBox` operator.
const OP_FONT_BBOX: u16 = 5;
/// Top DICT `charset` offset operator.
const OP_CHARSET: u16 = 15;
/// Top DICT `CharStrings` offset operator.
const OP_CHAR_STRINGS: u16 = 17;
/// Top DICT `Private` (size, offset) operator.
const OP_PRIVATE: u16 = 18;
/// Top DICT `CharstringType` operator (12 6).
const OP_CHARSTRING_TYPE: u16 = 0x0C06;
/// Top DICT `FontMatrix` operator (12 7).
const OP_FONT_MATRIX: u16 = 0x0C07;
/// Top DICT `ROS` operator (12 30).
const OP_ROS: u16 = 0x0C1E;
/// Top DICT `CIDCount` operator (12 34).
const OP_CID_COUNT: u16 = 0x0C22;
/// Top DICT `FDArray` offset operator (12 36).
const OP_FD_ARRAY: u16 = 0x0C24;
/// Top DICT `FDSelect` offset operator (12 37).
const OP_FD_SELECT: u16 = 0x0C25;
/// Private DICT `defaultWidthX` operator.
const OP_DEFAULT_WIDTH: u16 = 20;
/// Private DICT `nominalWidthX` operator.
const OP_NOMINAL_WIDTH: u16 = 21;

/// `defaultWidthX` / `nominalWidthX` values from one Private DICT.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct WidthDefaults {
    pub(crate) default_width: f64,
    pub(crate) nominal_width: f64,
}

/// Parse an INDEX starting at `offset`.
///
/// Returns the object ranges (absolute offsets into `data`) and the offset of
/// the first byte after the INDEX.
pub(crate) fn parse_index(data: &[u8], offset: usize) -> FontResult<CffIndex> {
    let count = usize::from(read_u16(data, offset)?);
    if count == 0 {
        return Ok(CffIndex {
            entries: Vec::new(),
            end: offset + 2,
        });
    }
    let off_size = usize::from(read_u8(data, offset + 2)?);
    if !(1..=4).contains(&off_size) {
        return Err(FontError::Invalid(format!(
            "invalid CFF INDEX offset size {off_size} at offset {offset}"
        )));
    }

    let offsets_start = offset + 3;
    let mut offsets = Vec::with_capacity(count + 1);
    for index in 0..=count {
        let start = offsets_start + index * off_size;
        let mut value = 0usize;
        for byte in 0..off_size {
            value = (value << 8) | usize::from(read_u8(data, start + byte)?);
        }
        offsets.push(value);
    }
    if offsets[0] != 1 {
        return Err(FontError::Invalid(format!(
            "invalid CFF INDEX first offset {} at offset {offset}",
            offsets[0]
        )));
    }

    let data_start = offsets_start + (count + 1) * off_size;
    let mut entries = Vec::with_capacity(count);
    for pair in offsets.windows(2) {
        if pair[1] < pair[0] {
            return Err(FontError::Invalid(format!(
                "non-monotonic CFF INDEX offsets at offset {offset}"
            )));
        }
        let start = data_start + (pair[0] - 1);
        let end = data_start + (pair[1] - 1);
        if end > data.len() {
            return Err(truncated("CFF INDEX data", end));
        }
        entries.push(start..end);
    }
    Ok(CffIndex {
        entries,
        end: data_start + (offsets[count] - 1),
    })
}

pub(crate) struct CffIndex {
    pub(crate) entries: Vec<Range<usize>>,
    pub(crate) end: usize,
}

/// A DICT operator together with the operands that preceded it.
struct DictEntry {
    operator: u16,
    operands: Vec<f64>,
}

fn parse_dict(data: &[u8]) -> FontResult<Vec<DictEntry>> {
    let mut entries = Vec::new();
    let mut operands = Vec::new();
    let mut position = 0;
    while position < data.len() {
        let first = data[position];
        if first <= 21 {
            position += 1;
            let operator = if first == 12 {
                let second = read_u8(data, position)?;
                position += 1;
                0x0C00 | u16::from(second)
            } else {
                u16::from(first)
            };
            entries.push(DictEntry {
                operator,
                operands: std::mem::take(&mut operands),
            });
            continue;
        }
        operands.push(dict_number(data, &mut position)?);
        if operands.len() > 48 {
            return Err(FontError::Invalid(
                "CFF DICT has more than 48 operands".into(),
            ));
        }
    }
    if !operands.is_empty() {
        return Err(FontError::Invalid(
            "CFF DICT ends with operands but no operator".into(),
        ));
    }
    Ok(entries)
}

fn last_operands(entries: &[DictEntry], operator: u16) -> Option<&[f64]> {
    entries
        .iter()
        .rev()
        .find(|entry| entry.operator == operator)
        .map(|entry| entry.operands.as_slice())
}

pub(crate) fn dict_number(data: &[u8], position: &mut usize) -> FontResult<f64> {
    let start = *position;
    let first = read_u8(data, start)?;
    *position += 1;
    match first {
        28 => {
            let value = read_i16(data, *position)?;
            *position += 2;
            Ok(f64::from(value))
        }
        29 => {
            let value = read_i32(data, *position)?;
            *position += 4;
            Ok(f64::from(value))
        }
        30 => parse_real(data, position),
        32..=246 => Ok(f64::from(first) - 139.0),
        247..=250 => {
            let second = read_u8(data, *position)?;
            *position += 1;
            Ok((f64::from(first) - 247.0) * 256.0 + f64::from(second) + 108.0)
        }
        251..=254 => {
            let second = read_u8(data, *position)?;
            *position += 1;
            Ok(-((f64::from(first) - 251.0) * 256.0) - f64::from(second) - 108.0)
        }
        other => Err(FontError::Invalid(format!(
            "invalid CFF DICT number byte 0x{other:02X} at offset {start}"
        ))),
    }
}

/// Parse the nibble-encoded real number introduced by the `30` byte.
fn parse_real(data: &[u8], position: &mut usize) -> FontResult<f64> {
    let start = *position;
    let mut text = String::new();
    let mut finished = false;
    while !finished {
        let byte = read_u8(data, *position)?;
        *position += 1;
        for nibble in [byte >> 4, byte & 0x0F] {
            match nibble {
                0x0..=0x9 => text.push(char::from(b'0' + nibble)),
                0xA => text.push('.'),
                0xB => text.push('E'),
                0xC => text.push_str("E-"),
                0xD => {
                    return Err(FontError::Invalid(format!(
                        "reserved nibble in CFF real number at offset {start}"
                    )));
                }
                0xE => text.push('-'),
                _ => {
                    finished = true;
                    break;
                }
            }
            if text.len() > 64 {
                return Err(FontError::Invalid(format!(
                    "CFF real number at offset {start} is too long"
                )));
            }
        }
    }
    if text.is_empty() {
        return Err(FontError::Invalid(format!(
            "empty CFF real number at offset {start}"
        )));
    }
    text.parse::<f64>().map_err(|_| {
        FontError::Invalid(format!(
            "invalid CFF real number {text:?} at offset {start}"
        ))
    })
}

/// The Top DICT operators that matter for bare CFF parsing.
pub(crate) struct TopDict {
    pub(crate) charset: Option<usize>,
    pub(crate) char_strings: Option<usize>,
    /// `(size, offset)` from the `Private` operator.
    pub(crate) private: Option<(usize, usize)>,
    pub(crate) ros: Option<[u16; 3]>,
    pub(crate) fd_array: Option<usize>,
    pub(crate) fd_select: Option<usize>,
    pub(crate) cid_count: Option<u16>,
    pub(crate) font_matrix: Option<[f64; 6]>,
    pub(crate) font_bbox: Option<[f64; 4]>,
}

pub(crate) fn parse_top_dict(data: &[u8]) -> FontResult<TopDict> {
    let entries = parse_dict(data)?;

    if let Some(operands) = last_operands(&entries, OP_CHARSTRING_TYPE) {
        let value = single_number(operands, "CharstringType")?;
        if value != 2.0 {
            return Err(FontError::Invalid(format!(
                "CFF CharstringType {value} is not supported; only Type 2 charstrings are"
            )));
        }
    }

    Ok(TopDict {
        charset: single_offset(last_operands(&entries, OP_CHARSET), "charset")?,
        char_strings: single_offset(last_operands(&entries, OP_CHAR_STRINGS), "CharStrings")?,
        private: private_range(last_operands(&entries, OP_PRIVATE))?,
        ros: ros_operands(last_operands(&entries, OP_ROS))?,
        fd_array: single_offset(last_operands(&entries, OP_FD_ARRAY), "FDArray")?,
        fd_select: single_offset(last_operands(&entries, OP_FD_SELECT), "FDSelect")?,
        cid_count: match last_operands(&entries, OP_CID_COUNT) {
            Some(operands) => {
                let value = single_number(operands, "CIDCount")?;
                Some(
                    u16::try_from(integer_operand(value, "CIDCount")?).map_err(|_| {
                        FontError::Invalid(format!("CFF CIDCount {value} is out of range"))
                    })?,
                )
            }
            None => None,
        },
        font_matrix: match last_operands(&entries, OP_FONT_MATRIX) {
            Some(operands) => Some(fixed_operands::<6>(operands, "FontMatrix")?),
            None => None,
        },
        font_bbox: match last_operands(&entries, OP_FONT_BBOX) {
            Some(operands) => Some(fixed_operands::<4>(operands, "FontBBox")?),
            None => None,
        },
    })
}

/// The Font DICT entries that matter inside the FDArray INDEX.
pub(crate) struct FontDict {
    pub(crate) private: Option<(usize, usize)>,
}

pub(crate) fn parse_font_dict(data: &[u8]) -> FontResult<FontDict> {
    let entries = parse_dict(data)?;
    Ok(FontDict {
        private: private_range(last_operands(&entries, OP_PRIVATE))?,
    })
}

pub(crate) fn parse_private_widths(
    data: &[u8],
    offset: usize,
    size: usize,
) -> FontResult<WidthDefaults> {
    let end = offset
        .checked_add(size)
        .ok_or_else(|| FontError::Invalid("CFF Private DICT range overflows".into()))?;
    let private = data.get(offset..end).ok_or_else(|| {
        FontError::Invalid(format!(
            "CFF Private DICT range {offset}..{end} is outside the program"
        ))
    })?;
    let entries = parse_dict(private)?;
    let mut widths = WidthDefaults::default();
    if let Some(operands) = last_operands(&entries, OP_DEFAULT_WIDTH) {
        widths.default_width = single_number(operands, "defaultWidthX")?;
    }
    if let Some(operands) = last_operands(&entries, OP_NOMINAL_WIDTH) {
        widths.nominal_width = single_number(operands, "nominalWidthX")?;
    }
    Ok(widths)
}

/// Parse a charset into a GID -> CID (name-keyed: GID -> SID) table.
pub(crate) fn parse_charset(data: &[u8], offset: usize, glyph_count: u16) -> FontResult<Vec<u16>> {
    let count = usize::from(glyph_count);
    let mut charset = Vec::with_capacity(count);
    charset.push(0);

    let format = read_u8(data, offset)?;
    let mut position = offset + 1;
    match format {
        0 => {
            while charset.len() < count {
                charset.push(read_u16(data, position)?);
                position += 2;
            }
        }
        1 | 2 => {
            while charset.len() < count {
                let first = read_u16(data, position)?;
                let left = if format == 1 {
                    let left = u32::from(read_u8(data, position + 2)?);
                    position += 3;
                    left
                } else {
                    let left = u32::from(read_u16(data, position + 2)?);
                    position += 4;
                    left
                };
                let last = u32::from(first) + left;
                if last > u32::from(u16::MAX) {
                    return Err(FontError::Invalid(format!(
                        "CFF charset range {first}..={last} exceeds the CID range"
                    )));
                }
                for value in u32::from(first)..=last {
                    if charset.len() == count {
                        return Err(FontError::Invalid(
                            "CFF charset ranges cover more glyphs than the CharStrings INDEX"
                                .into(),
                        ));
                    }
                    charset.push(value as u16);
                }
            }
        }
        other => {
            return Err(FontError::Invalid(format!(
                "unsupported CFF charset format {other} at offset {offset}"
            )));
        }
    }
    Ok(charset)
}

/// Parse an FDSelect table into a GID -> Font DICT index table.
pub(crate) fn parse_fd_select(data: &[u8], offset: usize, glyph_count: u16) -> FontResult<Vec<u8>> {
    let format = read_u8(data, offset)?;
    let count = usize::from(glyph_count);
    match format {
        0 => {
            let mut select = Vec::with_capacity(count);
            for index in 0..count {
                select.push(read_u8(data, offset + 1 + index)?);
            }
            Ok(select)
        }
        3 => {
            let mut position = offset + 1;
            let range_count = usize::from(read_u16(data, position)?);
            position += 2;
            if range_count == 0 {
                return Err(FontError::Invalid(
                    "CFF FDSelect format 3 has no ranges".into(),
                ));
            }
            let mut ranges = Vec::with_capacity(range_count);
            for _ in 0..range_count {
                let first = read_u16(data, position)?;
                let font_dict = read_u8(data, position + 2)?;
                position += 3;
                ranges.push((first, font_dict));
            }
            let sentinel = read_u16(data, position)?;
            if ranges[0].0 != 0 {
                return Err(FontError::Invalid(
                    "CFF FDSelect format 3 does not start at glyph 0".into(),
                ));
            }
            if let Some(pair) = ranges.windows(2).find(|pair| pair[1].0 <= pair[0].0) {
                return Err(FontError::Invalid(format!(
                    "CFF FDSelect format 3 ranges are not ascending at glyph {}",
                    pair[1].0
                )));
            }
            if sentinel != glyph_count {
                return Err(FontError::Invalid(format!(
                    "CFF FDSelect sentinel {sentinel} does not match the glyph count {glyph_count}"
                )));
            }
            if ranges
                .last()
                .is_some_and(|&(first, _)| usize::from(first) >= count)
            {
                return Err(FontError::Invalid(
                    "CFF FDSelect format 3 range starts past the glyph count".into(),
                ));
            }
            let mut select = vec![0u8; count];
            for (index, &(first, font_dict)) in ranges.iter().enumerate() {
                let end = ranges
                    .get(index + 1)
                    .map_or(count, |next| usize::from(next.0));
                select[usize::from(first)..end].fill(font_dict);
            }
            Ok(select)
        }
        other => Err(FontError::Invalid(format!(
            "unsupported CFF FDSelect format {other} at offset {offset}"
        ))),
    }
}

fn single_offset(operands: Option<&[f64]>, name: &str) -> FontResult<Option<usize>> {
    match operands {
        Some(operands) => {
            let value = single_number(operands, name)?;
            Ok(Some(integer_operand(value, name)?))
        }
        None => Ok(None),
    }
}

fn single_number(operands: &[f64], name: &str) -> FontResult<f64> {
    match operands {
        [value] => Ok(*value),
        _ => Err(FontError::Invalid(format!(
            "CFF {name} requires exactly one operand, found {}",
            operands.len()
        ))),
    }
}

fn integer_operand(value: f64, name: &str) -> FontResult<usize> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) {
        return Err(FontError::Invalid(format!(
            "CFF {name} operand {value} is not a non-negative 32-bit integer"
        )));
    }
    Ok(value as usize)
}

fn fixed_operands<const N: usize>(operands: &[f64], name: &str) -> FontResult<[f64; N]> {
    operands.try_into().map_err(|_| {
        FontError::Invalid(format!(
            "CFF {name} requires {N} operands, found {}",
            operands.len()
        ))
    })
}

fn private_range(operands: Option<&[f64]>) -> FontResult<Option<(usize, usize)>> {
    match operands {
        Some([size, offset]) => Ok(Some((
            integer_operand(*size, "Private size")?,
            integer_operand(*offset, "Private offset")?,
        ))),
        Some(operands) => Err(FontError::Invalid(format!(
            "CFF Private requires 2 operands (size, offset), found {}",
            operands.len()
        ))),
        None => Ok(None),
    }
}

fn ros_operands(operands: Option<&[f64]>) -> FontResult<Option<[u16; 3]>> {
    match operands {
        Some([registry, ordering, supplement]) => {
            let mut values = [0u16; 3];
            for (slot, (value, name)) in values.iter_mut().zip([
                (registry, "ROS registry SID"),
                (ordering, "ROS ordering SID"),
                (supplement, "ROS supplement"),
            ]) {
                *slot = u16::try_from(integer_operand(*value, name)?).map_err(|_| {
                    FontError::Invalid(format!("CFF {name} {value} is out of range"))
                })?;
            }
            Ok(Some(values))
        }
        Some(operands) => Err(FontError::Invalid(format!(
            "CFF ROS requires 3 operands, found {}",
            operands.len()
        ))),
        None => Ok(None),
    }
}

pub(crate) fn read_u8(data: &[u8], position: usize) -> FontResult<u8> {
    data.get(position)
        .copied()
        .ok_or_else(|| truncated("byte", position))
}

fn read_u16(data: &[u8], position: usize) -> FontResult<u16> {
    let bytes = data
        .get(position..position + 2)
        .ok_or_else(|| truncated("u16", position))?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

pub(crate) fn read_i16(data: &[u8], position: usize) -> FontResult<i16> {
    Ok(read_u16(data, position)? as i16)
}

pub(crate) fn read_i32(data: &[u8], position: usize) -> FontResult<i32> {
    let bytes = data
        .get(position..position + 4)
        .ok_or_else(|| truncated("i32", position))?;
    Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub(crate) fn truncated(what: &str, position: usize) -> FontError {
    FontError::Invalid(format!(
        "truncated CFF program: cannot read {what} at offset {position}"
    ))
}
