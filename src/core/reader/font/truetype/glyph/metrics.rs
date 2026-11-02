use crate::core::reader::font::FontError;

use super::error::invalid_glyph;

pub(super) fn read_loca_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_glyph("loca table is truncated"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn read_glyph_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_glyph("glyph data is truncated"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn read_glyph_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_glyph("composite transform is truncated"))?;
    Ok(i16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn read_i8(data: &[u8], offset: usize) -> Result<i8, FontError> {
    let value = *data
        .get(offset)
        .ok_or_else(|| invalid_glyph("composite component arguments are truncated"))?;
    Ok(i8::from_ne_bytes([value]))
}

pub(super) fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_glyph("loca table is truncated"))?;
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn read_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_glyph("glyph data is truncated"))?;
    Ok(i16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}
