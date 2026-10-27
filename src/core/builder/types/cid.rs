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

/// `/W2` 垂直度量条目（PDF 32000-1 §9.7.4.3）。
///
/// 每个 CID 携带 `w1_y`（位置向量的垂直分量）与 `v_x`/`v_y`（垂直
/// 位移向量的分量）。未在 `/W2` 中列出的 CID 按 `/DW2` 回退，其中
/// `v_x` 取该 CID 水平宽度的一半。
#[derive(Debug, Clone, PartialEq)]
pub enum CidVerticalMetrics {
    /// 单个 CID 的 `w1_y v_x v_y` 三元组。
    Single(u16, f64, f64, f64),
    /// CID 区间 `[first, last]` 共用一个三元组。
    Range(u16, u16, f64, f64, f64),
}

/// Type 0 字体的嵌入 CMap 编码配置（PDF 32000-1 §9.7.5）。
///
/// 字符码按大端序写入内容流，再由映射转换为 CID。2 字节码走
/// [`CidEncoding::mappings`]，1/3/4 字节码走 [`CidEncoding::byte_mappings`]；
/// 也可通过 [`CidEncoding::with_usecmap`] 声明从基座 CMap 继承。未配置本
/// 编码时 Type 0 字体按 `/Identity-H` 写出（字符码即 CID）。
///
/// `wmode` 只能为 0（水平）或 1（垂直）；非法值、空映射、非法码长、
/// 非法 `/CMapName` 或未知 `usecmap` 基座会在 [`PdfBuilder::build`] 阶段
/// 返回 [`WriteError::InvalidObject`]。
///
/// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
/// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidEncoding {
    /// 书写模式：0 水平（`/Custom-H`）、1 垂直（`/Custom-V`）。
    pub wmode: u8,
    /// 2 字节字符码 → CID 映射，写出时按字符码升序排列。
    pub mappings: BTreeMap<u16, u16>,
    /// 1/3/4 字节字符码 → CID 映射，键为完整字符码字节序列。
    ///
    /// 2 字节码统一存入 [`CidEncoding::mappings`]；本表仅保存其他码长，
    /// 由 [`CidEncoding::mapping_bytes`] 维护。键长度必须为 1..=4，否则
    /// 构建返回 [`WriteError::InvalidObject`]。
    ///
    /// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
    pub byte_mappings: BTreeMap<Vec<u8>, u16>,
    /// 可选的自定义 `/CMapName`（不含前导 `/`）。
    ///
    /// 为 `None` 时按书写模式写 `/Custom-H` 或 `/Custom-V`；名称必须非空
    /// 且只含可安全写入 PDF Name 的 ASCII 字符，否则构建报错。
    pub cmap_name: Option<String>,
    /// 可选的 `usecmap` 基座 CMap 名称（不含前导 `/`）。
    ///
    /// 写出时在 codespace 之前写 `/<基座> usecmap`，仅声明继承关系；
    /// 本 CMap 仍显式写出自己的 codespace 与全部映射。基座须为
    /// `/Identity-H`、`/Identity-V` 或读端 `predefined_cmaps` 名册中的
    /// 名称，且不能与 `/CMapName` 同名，否则构建报错。
    pub usecmap: Option<String>,
}

impl CidEncoding {
    /// 创建指定书写模式的空编码。
    ///
    /// 空映射本身合法，但 [`CidFont::with_encoding`] 使用后若未调用
    /// [`CidEncoding::mapping`] 或 [`CidEncoding::mapping_bytes`] 登记任何
    /// 映射，构建会因空映射失败。
    pub fn new(wmode: u8) -> Self {
        Self {
            wmode,
            mappings: BTreeMap::new(),
            byte_mappings: BTreeMap::new(),
            cmap_name: None,
            usecmap: None,
        }
    }

    /// 登记一个 2 字节字符码到 CID 的映射（链式构造）。
    ///
    /// 同一字符码重复登记时以最后一次为准。
    pub fn mapping(mut self, code: u16, cid: u16) -> Self {
        self.mappings.insert(code, cid);
        self
    }

    /// 登记一个显式字节字符码到 CID 的映射（链式构造）。
    ///
    /// `code` 为字符码的完整大端字节序列，长度 1..=4；长度 2 时等价于
    /// [`CidEncoding::mapping`]（存入 [`CidEncoding::mappings`]），其他长度
    /// 存入 [`CidEncoding::byte_mappings`]。非法长度（空或超过 4 字节）在
    /// 构建阶段以 [`WriteError::InvalidObject`] 报出。同一字符码重复登记
    /// 时以最后一次为准。
    ///
    /// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
    pub fn mapping_bytes(mut self, code: &[u8], cid: u16) -> Self {
        match code {
            [high, low] => {
                self.mappings.insert(u16::from_be_bytes([*high, *low]), cid);
            }
            _ => {
                self.byte_mappings.insert(code.to_vec(), cid);
            }
        }
        self
    }

    /// 设置自定义 `/CMapName`（链式构造）。
    ///
    /// 名称不含前导 `/`；合法性在构建阶段校验。
    pub fn with_cmap_name(mut self, name: impl Into<String>) -> Self {
        self.cmap_name = Some(name.into());
        self
    }

    /// 设置 `usecmap` 基座 CMap 名称（链式构造）。
    ///
    /// 名称不含前导 `/`；必须是 `/Identity-H`、`/Identity-V` 或读端名册
    /// 内的预定义 CMap，否则构建阶段报错。
    pub fn with_usecmap(mut self, name: impl Into<String>) -> Self {
        self.usecmap = Some(name.into());
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
    /// `/DW2` 默认垂直度量 `[v_y, w1_y]`（默认 `[880, -1000]`）。
    ///
    /// `v_y` 是字形原点的垂直位移，`w1_y` 是位置向量垂直分量的默认值。
    /// 与默认值相同时省略 `/DW2`；非有限值会使构建返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    pub default_vertical_metrics: [f64; 2],
    /// `/W2` 垂直度量条目。
    pub vertical_metrics: Vec<CidVerticalMetrics>,
    /// 字号（pt），用于内容流 `Tf` 操作符。
    pub size: f64,
    /// 可选的嵌入字体程序。
    ///
    /// CIDFontType2 接受 [`FontProgram::TrueType`] 与 TrueType-flavored
    /// [`FontProgram::OpenType`]（写为 `/FontFile2`），CIDFontType0 接受
    /// [`FontProgram::CFF`]（写为 `/FontFile3`，流 `/Subtype /CIDFontType0C`）
    /// 与 CFF-flavored `OpenType`（流 `/Subtype /OpenType`）；类型或
    /// 轮廓不匹配时 [`PdfBuilder::build`] 返回错误。
    pub font_program: Option<FontProgram>,
    /// 可选的自定义编码 CMap（非 Identity）。
    ///
    /// 为 `None` 时 Type 0 父字体写 `/Encoding /Identity-H`，字符码即 CID；
    /// 为 `Some` 时写出嵌入 CMap 流并以引用作为 `/Encoding`，字符码经映射
    /// 转换为 CID（支持 1/3/4 字节码、自定义名称与 `usecmap` 基座）。
    /// `wmode` 非法或映射为空会使 [`PdfBuilder::build`] 返回错误。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub encoding: Option<CidEncoding>,
    /// 可选的预定义 CMap 名称引用（不嵌入 CMap 流）。
    ///
    /// 为 `Some` 时 Type 0 父字体的 `/Encoding` 直接写该名称（如
    /// `/UniGB-UCS2-H`），不生成嵌入 CMap 流；读端按同名名册解析，写出的
    /// 文档保持「写入→解析→提取」可还原。名称必须在读端
    /// `predefined_cmaps` 名册内（当前为 `Adobe-GB1`、`Adobe-CNS1`、
    /// `Adobe-Japan1`、`Adobe-Korea1`、`UniGB-UCS2-H`），否则
    /// [`PdfBuilder::build`] 返回 [`WriteError::InvalidObject`]。
    ///
    /// 与 [`CidFont::encoding`]（嵌入 CMap）互斥：同时设置时构建报错。
    /// 名册内 CMap 均为水平书写（WMode 0），与读端解析语义一致；`-V`
    /// 竖排变体不在名册内，传入会在构建阶段按未知名称报错。
    ///
    /// 文本写入可用 [`PdfBuilder::text_predefined_unicode`]（按 Unicode
    /// 文本编码为目标 CMap 的 2 字节字符码）或沿用
    /// [`PdfBuilder::text_codes_unicode`] 直接给字符码。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    /// [`PdfBuilder::text_predefined_unicode`]: crate::core::builder::PdfBuilder::text_predefined_unicode
    /// [`PdfBuilder::text_codes_unicode`]: crate::core::builder::PdfBuilder::text_codes_unicode
    /// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
    pub predefined_encoding: Option<String>,
    /// 可选的 CID 子集列表（原 CFF 程序中的 CID，不含必须保留的 CID 0）。
    ///
    /// 为 `Some` 时，嵌入的 CFF 程序（仅 [`CidFontSubtype::CIDFontType0`]
    /// 接受）会先由
    /// [`subset_cff`](crate::core::reader::font::subset_cff) 重写为只含
    /// CID 0 与这些 CID 的子集，且 `/BaseFont`（Type 0 父字体与 CIDFont
    /// 后代一致）加上由原始程序内容确定的六字母子集标签前缀
    /// （PDF 32000-1 §9.6.4）；描述符提供的 `/CIDSet` 会按子集 CID 交集
    /// 裁剪。未嵌入程序、程序不是 [`FontProgram::CFF`] 或子类型不是
    /// CIDFontType0 时，[`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub subset_cids: Option<Vec<u16>>,
    /// 可选的 `/CIDToGIDMap`（CIDFontType2 专用）。
    ///
    /// 数组按 CID 索引给出 TrueType glyph ID，写出为 2 字节大端流
    /// （覆盖 CID 0..len-1），并在 CIDFont 字典写 `/CIDToGIDMap` 引用。
    /// 为 `None` 时不写该条目，读端按 Identity 处理。仅
    /// [`CidFontSubtype::CIDFontType2`] 允许设置，其他子类型在
    /// [`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub cid_to_gid: Option<Vec<u16>>,
    /// 字符码 → Unicode 映射，写出时生成 Type 0 字体的 `/ToUnicode` CMap。
    ///
    /// 键是**内容流中的源字符码**（即使经自定义 CMap 映射到不同 CID）；
    /// 非空时 [`PdfBuilder::build`] 用 `bfchar`/`bfrange` 条目输出标准
    /// ToUnicode CMap；[`PdfBuilder::text_cid_unicode`] 与
    /// [`PdfBuilder::text_codes_unicode`] 会自动登记。本表仅存 2 字节码，
    /// 其他码长见 [`CidFont::to_unicode_bytes`]。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub to_unicode: BTreeMap<u16, String>,
    /// 非 2 字节源字符码（完整字节序列）→ Unicode 映射。
    ///
    /// 与 [`CidFont::to_unicode`] 一同生成 `/ToUnicode` CMap，codespace 与
    /// `bfchar`/`bfrange` 键按字符码的实际字节长度写出；由
    /// [`PdfBuilder::text_code_bytes_unicode`] 自动登记。键长度须为
    /// 1..=4 且不得与 [`CidFont::to_unicode`] 中的 2 字节码重复。
    ///
    /// [`PdfBuilder::text_code_bytes_unicode`]: crate::core::builder::PdfBuilder::text_code_bytes_unicode
    pub to_unicode_bytes: BTreeMap<Vec<u8>, String>,
    /// 可选 ToUnicode CMap 的 `usecmap` 基座名（不含前导 `/`）。
    ///
    /// 写出 `/ToUnicode` CMap 时在 codespace 之前写 `/<基座> usecmap`；
    /// 此时生成的 `/CMapName` 为 `/Custom-UCS`，避免与基座同名。当前仅
    /// 支持 `Adobe-Identity-UCS`（读端内置恒等基座：未显式映射的 2 字节
    /// 字符码按码值解析为 Unicode 标量）；其他名称在
    /// [`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    /// 即使没有任何显式映射，设置基座也会生成只含 codespace 与 `usecmap`
    /// 的 CMap。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub to_unicode_usecmap: Option<String>,
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
            default_vertical_metrics: [880.0, -1000.0],
            vertical_metrics: Vec::new(),
            size,
            font_program: None,
            encoding: None,
            predefined_encoding: None,
            subset_cids: None,
            cid_to_gid: None,
            to_unicode: BTreeMap::new(),
            to_unicode_bytes: BTreeMap::new(),
            to_unicode_usecmap: None,
        }
    }

    /// 设置自定义编码 CMap（链式构造）。
    ///
    /// 使用后 Type 0 父字体的 `/Encoding` 指向嵌入 CMap 流，字符码按
    /// 实际字节长度写出并经 [`CidEncoding::mappings`] 映射为 CID。
    /// 与 [`CidFont::with_predefined_encoding`] 互斥，同时设置时
    /// [`PdfBuilder::build`] 返回错误。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub fn with_encoding(mut self, encoding: CidEncoding) -> Self {
        self.encoding = Some(encoding);
        self
    }

    /// 按名称引用预定义 CMap 作为 Type 0 的 `/Encoding`（链式构造）。
    ///
    /// 名称须在读端 `predefined_cmaps` 名册内（当前为 `Adobe-GB1`、
    /// `Adobe-CNS1`、`Adobe-Japan1`、`Adobe-Korea1`、`UniGB-UCS2-H`）；
    /// 未知名称或与 [`CidFont::with_encoding`] 同时使用都会在
    /// [`PdfBuilder::build`] 阶段返回 [`WriteError::InvalidObject`]。
    /// 候选名称与 WMode 语义详见 [`CidFont::predefined_encoding`]。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    /// [`WriteError::InvalidObject`]: crate::core::writer::WriteError::InvalidObject
    pub fn with_predefined_encoding(mut self, name: impl Into<String>) -> Self {
        self.predefined_encoding = Some(name.into());
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

    /// 设置默认垂直度量 `/DW2`（链式构造）。
    ///
    /// 参数按 `/DW2 [v_y w1_y]` 的顺序给出。非有限值会使
    /// [`PdfBuilder::build`](crate::core::builder::PdfBuilder::build)
    /// 返回 [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    pub fn with_default_vertical_metrics(mut self, v_y: f64, w1_y: f64) -> Self {
        self.default_vertical_metrics = [v_y, w1_y];
        self
    }

    /// 添加垂直度量条目 `/W2`（链式构造）。
    ///
    /// 区间条目的 `first` 须不大于 `last`，且全部数值须为有限数，否则
    /// 构建返回 [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    pub fn with_vertical_metrics(mut self, metrics: Vec<CidVerticalMetrics>) -> Self {
        self.vertical_metrics = metrics;
        self
    }

    /// 嵌入字体程序（链式构造）。
    pub fn with_font_program(mut self, program: FontProgram) -> Self {
        self.font_program = Some(program);
        self
    }

    /// 按 CID 子集嵌入 CFF 字体程序（链式构造）。
    ///
    /// `cids` 是原程序中的 CID 列表；CID 0（`.notdef`）总会保留，保留
    /// 字形在子集程序中按 CID 升序重编号。`/BaseFont` 会带六字母子集
    /// 标签前缀（标签仍按原始程序字节计算）。仅适用于
    /// [`CidFontSubtype::CIDFontType0`] + [`FontProgram::CFF`]；其他组合
    /// 或未知/越界 CID 会在 [`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    /// 与 [`CidFont::with_predefined_encoding`] 等编码配置可共存。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub fn with_subset_cids(mut self, cids: &[u16]) -> Self {
        self.subset_cids = Some(cids.to_vec());
        self
    }

    /// 设置 `/CIDToGIDMap` 映射（链式构造）。
    ///
    /// 数组第 `cid` 项给出该 CID 对应的 TrueType glyph ID，写出为 2 字节
    /// 大端流。仅 [`CidFontSubtype::CIDFontType2`] 允许设置，其他子类型
    /// 在 [`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub fn with_cid_to_gid_map(mut self, mapping: &[u16]) -> Self {
        self.cid_to_gid = Some(mapping.to_vec());
        self
    }

    /// 设置 ToUnicode CMap 的 `usecmap` 基座（链式构造）。
    ///
    /// 名称不含前导 `/`；当前仅支持 `Adobe-Identity-UCS`（读端内置恒等
    /// 基座），其他名称在 [`PdfBuilder::build`] 返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)。
    ///
    /// [`PdfBuilder::build`]: crate::core::builder::PdfBuilder::build
    pub fn with_to_unicode_usecmap(mut self, name: impl Into<String>) -> Self {
        self.to_unicode_usecmap = Some(name.into());
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

    /// 构建 `/W2` 数组值。
    pub(crate) fn vertical_metrics_to_value(&self) -> Value {
        let mut arr = Array::new();
        for entry in &self.vertical_metrics {
            match entry {
                CidVerticalMetrics::Single(cid, w1y, v1x, v1y) => {
                    arr = arr.push(Value::Integer(i64::from(*cid))).push(Value::Array(
                        Array::new()
                            .push(Value::Float(*w1y))
                            .push(Value::Float(*v1x))
                            .push(Value::Float(*v1y)),
                    ));
                }
                CidVerticalMetrics::Range(first, last, w1y, v1x, v1y) => {
                    arr = arr
                        .push(Value::Integer(i64::from(*first)))
                        .push(Value::Integer(i64::from(*last)))
                        .push(Value::Float(*w1y))
                        .push(Value::Float(*v1x))
                        .push(Value::Float(*v1y));
                }
            }
        }
        Value::Array(arr)
    }
}
