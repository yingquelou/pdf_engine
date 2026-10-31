//! 标准结构类型目录（ISO 32000-1 14.8.4 Table 333–340）。
//!
//! 14.8.4 把标准结构类型分为五组：
//!
//! - 分组元素（Table 333）：`Document` `Part` `Art` `Sect` `Div`
//!   `BlockQuote` `Caption` `TOC` `TOCI` `Index` `NonStruct` `Private`；
//! - 块级结构元素 BLSE（Table 334）：`P` `H` `H1`–`H6` `L` `LI` `Lbl`
//!   `LBody` `Table`（`Table` 是 BLSE，容器本身不是表格元素）；
//! - 表格元素（Table 337）：`TR` `TH` `TD` `THead` `TBody` `TFoot`
//!   （既非 BLSE 也非 ILSE）；
//! - 行内元素 ILSE（Table 338）：`Span` `Quote` `Note` `Reference`
//!   `BibEntry` `Code` `Link` `Annot`；
//! - Ruby/Warichu（Table 339）：`Ruby` `RB` `RT` `RP` `Warichu` `WT`
//!   `WP`；
//! - 插图元素（Table 340）：`Figure` `Formula` `Form`。
//!
//! 共 49 个标准名。结构元素的 `/S` 可以是任意名字：14.8.4.1 规定先沿
//! `/RoleMap` 一或多级映射，映射结果才是生效的标准类型（PDF 1.5 起即使
//! 原名就是标准名也先查 `/RoleMap`）。[`StructureTree::standard_type`]
//! 按这一规则给出 [`StandardStructureType`]。
//!
//! 11 个标准名自 PDF 1.5 引入：`THead` `TBody` `TFoot` `Annot` `Ruby`
//! `RB` `RT` `RP` `Warichu` `WT` `WP`；其余为 Tagged PDF 基线（PDF 1.3）。
//! 写端据此做版本门禁（`PdfFeature::StructureTypeExtensions`）。

use super::StructureTree;

/// 标准结构类型（14.8.4 Table 333–340 的 49 个标准名）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StandardStructureType {
    /// `/Document`：文档（Table 333）。
    Document,
    /// `/Part`：大块内容部分（Table 333）。
    Part,
    /// `/Art`：文章（Table 333）。
    Article,
    /// `/Sect`：节（Table 333）。
    Section,
    /// `/Div`：通用块级分组（Table 333）。
    Division,
    /// `/BlockQuote`：块引用（Table 333）。
    BlockQuote,
    /// `/Caption`：题注（Table 333）。
    Caption,
    /// `/TOC`：目录（Table 333）。
    TableOfContents,
    /// `/TOCI`：目录项（Table 333）。
    TableOfContentsItem,
    /// `/Index`：索引（Table 333）。
    Index,
    /// `/NonStruct`：无结构分组（Table 333）。
    NonStruct,
    /// `/Private`：私有元素（Table 333）。
    Private,
    /// `/P`：段落（Table 334）。
    Paragraph,
    /// `/H`：不区分级别的标题（Table 334）。
    Heading,
    /// `/H1`：一级标题（Table 334）。
    Heading1,
    /// `/H2`：二级标题（Table 334）。
    Heading2,
    /// `/H3`：三级标题（Table 334）。
    Heading3,
    /// `/H4`：四级标题（Table 334）。
    Heading4,
    /// `/H5`：五级标题（Table 334）。
    Heading5,
    /// `/H6`：六级标题（Table 334）。
    Heading6,
    /// `/L`：列表（Table 334）。
    List,
    /// `/LI`：列表项（Table 334）。
    ListItem,
    /// `/Lbl`：列表标记（Table 334）。
    Label,
    /// `/LBody`：列表项正文（Table 334）。
    ListBody,
    /// `/Table`：表格容器，属 BLSE（Table 334）。
    Table,
    /// `/TR`：表格行（Table 337）。
    TableRow,
    /// `/TH`：表头单元（Table 337）。
    TableHeader,
    /// `/TD`：数据单元（Table 337）。
    TableData,
    /// `/THead`：表头行组（Table 337，PDF 1.5）。
    TableHead,
    /// `/TBody`：表体行组（Table 337，PDF 1.5）。
    TableBody,
    /// `/TFoot`：表尾行组（Table 337，PDF 1.5）。
    TableFooter,
    /// `/Span`：行内通用分组（Table 338）。
    Span,
    /// `/Quote`：行内引用（Table 338）。
    Quote,
    /// `/Note`：脚注/尾注（Table 338）。
    Note,
    /// `/Reference`：引用（Table 338）。
    Reference,
    /// `/BibEntry`：书目条目（Table 338）。
    BibEntry,
    /// `/Code`：代码（Table 338）。
    Code,
    /// `/Link`：链接（Table 338）。
    Link,
    /// `/Annot`：注解关联（Table 338，PDF 1.5）。
    Annot,
    /// `/Ruby`：注音容器（Table 339，PDF 1.5）。
    Ruby,
    /// `/RB`：注音基文（Table 339，PDF 1.5）。
    RubyBase,
    /// `/RT`：注音文本（Table 339，PDF 1.5）。
    RubyText,
    /// `/RP`：注音标点（Table 339，PDF 1.5）。
    RubyPunctuation,
    /// `/Warichu`：割注容器（Table 339，PDF 1.5）。
    Warichu,
    /// `/WT`：割注文本（Table 339，PDF 1.5）。
    WarichuText,
    /// `/WP`：割注标点（Table 339，PDF 1.5）。
    WarichuPunctuation,
    /// `/Figure`：插图（Table 340）。
    Figure,
    /// `/Formula`：公式（Table 340）。
    Formula,
    /// `/Form`：表单域（Table 340）。
    Form,
    /// 非标准名字（未映射的 `/S` 或映射链末未命中标准名）。
    Other(String),
}

impl StandardStructureType {
    /// 按 PDF 名字精确匹配标准名；未知名字归入 `Other`。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Document" => Self::Document,
            "Part" => Self::Part,
            "Art" => Self::Article,
            "Sect" => Self::Section,
            "Div" => Self::Division,
            "BlockQuote" => Self::BlockQuote,
            "Caption" => Self::Caption,
            "TOC" => Self::TableOfContents,
            "TOCI" => Self::TableOfContentsItem,
            "Index" => Self::Index,
            "NonStruct" => Self::NonStruct,
            "Private" => Self::Private,
            "P" => Self::Paragraph,
            "H" => Self::Heading,
            "H1" => Self::Heading1,
            "H2" => Self::Heading2,
            "H3" => Self::Heading3,
            "H4" => Self::Heading4,
            "H5" => Self::Heading5,
            "H6" => Self::Heading6,
            "L" => Self::List,
            "LI" => Self::ListItem,
            "Lbl" => Self::Label,
            "LBody" => Self::ListBody,
            "Table" => Self::Table,
            "TR" => Self::TableRow,
            "TH" => Self::TableHeader,
            "TD" => Self::TableData,
            "THead" => Self::TableHead,
            "TBody" => Self::TableBody,
            "TFoot" => Self::TableFooter,
            "Span" => Self::Span,
            "Quote" => Self::Quote,
            "Note" => Self::Note,
            "Reference" => Self::Reference,
            "BibEntry" => Self::BibEntry,
            "Code" => Self::Code,
            "Link" => Self::Link,
            "Annot" => Self::Annot,
            "Ruby" => Self::Ruby,
            "RB" => Self::RubyBase,
            "RT" => Self::RubyText,
            "RP" => Self::RubyPunctuation,
            "Warichu" => Self::Warichu,
            "WT" => Self::WarichuText,
            "WP" => Self::WarichuPunctuation,
            "Figure" => Self::Figure,
            "Formula" => Self::Formula,
            "Form" => Self::Form,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 规范名（不含前导 `/`），如 `P`、`H1`、`TOCI`。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Document => "Document",
            Self::Part => "Part",
            Self::Article => "Art",
            Self::Section => "Sect",
            Self::Division => "Div",
            Self::BlockQuote => "BlockQuote",
            Self::Caption => "Caption",
            Self::TableOfContents => "TOC",
            Self::TableOfContentsItem => "TOCI",
            Self::Index => "Index",
            Self::NonStruct => "NonStruct",
            Self::Private => "Private",
            Self::Paragraph => "P",
            Self::Heading => "H",
            Self::Heading1 => "H1",
            Self::Heading2 => "H2",
            Self::Heading3 => "H3",
            Self::Heading4 => "H4",
            Self::Heading5 => "H5",
            Self::Heading6 => "H6",
            Self::List => "L",
            Self::ListItem => "LI",
            Self::Label => "Lbl",
            Self::ListBody => "LBody",
            Self::Table => "Table",
            Self::TableRow => "TR",
            Self::TableHeader => "TH",
            Self::TableData => "TD",
            Self::TableHead => "THead",
            Self::TableBody => "TBody",
            Self::TableFooter => "TFoot",
            Self::Span => "Span",
            Self::Quote => "Quote",
            Self::Note => "Note",
            Self::Reference => "Reference",
            Self::BibEntry => "BibEntry",
            Self::Code => "Code",
            Self::Link => "Link",
            Self::Annot => "Annot",
            Self::Ruby => "Ruby",
            Self::RubyBase => "RB",
            Self::RubyText => "RT",
            Self::RubyPunctuation => "RP",
            Self::Warichu => "Warichu",
            Self::WarichuText => "WT",
            Self::WarichuPunctuation => "WP",
            Self::Figure => "Figure",
            Self::Formula => "Formula",
            Self::Form => "Form",
            Self::Other(name) => name,
        }
    }

    /// 是否为 14.8.4 定义的标准名字（`Other` 返回 false）。
    pub fn is_standard(&self) -> bool {
        !matches!(self, Self::Other(_))
    }

    /// 标准名所属的 14.8.4 分组；`Other` 返回 `None`。
    ///
    /// 按表归属：`Table` 是 BLSE（Table 334）而非表格元素；TR/TH/TD 与
    /// THead/TBody/TFoot 是表格元素（Table 337）；Ruby/Warichu 家族是
    /// 行内元素（Table 339）；Figure/Formula/Form 是插图元素（Table 340）。
    pub fn category(&self) -> Option<StructureTypeCategory> {
        use StructureTypeCategory::{
            BlockLevel, Grouping, Illustration, InlineLevel, TableElement,
        };
        let category = match self {
            Self::Document
            | Self::Part
            | Self::Article
            | Self::Section
            | Self::Division
            | Self::BlockQuote
            | Self::Caption
            | Self::TableOfContents
            | Self::TableOfContentsItem
            | Self::Index
            | Self::NonStruct
            | Self::Private => Grouping,
            Self::Paragraph
            | Self::Heading
            | Self::Heading1
            | Self::Heading2
            | Self::Heading3
            | Self::Heading4
            | Self::Heading5
            | Self::Heading6
            | Self::List
            | Self::ListItem
            | Self::Label
            | Self::ListBody
            | Self::Table => BlockLevel,
            Self::TableRow
            | Self::TableHeader
            | Self::TableData
            | Self::TableHead
            | Self::TableBody
            | Self::TableFooter => TableElement,
            Self::Span
            | Self::Quote
            | Self::Note
            | Self::Reference
            | Self::BibEntry
            | Self::Code
            | Self::Link
            | Self::Annot
            | Self::Ruby
            | Self::RubyBase
            | Self::RubyText
            | Self::RubyPunctuation
            | Self::Warichu
            | Self::WarichuText
            | Self::WarichuPunctuation => InlineLevel,
            Self::Figure | Self::Formula | Self::Form => Illustration,
            Self::Other(_) => return None,
        };
        Some(category)
    }

    /// 该标准名是否自 PDF 1.5 引入（14.8.4 Table 337–339 的 11 个名字）。
    ///
    /// `THead` `TBody` `TFoot`（Table 337）、`Annot`（Table 338）与
    /// `Ruby` `RB` `RT` `RP` `Warichu` `WT` `WP`（Table 339）；其余标准名
    /// 属 Tagged PDF 基线（PDF 1.3）。
    pub fn requires_pdf_1_5(&self) -> bool {
        matches!(
            self,
            Self::TableHead
                | Self::TableBody
                | Self::TableFooter
                | Self::Annot
                | Self::Ruby
                | Self::RubyBase
                | Self::RubyText
                | Self::RubyPunctuation
                | Self::Warichu
                | Self::WarichuText
                | Self::WarichuPunctuation
        )
    }
}

/// 标准结构类型的 14.8.4 分组。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureTypeCategory {
    /// 分组元素（Table 333）。
    Grouping,
    /// 块级结构元素 BLSE（Table 334，含 `/Table` 容器）。
    BlockLevel,
    /// 表格元素（Table 337）。
    TableElement,
    /// 行内元素 ILSE（Table 338 与 Ruby/Warichu Table 339）。
    InlineLevel,
    /// 插图元素（Table 340）。
    Illustration,
}

impl StructureTree {
    /// 结构元素生效的标准类型（14.8.4）。
    ///
    /// 取 `/S` 沿 `/RoleMap` 一或多级映射后的名字（14.8.4.1：PDF 1.5 起
    /// 标准名也先查 `/RoleMap`），再按 14.8.4 的 49 个标准名分类；未命中
    /// 标准名时返回 [`StandardStructureType::Other`]。越界下标返回 `None`。
    pub fn standard_type(&self, index: usize) -> Option<StandardStructureType> {
        let element = self.element(index)?;
        let name = self
            .resolved_role(&element.structure_type)
            .unwrap_or(element.structure_type.as_str());
        Some(StandardStructureType::from_name(name))
    }
}
