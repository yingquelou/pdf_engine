//! 标准结构属性所有者的类型化读端（ISO 32000-1 14.8.5.2、14.8.5.4–14.8.5.7）。
//!
//! 属性对象仍由 14.7.5 Table 327 表达（`/O` 必填名字），本模块把同一结构
//! 元素实际生效的属性对象（已合并 `/ClassMap` 与 `/A`，见
//! [`StructureTree::resolved_attributes`]）按所有者分类，并解析：
//!
//! - Layout（14.8.5.4 Table 342–346）：见子模块 [`layout`]；
//! - List（Table 347）：`/ListNumbering`；
//! - PrintField（Table 348）：`/Role`、`/checked`、`/Desc`；
//! - Table（Table 349）：`/RowSpan`、`/ColSpan`、`/Headers`、`/Scope`、
//!   `/Summary`。
//!
//! Table 341 的 7 个格式专用所有者（`XML-1.00`、`HTML-3.20`、`HTML-4.01`、
//! `OEB-1.00`、`RTF-1.05`、`CSS-1.00`、`CSS-2.00`）作为
//! [`FormatSpecificOwner`] 类型化后进入 [`StandardAttributes::format_specific`]，
//! 条目保持原始字典/流不解释（ISO 32000-1 不定义其集合）。格式专用属性不
//! 参与 14.8.5.3 标准继承：它们只用于导出到对应格式，是取值优先级 (a)。
//!
//! [`StructureTree::standard_attributes`] 只报告元素自身（含其类）的合并
//! 结果；14.8.5.3 的继承链解析（祖先可继承条目）由子模块 [`inherit`] 的
//! [`StructureTree::effective_standard_attributes`] 提供。

use crate::core::reader::content::text_value;
use crate::core::types::{Dict, Value};

use super::{StructureAttribute, StructureError, StructureTree, malformed};

mod inherit;
mod layout;

pub use layout::{
    BlockAlign, BorderColor, BorderStyle, BorderStyles, BorderThickness, GlyphOrientationVertical,
    InlineAlign, LayoutAttributes, LineHeight, NumberOrAuto, NumberOrFour, NumberOrNumbers,
    Placement, RubyAlign, RubyPosition, TextAlign, TextDecorationType, WritingMode,
};

/// `/Headers` 数组的条目上限（与结构元素子项上限一致）。
const MAX_TABLE_HEADERS: usize = 65536;

/// 标准属性所有者（14.8.5.2 Table 341）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StandardAttributeOwner {
    /// `/Layout`：布局属性（Table 342–346）。
    Layout,
    /// `/List`：列表属性（Table 347）。
    List,
    /// `/PrintField`：表单域属性（Table 348，PDF 1.7）。
    PrintField,
    /// `/Table`：表格属性（Table 349）。
    Table,
    /// 未知/扩展所有者，保留原名字。
    Other(String),
}

impl StandardAttributeOwner {
    /// 按 `/O` 名字分类；未知名字归入 `Other`。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Layout" => Self::Layout,
            "List" => Self::List,
            "PrintField" => Self::PrintField,
            "Table" => Self::Table,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 所有者的 PDF 名字（不含前导 `/`）。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Layout => "Layout",
            Self::List => "List",
            Self::PrintField => "PrintField",
            Self::Table => "Table",
            Self::Other(name) => name,
        }
    }
}

/// Table 341 定义的格式专用属性所有者（14.8.5.2）。
///
/// 这些所有者的属性条目集合是开放式的（ISO 32000-1 不定义），读端只识别
/// 所有者名字，条目原样保留在 [`FormatSpecificAttributes::value`] 中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatSpecificOwner {
    /// `/XML-1.00`：XML 1.00。
    Xml1_00,
    /// `/HTML-3.20`：HTML 3.20。
    Html3_20,
    /// `/HTML-4.01`：HTML 4.01。
    Html4_01,
    /// `/OEB-1.00`：OEB 1.00。
    Oeb1_00,
    /// `/RTF-1.05`：RTF 1.05。
    Rtf1_05,
    /// `/CSS-1.00`：CSS 1.00。
    Css1_00,
    /// `/CSS-2.00`：CSS 2.00。
    Css2_00,
}

impl FormatSpecificOwner {
    /// 按 `/O` 名字识别 7 个 Table 341 格式专用所有者；其余名字返回 `None`。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "XML-1.00" => Some(Self::Xml1_00),
            "HTML-3.20" => Some(Self::Html3_20),
            "HTML-4.01" => Some(Self::Html4_01),
            "OEB-1.00" => Some(Self::Oeb1_00),
            "RTF-1.05" => Some(Self::Rtf1_05),
            "CSS-1.00" => Some(Self::Css1_00),
            "CSS-2.00" => Some(Self::Css2_00),
            _ => None,
        }
    }

    /// 所有者的 PDF 名字（不含前导 `/`）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Xml1_00 => "XML-1.00",
            Self::Html3_20 => "HTML-3.20",
            Self::Html4_01 => "HTML-4.01",
            Self::Oeb1_00 => "OEB-1.00",
            Self::Rtf1_05 => "RTF-1.05",
            Self::Css1_00 => "CSS-1.00",
            Self::Css2_00 => "CSS-2.00",
        }
    }
}

/// 一个格式专用属性对象（14.8.5.2 Table 341）。
///
/// 条目集合由各目标格式自行定义，`value` 保留原始字典或流供调用方使用；
/// 读端不解释条目，也不参与 14.8.5.3 标准继承。
#[derive(Debug, Clone, PartialEq)]
pub struct FormatSpecificAttributes {
    /// `/O`：格式专用所有者。
    pub owner: FormatSpecificOwner,
    /// 属性对象（原始字典或流）。
    pub value: Value,
    /// 属性对象的修订号（缺省 0）。
    pub revision: u32,
}

impl FormatSpecificAttributes {
    /// 属性对象为字典时返回引用；流形态返回 `None`。
    pub fn dict(&self) -> Option<&Dict> {
        self.value.as_dict()
    }
}

/// 列表编号方式（Table 347 `/ListNumbering`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListNumbering {
    /// 无编号。
    None,
    /// 实心圆点。
    Disc,
    /// 空心圆。
    Circle,
    /// 方块。
    Square,
    /// 十进制阿拉伯数字。
    Decimal,
    /// 大写罗马数字。
    UpperRoman,
    /// 小写罗马数字。
    LowerRoman,
    /// 大写字母。
    UpperAlpha,
    /// 小写字母。
    LowerAlpha,
    /// 规范允许将来扩展；保留原名。
    Other(String),
}

impl ListNumbering {
    fn from_name(name: &str) -> Self {
        match name {
            "None" => Self::None,
            "Disc" => Self::Disc,
            "Circle" => Self::Circle,
            "Square" => Self::Square,
            "Decimal" => Self::Decimal,
            "UpperRoman" => Self::UpperRoman,
            "LowerRoman" => Self::LowerRoman,
            "UpperAlpha" => Self::UpperAlpha,
            "LowerAlpha" => Self::LowerAlpha,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// List 标准属性（Table 347，`/O /List`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ListAttributes {
    /// `/ListNumbering`：编号方式，缺省 [`ListNumbering::None`]。
    pub numbering: ListNumbering,
    /// 合并后的属性字典（保留未建模条目）。
    pub dict: Dict,
}

/// 表单域角色（Table 348 `/Role`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintFieldRole {
    /// `/rb`：单选按钮。
    RadioButton,
    /// `/cb`：复选框。
    CheckBox,
    /// `/pb`：按钮。
    PushButton,
    /// `/tv`：文本值域。
    TextValue,
    /// 未知角色，保留原名。
    Other(String),
}

impl PrintFieldRole {
    fn from_name(name: &str) -> Self {
        match name {
            "rb" => Self::RadioButton,
            "cb" => Self::CheckBox,
            "pb" => Self::PushButton,
            "tv" => Self::TextValue,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 表单域选中状态（Table 348 `/checked`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintFieldChecked {
    /// `/on`：选中。
    On,
    /// `/off`：未选中（缺省）。
    Off,
    /// `/neutral`：不确定状态。
    Neutral,
    /// 未知状态，保留原名。
    Other(String),
}

impl PrintFieldChecked {
    fn from_name(name: &str) -> Self {
        // 规范明确 `/checked` 键名的大小写不合常规，值仍按小写比较。
        match name {
            "on" => Self::On,
            "off" => Self::Off,
            "neutral" => Self::Neutral,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// PrintField 标准属性（Table 348，`/O /PrintField`，PDF 1.7）。
#[derive(Debug, Clone, PartialEq)]
pub struct PrintFieldAttributes {
    /// `/Role`：表单域角色；缺省 `None`。
    pub role: Option<PrintFieldRole>,
    /// `/checked`：选中状态，缺省 [`PrintFieldChecked::Off`]。
    pub checked: PrintFieldChecked,
    /// `/Desc`：替代描述（已解码的文本串）。
    pub description: Option<String>,
    /// 合并后的属性字典（保留未建模条目）。
    pub dict: Dict,
}

/// 表格单元作用范围（Table 349 `/Scope`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableScope {
    /// `/Row`：行。
    Row,
    /// `/Column`：列。
    Column,
    /// `/Both`：行与列。
    Both,
    /// 未知范围，保留原名。
    Other(String),
}

impl TableScope {
    fn from_name(name: &str) -> Self {
        match name {
            "Row" => Self::Row,
            "Column" => Self::Column,
            "Both" => Self::Both,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// Table 标准属性（Table 349，`/O /Table`）。
#[derive(Debug, Clone, PartialEq)]
pub struct TableAttributes {
    /// `/RowSpan`：跨行数，缺省 1。
    pub row_span: u32,
    /// `/ColSpan`：跨列数，缺省 1。
    pub col_span: u32,
    /// `/Headers`：表头结构元素 `/ID` 的原始字节，按声明顺序。
    pub headers: Vec<Vec<u8>>,
    /// `/Scope`：作用范围，缺省 `None`。
    pub scope: Option<TableScope>,
    /// `/Summary`：表格摘要（已解码的文本串）。
    pub summary: Option<String>,
    /// 合并后的属性字典（保留未建模条目）。
    pub dict: Dict,
}

/// 一个结构元素生效的全部标准属性。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StandardAttributes {
    /// Layout 属性（`/O /Layout`）；缺省 `None`。
    pub layout: Option<LayoutAttributes>,
    /// List 属性（`/O /List`）；缺省 `None`。
    pub list: Option<ListAttributes>,
    /// PrintField 属性（`/O /PrintField`）；缺省 `None`。
    pub print_field: Option<PrintFieldAttributes>,
    /// Table 属性（`/O /Table`）；缺省 `None`。
    pub table: Option<TableAttributes>,
    /// 格式专用属性对象（Table 341 的 7 个所有者），按合并顺序克隆；
    /// 条目保持原始字典/流不解释。
    pub format_specific: Vec<FormatSpecificAttributes>,
    /// 既非四个标准所有者、也非 Table 341 格式专用所有者的属性对象
    /// （如 `/O /UserProperties`），按合并顺序克隆。
    pub other: Vec<StructureAttribute>,
}

impl StructureTree {
    /// 结构元素自身（含 `/ClassMap` 合并后）的标准属性
    /// （14.8.5.2、14.8.5.4–14.8.5.7）。
    ///
    /// 基于 [`StructureTree::resolved_attributes`] 的结果（`/ClassMap` 与
    /// `/A` 已按所有者合并），每个标准所有者至多解析出一个字典。越界
    /// 下标返回全空 [`StandardAttributes`]。
    ///
    /// 本方法**不**应用 14.8.5.3 的继承：需要祖先可继承条目的调用方应使用
    /// [`StructureTree::effective_standard_attributes`]。
    pub fn standard_attributes(&self, index: usize) -> Result<StandardAttributes, StructureError> {
        parse_attributes(&self.resolved_attributes(index), index)
    }
}

/// 把一组已合并的属性对象解析为类型化标准属性。
///
/// 供 [`StructureTree::standard_attributes`]（元素自身）与
/// [`StructureTree::effective_standard_attributes`]（含 14.8.5.3 继承）
/// 共用；错误消息只取决于 `index`，与调用方无关。
pub(super) fn parse_attributes(
    attributes: &[StructureAttribute],
    index: usize,
) -> Result<StandardAttributes, StructureError> {
    let mut standard = StandardAttributes::default();
    for attribute in attributes {
        match StandardAttributeOwner::from_name(&attribute.owner) {
            StandardAttributeOwner::Layout => {
                let dict = attribute_dict(attribute, index, "Layout")?;
                standard.layout = Some(layout::parse(dict, index)?);
            }
            StandardAttributeOwner::List => {
                let dict = attribute_dict(attribute, index, "List")?;
                standard.list = Some(parse_list(dict, index)?);
            }
            StandardAttributeOwner::PrintField => {
                let dict = attribute_dict(attribute, index, "PrintField")?;
                standard.print_field = Some(parse_print_field(dict, index)?);
            }
            StandardAttributeOwner::Table => {
                let dict = attribute_dict(attribute, index, "Table")?;
                standard.table = Some(parse_table(dict, index)?);
            }
            _ => match FormatSpecificOwner::from_name(&attribute.owner) {
                Some(owner) => standard.format_specific.push(FormatSpecificAttributes {
                    owner,
                    value: attribute.value.clone(),
                    revision: attribute.revision.unwrap_or(0),
                }),
                None => standard.other.push(attribute.clone()),
            },
        }
    }
    Ok(standard)
}

/// 属性对象的值必须是字典或流（Table 327 已保证，这里防御性复核）。
fn attribute_dict<'a>(
    attribute: &'a StructureAttribute,
    index: usize,
    owner: &str,
) -> Result<&'a Dict, StructureError> {
    attribute.value.as_dict().ok_or_else(|| {
        malformed(format!(
            "structure element {index} /{owner} attribute object must be a dictionary ({})",
            attribute.value.type_name()
        ))
    })
}

/// List 属性（Table 347）。
fn parse_list(dict: &Dict, index: usize) -> Result<ListAttributes, StructureError> {
    let numbering = match dict.get("ListNumbering") {
        None => ListNumbering::None,
        Some(Value::Name(name)) => ListNumbering::from_name(name),
        Some(other) => {
            return Err(malformed(format!(
                "structure element {index} /ListNumbering must be a name ({})",
                other.type_name()
            )));
        }
    };
    Ok(ListAttributes {
        numbering,
        dict: dict.clone(),
    })
}

/// PrintField 属性（Table 348）。
fn parse_print_field(dict: &Dict, index: usize) -> Result<PrintFieldAttributes, StructureError> {
    let role = match dict.get("Role") {
        None => None,
        Some(Value::Name(name)) => Some(PrintFieldRole::from_name(name)),
        Some(other) => {
            return Err(malformed(format!(
                "structure element {index} /Role must be a name ({})",
                other.type_name()
            )));
        }
    };
    let checked = match dict.get("checked") {
        None => PrintFieldChecked::Off,
        Some(Value::Name(name)) => PrintFieldChecked::from_name(name),
        Some(other) => {
            return Err(malformed(format!(
                "structure element {index} /checked must be a name ({})",
                other.type_name()
            )));
        }
    };
    Ok(PrintFieldAttributes {
        role,
        checked,
        description: text_string(dict, "Desc", index)?,
        dict: dict.clone(),
    })
}

/// Table 属性（Table 349）。
fn parse_table(dict: &Dict, index: usize) -> Result<TableAttributes, StructureError> {
    Ok(TableAttributes {
        row_span: span(dict, "RowSpan", index)?,
        col_span: span(dict, "ColSpan", index)?,
        headers: headers(dict, index)?,
        scope: match dict.get("Scope") {
            None => None,
            Some(Value::Name(name)) => Some(TableScope::from_name(name)),
            Some(other) => {
                return Err(malformed(format!(
                    "structure element {index} /Scope must be a name ({})",
                    other.type_name()
                )));
            }
        },
        summary: text_string(dict, "Summary", index)?,
        dict: dict.clone(),
    })
}

/// `/RowSpan` 与 `/ColSpan`：非负整数且 ≥1，缺省 1。
fn span(dict: &Dict, key: &str, index: usize) -> Result<u32, StructureError> {
    let Some(value) = dict.get(key) else {
        return Ok(1);
    };
    match value {
        Value::Integer(value) if *value >= 1 && *value <= i64::from(u32::MAX) => Ok(*value as u32),
        other => Err(malformed(format!(
            "structure element {index} /{key} must be an integer of at least 1 ({})",
            other.type_name()
        ))),
    }
}

/// `/Headers`：字节串数组，条目不超过 [`MAX_TABLE_HEADERS`]。
fn headers(dict: &Dict, index: usize) -> Result<Vec<Vec<u8>>, StructureError> {
    let Some(value) = dict.get("Headers") else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = value else {
        return Err(malformed(format!(
            "structure element {index} /Headers must be an array of byte strings ({})",
            value.type_name()
        )));
    };
    if items.0.len() > MAX_TABLE_HEADERS {
        return Err(StructureError::LimitExceeded("table attribute headers"));
    }
    let mut headers = Vec::with_capacity(items.0.len());
    for (position, item) in items.0.iter().enumerate() {
        let bytes = match item {
            Value::String(text) => text
                .chars()
                .map(|character| character as u32 as u8)
                .collect(),
            Value::HexString(hex) => decode_hex(hex).ok_or_else(|| {
                malformed(format!(
                    "structure element {index} /Headers entry {position} is not valid hexadecimal"
                ))
            })?,
            other => {
                return Err(malformed(format!(
                    "structure element {index} /Headers entry {position} must be a byte string ({})",
                    other.type_name()
                )));
            }
        };
        if bytes.is_empty() {
            return Err(malformed(format!(
                "structure element {index} /Headers entry {position} must not be empty"
            )));
        }
        headers.push(bytes);
    }
    Ok(headers)
}

/// 文本串条目：只接受字符串/十六进制串，经 `content::text_value` 解码。
fn text_string(dict: &Dict, key: &str, index: usize) -> Result<Option<String>, StructureError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    if !matches!(value, Value::String(_) | Value::HexString(_)) {
        return Err(malformed(format!(
            "structure element {index} /{key} must be a text string ({})",
            value.type_name()
        )));
    }
    text_value(value).map(Some).ok_or_else(|| {
        malformed(format!(
            "structure element {index} /{key} cannot be decoded"
        ))
    })
}

/// 十六进制串 → 原始字节；奇数位末尾补 0（与 `/ID` 解码一致）。
fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let mut digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if digits.len() % 2 == 1 {
        digits.push(b'0');
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.as_chunks::<2>().0 {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes.push(high * 16 + low);
    }
    Some(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod inherit_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod tests;
