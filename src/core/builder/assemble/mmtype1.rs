//! Multiple Master（MMType1）实例字典的写端门禁（PDF 32000-1 §9.6.2.3）。
//!
//! 规范依据（`docs/PDF32000_2008.pdf`，`pdftotext -raw` 抽取的 raw 行号）：
//! - Table 110「Font types」（raw 13801–13810）把 `MMType1`（raw 13807–13809）
//!   与 `Type1`/`Type3`/`TrueType` 并列，且**没有** `(PDF 1.x)` 标注——对照同表
//!   `Type0` 明确标注 `(PDF 1.2)`（raw 13803）。故 MMType1 自 PDF 1.0 起可用，
//!   写端不登记新的 `PdfFeature` 门禁，沿用 Type 1 的既有版本处理。
//! - §9.6.2.3（raw 13972–13977）给出 MM 实例字典相对 Type 1 字体字典
//!   （Table 111）的**全部**差异，只有两条：`/Subtype` shall 为 `MMType1`
//!   （raw 13974）；实例 PostScript 名含空格（20h）时 shall 在 `/BaseFont`
//!   里换成 LOW LINE（下划线，5Fh）（raw 13975–13977，规范 EXAMPLE 2 把
//!   `"MinionMM 366 465 11 "` 写成 `/MinionMM_366_465_11_`，raw 13981）。
//!   规范没有为 MM 定义任何专有字体字典条目：设计轴（weight/width）、`/RS`、
//!   blend 机制属 Adobe TN 5015（§9.6.2.3 NOTE，raw 13969），本模块因此
//!   **不写任何 MM 私有键**。
//! - §9.6.2.3（raw 14000–14002）要求 MM 实例的嵌入程序 shall 是「an ordinary
//!   Type 1 font program, not a multiple master font program」（快照）；
//!   Table 126（raw 15527 起）允许 `MMType1` 的只有 `/FontFile`
//!   （raw 15529–15531）与 `/FontFile3` `/Subtype /Type1C`
//!   （raw 15536–15540）。`/FontFile2` 只列 TrueType/CIDFontType2
//!   （raw 15532–15535），`/FontFile3 /Subtype /OpenType` 的续表只列
//!   `Type1` 与 `CIDFontType0`（raw 15569–15572），都不含 MMType1。
//!   流字典的 `/Length1/2/3` 与 `/Subtype /Type1C` 写法复用 `super::fonts`
//!   的既有 Type 1 程序通路，不另行推导。
//! - Table 111 把 `/FirstChar`、`/LastChar`、`/Widths`、`/FontDescriptor`
//!   标注为「Required except for the standard 14 fonts」（raw 13862、13869、
//!   13876、13891），而 §9.6.2.2 的名单是「The PostScript names of 14 Type 1
//!   fonts」（raw 13953–13956）——MM 实例不在其列，因此本模块不套用豁免：
//!   缺 `/Widths` 或缺 `/FontDescriptor` 在构建期就报
//!   [`WriteError::InvalidObject`]，而不是写出读端
//!   [`Document::mmtype1_font`](crate::core::types::Document::mmtype1_font)
//!   会报 `FontError::Invalid` 的文件。
//! - Table 121（raw 15097–15098）把 Type 0 的 `/DescendantFonts` 限定为
//!   CIDFont 字典，MMType1 不是合法的 composite 后代，故 MM 实例只经简单字体
//!   通路写出，不会被 CID 装配认领。
//!
//! [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject

use crate::core::builder::{Font, FontProgram};
use crate::core::writer::WriteError;

/// MM 实例字典的 `/Subtype` 值（§9.6.2.3，raw 13974）。
pub(super) const SUBTYPE: &str = "MMType1";

/// 按 §9.6.2.3 的命名规则（raw 13975–13977）把 MM 实例 PostScript 名中的
/// 空格（20h）换成下划线（5Fh），作为 `/BaseFont` 的字面值。
///
/// 只替换空格，其余字符按字面写出：与读端
/// [`MultipleMasterFont::instance_name_with_spaces`](crate::core::reader::font::MultipleMasterFont::instance_name_with_spaces)
/// 的 `_`→空格视图互逆，因此 `/BaseFont` 本身仍可能合法含下划线。
pub(super) fn base_font_for(instance_name: &str) -> String {
    instance_name.replace(' ', "_")
}

/// MM 实例是否嵌入 Table 126 许可的程序格式。
///
/// `None`（不嵌入，实例来自 conforming reader 环境）、[`FontProgram::Type1`]
/// （`/FontFile`）与 [`FontProgram::CFF`]（`/FontFile3 /Type1C`）合法；
/// [`FontProgram::TrueType`] 与 [`FontProgram::OpenType`] 会被写成 MMType1
/// 描述符不使用的 `/FontFile2` / `/FontFile3 /Subtype /OpenType`，故拒绝。
///
/// [`FontProgram::Type1`]: crate::core::builder::FontProgram::Type1
/// [`FontProgram::CFF`]: crate::core::builder::FontProgram::CFF
/// [`FontProgram::TrueType`]: crate::core::builder::FontProgram::TrueType
/// [`FontProgram::OpenType`]: crate::core::builder::FontProgram::OpenType
pub(super) fn check_program_format(font: &Font) -> Result<(), WriteError> {
    if !font.mmtype1 {
        return Ok(());
    }
    match &font.font_program {
        Some(FontProgram::TrueType(_)) => Err(WriteError::InvalidObject(format!(
            "/Subtype /{SUBTYPE} instance {} cannot embed a TrueType program: \
             Table 126 lists /FontFile2 only for /TrueType and /CIDFontType2 \
             dictionaries, and 9.6.2.3 requires the embedded MM program to be \
             an ordinary Type 1 program (use FontProgram::Type1 or FontProgram::CFF)",
            font.name
        ))),
        Some(FontProgram::OpenType(_)) => Err(WriteError::InvalidObject(format!(
            "/Subtype /{SUBTYPE} instance {} cannot embed an OpenType program: \
             Table 126's /FontFile3 /Subtype /OpenType row lists /Type1 and \
             /CIDFontType0 only, not /MMType1 (use FontProgram::Type1 or \
             FontProgram::CFF for a Type 1 snapshot)",
            font.name
        ))),
        _ => Ok(()),
    }
}

/// MM 实例是否可以省略 Table 111 的 `/Widths`（连同 `/FirstChar`/`/LastChar`）。
///
/// `base_font` 是准备写出的 `/BaseFont` 字面值，用于判断标准 14 豁免；MM 实例
/// 一律不豁免（raw 13953–13956 的名单只含 Type 1 字体），因此本函数对
/// [`Font::mmtype1`] 总是返回 `false`。
pub(super) fn may_omit_widths(font: &Font, base_font: &str) -> bool {
    !font.mmtype1 && super::fonts::is_standard_14(base_font)
}

/// 缺 `/Widths` 的 MM 实例字典的构建期错误（读端会报 `FontError::Invalid`）。
pub(super) fn missing_widths(font: &Font) -> WriteError {
    WriteError::InvalidObject(format!(
        "/Subtype /{SUBTYPE} instance {} requires /FirstChar, /LastChar and /Widths: \
         Table 111 exempts only the standard 14 /Type1 fonts (9.6.2.2), so that \
         exemption does not cover a multiple master instance; call Font::with_widths",
        font.name
    ))
}

/// MM 实例是否产出 `/FontDescriptor`（Table 111 必需，MM 不套用标准 14 豁免）。
///
/// `writes_descriptor` 表示本次装配会写出描述符对象（显式给出
/// [`Font::descriptor`]，或嵌入快照程序时按 Table 122 补默认值写出）。
pub(super) fn check_descriptor(font: &Font, writes_descriptor: bool) -> Result<(), WriteError> {
    if !font.mmtype1 || writes_descriptor {
        return Ok(());
    }
    Err(WriteError::InvalidObject(format!(
        "/Subtype /{SUBTYPE} instance {} requires a /FontDescriptor (Table 111; \
         the standard-14 exemption of 9.6.2.2 covers /Type1 fonts only); \
         call Font::with_descriptor or embed a Type 1 snapshot with Font::with_font_program",
        font.name
    )))
}
