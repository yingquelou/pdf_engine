//! `cmap` 子模块共享的大端读取、偏移溢出辅助与错误构造。

use crate::core::reader::font::FontError;

pub(super) fn read_u8(data: &[u8], offset: usize) -> Result<u8, FontError> {
    data.get(offset)
        .copied()
        .ok_or_else(|| invalid_cmap("unexpected end of cmap data"))
}

pub(super) fn read_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let bytes = data
        .get(offset..checked_add(offset, 2)?)
        .ok_or_else(|| invalid_cmap("unexpected end of cmap data"))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let bytes = data
        .get(offset..checked_add(offset, 4)?)
        .ok_or_else(|| invalid_cmap("unexpected end of cmap data"))?;
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("fixed-width slice"),
    ))
}

pub(super) fn checked_add(offset: usize, length: usize) -> Result<usize, FontError> {
    offset
        .checked_add(length)
        .ok_or_else(|| invalid_cmap("cmap offset overflows"))
}

pub(super) fn invalid_cmap(message: &str) -> FontError {
    FontError::Invalid(format!("invalid TrueType cmap: {message}"))
}
