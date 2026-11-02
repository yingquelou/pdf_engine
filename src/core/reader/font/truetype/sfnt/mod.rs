//! SFNT 字体程序解析：`TrueTypeFont` 持有已校验的表目录与派生数据。
//!
//! 主类型与共享错误构造保留在本模块；实现按职责拆分：
//! - `directory`：SFNT 签名、表目录读取与边界校验、表原始字节访问；
//! - `metrics`：`head`/`maxp`/`hhea`/`hmtx`/`vhea`/`vmtx` 派生的量度查询；
//! - `encoding`：`cmap` 派生的码位↔字形映射与内置单字节编码；
//! - `outline`：`loca`/`glyf` 字形轮廓访问。

use std::{collections::BTreeMap, ops::Range};

use super::{super::FontError, cmap::Cmap, glyph::GlyphLocations, tables::TrueTypeMetadata};

mod directory;
mod encoding;
mod metrics;
mod outline;

#[cfg(test)]
mod tests;

/// Recognized SFNT signatures for embedded font programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfntFlavor {
    /// The standard TrueType version 1 signature (`0x00010000`).
    TrueType,
    /// Apple's legacy TrueType signature (`true`).
    AppleTrue,
    /// The OpenType signature with CFF outlines (`OTTO`).
    ///
    /// A CFF-flavored program stores outlines in a `CFF ` table and normally
    /// has no `loca`/`glyf` tables, so glyph outlines are unavailable.
    Cff,
}

/// A decoded TrueType or OpenType font with a validated SFNT table directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrueTypeFont {
    data: Vec<u8>,
    flavor: SfntFlavor,
    tables: BTreeMap<[u8; 4], Range<usize>>,
    metadata: TrueTypeMetadata,
    cmap: Cmap,
    /// Present only when the program has both `loca` and `glyf` tables.
    glyph_locations: Option<GlyphLocations>,
}

fn invalid_sfnt(message: &str) -> FontError {
    FontError::Invalid(format!("invalid TrueType SFNT font: {message}"))
}
