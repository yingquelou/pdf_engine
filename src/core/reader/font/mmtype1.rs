//! Multiple Master（MMType1）字体字典的读端建模，PDF 32000-1 §9.6.2.3。
//!
//! 规范依据（`docs/PDF32000_2008.pdf`，`pdftotext -raw` 抽取行号）：
//! - Table 110「Font types」列出 `/Subtype /MMType1`（约第 13807–13809 行），
//!   且与 `Type1`/`Type3`/`TrueType` 一样没有 `(PDF 1.x)` 版本标注——相对照，
//!   `Type0`、`CIDFontType0`、`CIDFontType2` 都标注了 `(PDF 1.2)`。因此
//!   MMType1 自 PDF 1.0 起可用，读端沿用 Type 1 的现有门禁，不加额外版本门。
//! - §9.6.2.3（约第 13961–14000 行）明确：MM 实例字典「may contain the same
//!   entries as a Type 1 font dictionary (see Table 111)」，差异只有两条：
//!   `/Subtype` shall 为 `MMType1`；实例 PostScript 名中的空格（20h）在
//!   `/BaseFont` 中 shall 换成下划线（5Fh）。**规范没有为 MM 定义任何专有字典
//!   条目**，因此本模块按 Table 111（约第 13845–13920 行）建模条目，不臆造键名。
//!   设计维度（weight/width 轴）、`/RS`、blend 机制属于字体程序，规范交由
//!   Adobe Technical Note #5015（PDF 32000-1 之外）——本 crate 未建模，待核。
//! - §9.6.2.3 还要求嵌入的 MM 实例程序「shall be an ordinary Type 1 font
//!   program, not a multiple master font program」（快照），Table 126（约第
//!   15531、15539 行）允许 `MMType1` 描述符使用 `/FontFile` 与
//!   `/FontFile3 /Type1C`。故本模块复用 Type 1 程序解析路径。
//! - Table 121 要求 Type 0 的 `/DescendantFonts` 指向 CIDFont 字典，MMType1
//!   不是合法的 composite descendant，因此 composite `/FD` 路径不识别 MMType1。
//!
//! 写端（把 MM 实例字典写出 PDF）不在本增量范围内。

use std::collections::BTreeMap;

use crate::core::types::{Document, Value};

use super::cff::RawCffFont;
use super::{
    EmbeddedFontProgram, FontDescriptorMetrics, FontError, FontProgramSource, FontResult,
    SimpleFontEncoding, Type1FontProgram, parse_descriptor,
};

/// Table 111 中已由 [`MultipleMasterFont`] 类型化建模的条目；其余条目原样保留，
/// 经 [`MultipleMasterFont::entry`] 查询。
const MODELLED_KEYS: [&str; 10] = [
    "Type",
    "Subtype",
    "BaseFont",
    "Name",
    "FirstChar",
    "LastChar",
    "Widths",
    "FontDescriptor",
    "Encoding",
    "ToUnicode",
];

/// `/Subtype /MMType1` 字体字典的解析结果（PDF 32000-1 §9.6.2.3 + Table 111）。
///
/// 条目集合与 Type 1 相同；差异只在识别用的 `/Subtype` 与 `/BaseFont` 的下划线
/// 命名约定。`/BaseFont` 始终按字面保存（规范未要求读端反演），需要还原空格时用
/// [`MultipleMasterFont::instance_name_with_spaces`]。
#[derive(Debug, Clone, PartialEq)]
pub struct MultipleMasterFont {
    /// 字体字典 `/BaseFont`（实例名，空格以 `_` 表示；原样保留）。
    pub base_font: String,
    /// 可选 `/Name`（Table 111；PDF 1.0 必需、之后可选且已废弃）。
    pub name: Option<String>,
    /// `/Widths` 首个字符码；MM 字典必需（见 [`Document::mmtype1_font`]）。
    pub first_char: u8,
    /// 文本空间单位下的字宽，自 `first_char` 起索引。
    pub widths: Vec<f64>,
    /// 已解析的 `/FontDescriptor` 度量。
    pub descriptor: FontDescriptorMetrics,
    /// `/Encoding` 解析结果（Table 111 可选）。
    ///
    /// 条目缺失时给出 `/BaseFont` 隐含的编码（StandardEncoding，或
    /// Symbol/ZapfDingbats），与 [`Document::simple_font_encoding`] 一致。
    pub encoding: Option<SimpleFontEncoding>,
    /// 是否提供 `/ToUnicode` 流（Table 111 可选；PDF 1.2）。
    ///
    /// CMap 内容由文本提取经 `Document::simple_font_to_unicode` 解析。
    pub has_to_unicode: bool,
    /// Table 111 未建模的条目：原样保存（间接引用不解引用）。
    other_entries: BTreeMap<String, Value>,
}

impl MultipleMasterFont {
    /// 返回字符码的字宽；`/Widths` 覆盖范围之外时取描述符的 `/MissingWidth`。
    ///
    /// 与 [`super::Type1Font::width_for`] 一致：`code` 小于 `/FirstChar` 或超出
    /// 数组长度都视为「不在宽度数组内」，回退 `/MissingWidth`。
    pub fn width_for(&self, code: u8) -> Option<f64> {
        if let Some(index) = usize::from(code).checked_sub(usize::from(self.first_char))
            && let Some(width) = self.widths.get(index)
        {
            return Some(*width);
        }
        self.descriptor.missing_width
    }

    /// Table 111 未建模条目的原值，例如生产器写入的私有键。
    pub fn entry(&self, key: &str) -> Option<&Value> {
        self.other_entries.get(key)
    }

    /// Table 111 未建模条目的原值集合（按键升序）。
    pub fn entries(&self) -> &BTreeMap<String, Value> {
        &self.other_entries
    }

    /// 按 §9.6.2.3 的命名约定把 `/BaseFont` 中的 `_`（5Fh）还原为空格（20h）。
    ///
    /// 规范只规定了写入方向（实例名中的空格须以下划线出现在 `/BaseFont`），没有
    /// 规定读端反演，且 `/BaseFont` 本身可以合法含下划线——因此本方法是供按外部
    /// 字体名匹配实例的调用方使用的便捷视图，[`Self::base_font`] 始终是字面值。
    pub fn instance_name_with_spaces(&self) -> String {
        self.base_font.replace('_', " ")
    }
}

impl Document {
    /// 解析 `/Subtype /MMType1` 字体字典（PDF 32000-1 §9.6.2.3）。
    ///
    /// 非 MMType1 字典返回 `Ok(None)`，`/Type1`、`/TrueType` 等既有路径的行为不
    /// 受影响。字典按 Table 111 校验：`/BaseFont` 必需；`/FirstChar`、`/LastChar`
    /// 与 `/Widths` 必需且长度须等于 `LastChar - FirstChar + 1`；
    /// `/FontDescriptor` 必需。
    ///
    /// 关于标准 14 字体的豁免：Table 111 把「standard 14 fonts」列为 Type 1 字体
    /// 名单（§9.6.2.2），MMType1 字典不在其列，因此本方法不套用该豁免——缺
    /// `/Widths` 或 `/FontDescriptor` 的 MM 字典按类型化错误
    /// [`FontError::Invalid`] 报告，而不是回退内置度量。
    ///
    /// 缺条目、类型不符、悬空引用一律返回类型化错误；不 panic。
    pub fn mmtype1_font(&self, value: &Value) -> FontResult<Option<MultipleMasterFont>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("MMType1") {
            return Ok(None);
        }

        let base_font = dict
            .get("BaseFont")
            .and_then(Value::as_name)
            .ok_or_else(|| FontError::Invalid("missing or invalid /BaseFont".to_string()))?
            .to_string();

        let (first_char, widths) = self.read_widths(dict)?;
        let first_char = first_char.ok_or_else(|| {
            FontError::Invalid(
                "MMType1 fonts require /FirstChar, /LastChar, and /Widths".to_string(),
            )
        })?;

        let name = match dict.get("Name") {
            Some(value) => {
                let value = self.resolve_value(value)?;
                Some(
                    value
                        .as_name()
                        .ok_or_else(|| {
                            FontError::Invalid("/Name in MMType1 font is not a name".to_string())
                        })?
                        .to_string(),
                )
            }
            None => None,
        };

        let descriptor_value = dict.get("FontDescriptor").ok_or_else(|| {
            FontError::Invalid("MMType1 fonts require /FontDescriptor".to_string())
        })?;
        let descriptor = self.resolve_value(descriptor_value)?;
        let descriptor = descriptor
            .as_dict()
            .ok_or_else(|| FontError::Invalid("/FontDescriptor is not a dictionary".to_string()))?;
        let descriptor = parse_descriptor(self, descriptor)?;

        let has_to_unicode = match dict.get("ToUnicode") {
            Some(value) => {
                let value = self.resolve_value(value)?;
                if !matches!(value, Value::Stream(_, _)) {
                    return Err(FontError::Invalid(
                        "/ToUnicode in MMType1 font is not a stream".to_string(),
                    ));
                }
                true
            }
            None => false,
        };

        let encoding = self.simple_font_encoding(&value)?;

        let other_entries = dict
            .0
            .iter()
            .filter(|(key, _)| !MODELLED_KEYS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        Ok(Some(MultipleMasterFont {
            base_font,
            name,
            first_char,
            widths,
            descriptor,
            encoding,
            has_to_unicode,
            other_entries,
        }))
    }

    /// 解析 MMType1 字体描述符中的嵌入程序快照。
    ///
    /// Table 126 允许 `MMType1` 描述符使用 `/FontFile`（Type 1 程序）或
    /// `/FontFile3` `/Subtype /Type1C`（CFF 表示），两者都返回解码后的字节，
    /// 由 `source` 与 `subtype` 区分。字体无描述符或描述符无 `/FontFile*` 时
    /// 返回 `Ok(None)`；非 MMType1 字典返回 `Ok(None)`。
    pub fn mmtype1_font_program(&self, value: &Value) -> FontResult<Option<EmbeddedFontProgram>> {
        self.font_program(
            value,
            "MMType1",
            &[FontProgramSource::FontFile, FontProgramSource::FontFile3],
            None,
        )
    }

    /// 解析 MMType1 快照的 `/FontFile` Type 1 程序。
    ///
    /// §9.6.2.3 要求嵌入的 MM 实例程序是普通 Type 1 程序，故直接复用
    /// [`Type1FontProgram`] 解析。`/FontFile3` 流不是 Type 1 程序，按类型化错误
    /// 报告（改用 [`Document::mmtype1_type1c_program`]）。
    pub fn mmtype1_type1_program(&self, value: &Value) -> FontResult<Option<Type1FontProgram>> {
        let Some(program) = self.mmtype1_font_program(value)? else {
            return Ok(None);
        };
        if program.source != FontProgramSource::FontFile {
            return Err(FontError::Invalid(
                "MMType1 snapshot programs must use /FontFile; /FontFile3 is not parsed as Type 1"
                    .to_string(),
            ));
        }
        Type1FontProgram::parse(&program.data).map(Some)
    }

    /// 解析 MMType1 快照的 `/FontFile3` `/Subtype /Type1C` CFF 程序。
    ///
    /// 只接受非 CID-keyed 的 name-keyed CFF（Table 126 的 `/Type1C`）；其他流
    /// `/Subtype`、CID-keyed 程序、畸形程序都返回类型化错误。字体无嵌入程序或
    /// 使用 `/FontFile` 时返回 `Ok(None)`。
    pub fn mmtype1_type1c_program(&self, value: &Value) -> FontResult<Option<RawCffFont>> {
        let Some(program) = self.mmtype1_font_program(value)? else {
            return Ok(None);
        };
        if program.source != FontProgramSource::FontFile3 {
            return Ok(None);
        }
        if program.subtype.as_deref() != Some("Type1C") {
            return Err(FontError::Invalid(format!(
                "/Subtype /MMType1 /FontFile3 stream requires /Subtype /Type1C, found /{}",
                program.subtype.as_deref().unwrap_or_default()
            )));
        }
        let font = RawCffFont::parse(program.data)?;
        if font.is_cid_keyed() {
            return Err(FontError::Invalid(
                "MMType1 /FontFile3 /Type1C program is CID-keyed; use a CIDFontType0 font for /CIDFontType0C".into(),
            ));
        }
        Ok(Some(font))
    }
}
