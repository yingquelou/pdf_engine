//! 标准结构属性所有者的类型化写端（ISO 32000-1 14.8.5.2、14.8.5.5–14.8.5.7）。
//!
//! 提供 List（Table 347）、PrintField（Table 348）与 Table（Table 349）
//! 属性对象的构造器，经 [`StructureAttribute::list`]、
//! [`StructureAttribute::print_field`]、[`StructureAttribute::table`] 生成
//! `/O` 分别为 `/List`、`/PrintField`、`/Table` 的属性字典。缺省值按规范
//! 不物化：`/RowSpan` `/ColSpan` 仅在 ≠1 时写出，`/Desc` `/Summary`
//! `/Scope` 仅在提供时写出；`/checked` 与 `/ListNumbering` 为构造必填
//! （`ListNumbering::None` 与 `PrintFieldChecked::Off` 是规范缺省，但显式
//! 写出可避免读者自行补缺省）。
//!
//! 元素适用性与版本门禁由构建器在分配对象号之前完成（见
//! `builder/navigation/structure_standard.rs`）；本模块只负责生成符合
//! Table 347–349 的值形态。

use crate::core::types::{Array, Dict, Value};

use super::super::text_string_value;
use super::StructureAttribute;

/// 列表编号方式（Table 347 `/ListNumbering`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListNumbering {
    /// `/None`：无编号。
    None,
    /// `/Disc`：实心圆点。
    Disc,
    /// `/Circle`：空心圆。
    Circle,
    /// `/Square`：方块。
    Square,
    /// `/Decimal`：十进制阿拉伯数字。
    Decimal,
    /// `/UpperRoman`：大写罗马数字。
    UpperRoman,
    /// `/LowerRoman`：小写罗马数字。
    LowerRoman,
    /// `/UpperAlpha`：大写字母。
    UpperAlpha,
    /// `/LowerAlpha`：小写字母。
    LowerAlpha,
    /// 规范允许将来扩展；按原名字写出。
    Other(String),
}

impl ListNumbering {
    /// `/ListNumbering` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Disc => "Disc",
            Self::Circle => "Circle",
            Self::Square => "Square",
            Self::Decimal => "Decimal",
            Self::UpperRoman => "UpperRoman",
            Self::LowerRoman => "LowerRoman",
            Self::UpperAlpha => "UpperAlpha",
            Self::LowerAlpha => "LowerAlpha",
            Self::Other(name) => name,
        }
    }
}

/// List 标准属性（Table 347，`/O /List`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAttributes {
    /// `/ListNumbering`：编号方式。
    pub numbering: ListNumbering,
}

impl ListAttributes {
    /// 指定编号方式的 List 属性。
    pub fn new(numbering: ListNumbering) -> Self {
        Self { numbering }
    }
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
    /// 未知角色，按原名字写出。
    Other(String),
}

impl PrintFieldRole {
    /// `/Role` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::RadioButton => "rb",
            Self::CheckBox => "cb",
            Self::PushButton => "pb",
            Self::TextValue => "tv",
            Self::Other(name) => name,
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
    /// 未知状态，按原名字写出。
    Other(String),
}

impl PrintFieldChecked {
    /// `/checked` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Neutral => "neutral",
            Self::Other(name) => name,
        }
    }
}

/// PrintField 标准属性（Table 348，`/O /PrintField`，PDF 1.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintFieldAttributes {
    /// `/Role`：表单域角色。
    pub role: Option<PrintFieldRole>,
    /// `/checked`：选中状态，缺省 `Off`。
    pub checked: PrintFieldChecked,
    /// `/Desc`：替代描述。
    pub description: Option<String>,
}

impl PrintFieldAttributes {
    /// 指定角色的 PrintField 属性：`/checked` 缺省 `off`，无 `/Desc`。
    pub fn new(role: PrintFieldRole) -> Self {
        Self {
            role: Some(role),
            checked: PrintFieldChecked::Off,
            description: None,
        }
    }

    /// 设置 `/checked` 选中状态。
    pub fn with_checked(mut self, checked: PrintFieldChecked) -> Self {
        self.checked = checked;
        self
    }

    /// 设置 `/Desc` 替代描述。
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
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
    /// 未知范围，按原名字写出。
    Other(String),
}

impl TableScope {
    /// `/Scope` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Row => "Row",
            Self::Column => "Column",
            Self::Both => "Both",
            Self::Other(name) => name,
        }
    }
}

/// Table 标准属性（Table 349，`/O /Table`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableAttributes {
    /// `/RowSpan`：跨行数，缺省 1（不写出）。
    pub row_span: u32,
    /// `/ColSpan`：跨列数，缺省 1（不写出）。
    pub col_span: u32,
    /// `/Headers`：表头结构元素 `/ID` 的原始字节，按声明顺序。
    pub headers: Vec<Vec<u8>>,
    /// `/Scope`：作用范围。
    pub scope: Option<TableScope>,
    /// `/Summary`：表格摘要。
    pub summary: Option<String>,
}

impl TableAttributes {
    /// 全部取缺省值的 Table 属性：跨度为 1、无表头、无范围与摘要。
    pub fn new() -> Self {
        Self {
            row_span: 1,
            col_span: 1,
            headers: Vec::new(),
            scope: None,
            summary: None,
        }
    }

    /// 设置 `/RowSpan`。
    pub fn with_row_span(mut self, span: u32) -> Self {
        self.row_span = span;
        self
    }

    /// 设置 `/ColSpan`。
    pub fn with_col_span(mut self, span: u32) -> Self {
        self.col_span = span;
        self
    }

    /// 追加一个 `/Headers` 表头 `/ID`（原始字节）。
    pub fn with_header(mut self, id: impl Into<Vec<u8>>) -> Self {
        self.headers.push(id.into());
        self
    }

    /// 设置 `/Scope`。
    pub fn with_scope(mut self, scope: TableScope) -> Self {
        self.scope = Some(scope);
        self
    }

    /// 设置 `/Summary`。
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }
}

impl Default for TableAttributes {
    fn default() -> Self {
        Self::new()
    }
}

impl StructureAttribute {
    /// List 属性对象（Table 347，`/O /List`）。
    pub fn list(attributes: ListAttributes) -> Self {
        let dict = Dict::new().insert("O", Value::Name("List".into())).insert(
            "ListNumbering",
            Value::Name(attributes.numbering.name().to_owned()),
        );
        Self::new(Value::Dict(dict))
    }

    /// PrintField 属性对象（Table 348，`/O /PrintField`，PDF 1.7）。
    pub fn print_field(attributes: PrintFieldAttributes) -> Self {
        let mut dict = Dict::new().insert("O", Value::Name("PrintField".into()));
        if let Some(role) = &attributes.role {
            dict = dict.insert("Role", Value::Name(role.name().to_owned()));
        }
        dict = dict.insert("checked", Value::Name(attributes.checked.name().to_owned()));
        if let Some(description) = &attributes.description {
            dict = dict.insert("Desc", text_string_value(description));
        }
        Self::new(Value::Dict(dict))
    }

    /// Table 属性对象（Table 349，`/O /Table`）。
    ///
    /// 跨度为 1 时省略 `/RowSpan` `/ColSpan`；空表头数组、无范围与摘要时
    /// 同样省略对应条目。
    pub fn table(attributes: TableAttributes) -> Self {
        let mut dict = Dict::new().insert("O", Value::Name("Table".into()));
        if attributes.row_span != 1 {
            dict = dict.insert("RowSpan", Value::Integer(i64::from(attributes.row_span)));
        }
        if attributes.col_span != 1 {
            dict = dict.insert("ColSpan", Value::Integer(i64::from(attributes.col_span)));
        }
        if !attributes.headers.is_empty() {
            let mut headers = Array::new();
            for id in &attributes.headers {
                // Table 349 的 /Headers 是字节串；按每个字节一个 char 存放，
                // 写出端仅接受 ASCII，故不会与 UTF-8 编码冲突。
                headers =
                    headers.push(Value::String(id.iter().map(|byte| *byte as char).collect()));
            }
            dict = dict.insert("Headers", Value::Array(headers));
        }
        if let Some(scope) = &attributes.scope {
            dict = dict.insert("Scope", Value::Name(scope.name().to_owned()));
        }
        if let Some(summary) = &attributes.summary {
            dict = dict.insert("Summary", text_string_value(summary));
        }
        Self::new(Value::Dict(dict))
    }
}
