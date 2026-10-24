//! CIDFont、CID 编码与宽度条目。

use super::{FontDescriptor, FontProgram};
use crate::core::types::{Array, Dict, Value};
use std::collections::BTreeMap;

/// CID 字符集合信息（`/CIDSystemInfo`）。
///
/// 标识 CIDFont 的字符集合：注册机构、排序与补充版本号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidSystemInfo {
    /// `/Registry`（如 `"Adobe"`）。
    pub registry: String,
    /// `/Ordering`（如 `"Identity"`、`"GB1"`）。
    pub ordering: String,
    /// `/Supplement` 补充版本号。
    pub supplement: i64,
}

impl CidSystemInfo {
    /// 创建 `/CIDSystemInfo`。
    pub fn new(registry: impl Into<String>, ordering: impl Into<String>, supplement: i64) -> Self {
        Self {
            registry: registry.into(),
            ordering: ordering.into(),
            supplement,
        }
    }

    /// 转换为 PDF 字典值。
    pub(crate) fn to_value(&self) -> Value {
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String(self.registry.clone()))
                .insert("Ordering", Value::String(self.ordering.clone()))
                .insert("Supplement", Value::Integer(self.supplement)),
        )
    }
}

/// CIDFont 子类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CidFontSubtype {
    /// CIDFontType0（CFF 轮廓）。
    CIDFontType0,
    /// CIDFontType2（TrueType 轮廓）。
    CIDFontType2,
}

impl CidFontSubtype {
    /// 返回 PDF 子类型名。
    pub(crate) fn as_name(&self) -> &'static str {
        match self {
            CidFontSubtype::CIDFontType0 => "CIDFontType0",
            CidFontSubtype::CIDFontType2 => "CIDFontType2",
        }
    }
}

/// CIDFont 宽度条目：单个 CID 或 CID 区间共用一个宽度。
#[derive(Debug, Clone, PartialEq)]
pub enum CidWidth {
    /// 单个 CID 的宽度。
    Single(u16, f64),
    /// CID 区间 `[first, last]` 共用同一宽度。
    Range(u16, u16, f64),
}

/// Type 0 字体的嵌入 CMap 编码配置（PDF 32000-1 §9.7.5）。
///
/// 仅支持 2 字节字符码：字符码按 2 字节大端序写入内容流，再由
/// [`CidEncoding::mappings`] 映射为 CID。未配置本编码时 Type 0 字体按
/// `/Identity-H` 写出（字符码即 CID）。
///
/// `wmode` 只能为 0（水平）或 1（垂直）；非法值或空映射会在
/// [`PdfBuilder::build`] 阶段返回 [`WriteError::InvalidObject`]。
///
/// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
/// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidEncoding {
    /// 书写模式：0 水平（`/Custom-H`）、1 垂直（`/Custom-V`）。
    pub wmode: u8,
    /// 字符码 → CID 映射，写出时按字符码升序排列。
    pub mappings: BTreeMap<u16, u16>,
}

impl CidEncoding {
    /// 创建指定书写模式的空编码。
    ///
    /// 空映射本身合法，但 [`CidFont::with_encoding`] 使用后若未调用
    /// [`CidEncoding::mapping`] 登记任何映射，构建会因空映射失败。
    pub fn new(wmode: u8) -> Self {
        Self {
            wmode,
            mappings: BTreeMap::new(),
        }
    }

    /// 登记一个字符码到 CID 的映射（链式构造）。
    ///
    /// 同一字符码重复登记时以最后一次为准。
    pub fn mapping(mut self, code: u16, cid: u16) -> Self {
        self.mappings.insert(code, cid);
        self
    }
}

/// CIDFont 逻辑字体描述。
///
/// 对应 PDF 标准 Type 0 复合字体中的 CIDFont 后代字体字典。
/// 包含 `/CIDSystemInfo`、`/FontDescriptor`（含 `/CIDSet`）、`/DW`/`/W` 宽度信息，
/// 并可通过 [`CidFont::with_font_program`] 嵌入字体程序。
/// 通过 [`PdfBuilder::cid_font`] 注册到页面资源。
#[derive(Debug, Clone, PartialEq)]
pub struct CidFont {
    /// `/BaseFont` 名称。
    pub name: String,
    /// `/Subtype`（CIDFontType0 或 CIDFontType2）。
    pub subtype: CidFontSubtype,
    /// `/CIDSystemInfo`。
    pub system_info: CidSystemInfo,
    /// 字体描述符（含 `/CIDSet`）。
    pub descriptor: FontDescriptor,
    /// `/DW` 默认宽度（默认 1000）。
    pub default_width: f64,
    /// `/W` 宽度条目。
    pub widths: Vec<CidWidth>,
    /// 字号（pt），用于内容流 `Tf` 操作符。
    pub size: f64,
    /// 可选的嵌入字体程序。
    ///
    /// CIDFontType2 接受 [`FontProgram::TrueType`]（写为 `/FontFile2`），
    /// CIDFontType0 接受 [`FontProgram::CFF`]（写为 `/FontFile3`，流
    /// `/Subtype /CIDFontType0C`）；类型不匹配时 [`PdfBuilder::build`]
    /// 返回错误。
    pub font_program: Option<FontProgram>,
    /// 可选的自定义编码 CMap（非 Identity）。
    ///
    /// 为 `None` 时 Type 0 父字体写 `/Encoding /Identity-H`，字符码即 CID；
    /// 为 `Some` 时写出嵌入 CMap 流并以引用作为 `/Encoding`，2 字节字符码
    /// 经映射转换为 CID。`wmode` 非法或映射为空会使 [`PdfBuilder::build`]
    /// 返回错误。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub encoding: Option<CidEncoding>,
    /// 字符码 → Unicode 映射，写出时生成 Type 0 字体的 `/ToUnicode` CMap。
    ///
    /// 键是**内容流中的源字符码**（即使经自定义 CMap 映射到不同 CID）；
    /// 非空时 [`PdfBuilder::build`] 用 `bfchar`/`bfrange` 条目输出标准
    /// ToUnicode CMap；[`PdfBuilder::text_cid_unicode`] 与
    /// [`PdfBuilder::text_codes_unicode`] 会自动登记。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub to_unicode: BTreeMap<u16, String>,
}

impl CidFont {
    /// 创建 CIDFont。
    pub fn new(
        name: impl Into<String>,
        subtype: CidFontSubtype,
        system_info: CidSystemInfo,
        descriptor: FontDescriptor,
        size: f64,
    ) -> Self {
        Self {
            name: name.into(),
            subtype,
            system_info,
            descriptor,
            default_width: 1000.0,
            widths: Vec::new(),
            size,
            font_program: None,
            encoding: None,
            to_unicode: BTreeMap::new(),
        }
    }

    /// 设置自定义编码 CMap（链式构造）。
    ///
    /// 使用后 Type 0 父字体的 `/Encoding` 指向嵌入 CMap 流，字符码按
    /// 2 字节大端序写出并经 [`CidEncoding::mappings`] 映射为 CID。
    pub fn with_encoding(mut self, encoding: CidEncoding) -> Self {
        self.encoding = Some(encoding);
        self
    }

    /// 设置默认宽度 `/DW`。
    pub fn with_default_width(mut self, width: f64) -> Self {
        self.default_width = width;
        self
    }

    /// 添加宽度条目。
    pub fn with_widths(mut self, widths: Vec<CidWidth>) -> Self {
        self.widths = widths;
        self
    }

    /// 嵌入字体程序（链式构造）。
    pub fn with_font_program(mut self, program: FontProgram) -> Self {
        self.font_program = Some(program);
        self
    }

    /// 构建 `/W` 数组值。
    pub(crate) fn widths_to_value(&self) -> Value {
        let mut arr = Array::new();
        for entry in &self.widths {
            match entry {
                CidWidth::Single(cid, width) => {
                    arr = arr
                        .push(Value::Integer(i64::from(*cid)))
                        .push(Value::Array(Array::new().push(Value::Float(*width))));
                }
                CidWidth::Range(first, last, width) => {
                    arr = arr
                        .push(Value::Integer(i64::from(*first)))
                        .push(Value::Integer(i64::from(*last)))
                        .push(Value::Float(*width));
                }
            }
        }
        Value::Array(arr)
    }
}
