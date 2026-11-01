//! 逻辑结构写端模型（ISO 32000-1 14.7.1–14.7.5、14.8.5.2、14.8.5.4–14.8.5.7）。
//!
//! 建模写端可写出的条目：目录 `/MarkInfo`（Table 321）、结构树根
//! `/StructTreeRoot`（Table 322）、结构元素（Table 323）、标记内容引用
//! MCR（Table 324）与结构属性对象 `/A`（14.7.5 Table 327–329，含用户
//! 属性）。标准属性所有者（14.8.5.2 Table 341）与 Layout/List/PrintField/
//! Table 条目（14.8.5.4–14.8.5.7 Table 342–349）由子模块 [`layout`]、
//! [`standard`] 与 [`format_specific`] 提供类型化构造器。
//! 对象引用 OBJR（Table 325）以 [`StructureKid::ObjectReference`] 建模，
//! 引用的对象是已注册的注解；标记内容序列以 MCR 字典
//! （[`StructureKid::MarkedContent`]）或 14.7.4.2 的整数简写
//! （[`StructureKid::InlineMarkedContent`]，须配合元素 `/Pg`）两式建模，
//! `/Stm` `/StmOwn` 不在本模型内。
//!
//! 结构元素以拥有所有权的树形 `Vec` 表达（`StructureKid::Element` 装箱
//! 子元素），与读端的 arena 前序模型不同：写端在 `PdfBuilder::build`
//! 时才展开为前序并分配对象号，调用方按文档顺序构造即可。

use std::collections::BTreeMap;

use crate::core::types::{Array, Dict, Value};

use super::text_string_value;

mod format_specific;
mod layout;
mod standard;
pub use format_specific::{FormatSpecificAttributes, FormatSpecificOwner};
pub use layout::{
    BlockAlign, BorderColor, BorderStyle, BorderStyles, BorderThickness, GlyphOrientationVertical,
    InlineAlign, LayoutAttributes, LineHeight, NumberOrAuto, NumberOrFour, NumberOrNumbers,
    Placement, RubyAlign, RubyPosition, TextAlign, TextDecorationType, WritingMode,
};
pub use standard::{
    ListAttributes, ListNumbering, PrintFieldAttributes, PrintFieldChecked, PrintFieldRole,
    TableAttributes, TableScope,
};

/// 结构树（14.7.2 Table 322）：顶层结构元素 + 根映射。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructureTree {
    /// `/RoleMap`：结构类型 → 标准类型的近似映射；空时不写出。
    pub role_map: BTreeMap<String, String>,
    /// `/ClassMap`：类名 → 属性对象或属性对象数组（14.7.5.2）；空时不
    /// 写出。数组形式把多个属性对象组合成一个属性类。
    pub class_map: BTreeMap<String, Value>,
    /// `/K`：顶层结构元素，按文档顺序。
    pub elements: Vec<StructureElement>,
}

impl StructureTree {
    /// 空结构树：只写出空的 `/StructTreeRoot`。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一条 `/RoleMap` 映射。
    pub fn with_role(
        mut self,
        structure_type: impl Into<String>,
        standard_type: impl Into<String>,
    ) -> Self {
        self.role_map
            .insert(structure_type.into(), standard_type.into());
        self
    }

    /// 追加一条 `/ClassMap` 属性类。
    ///
    /// `attributes` 为属性对象字典/流，或 `Value::Array` 形式的属性对象
    /// 数组（14.7.5.2）。每个属性对象须带合法非空 `/O`。
    pub fn with_class(mut self, name: impl Into<String>, attributes: Value) -> Self {
        self.class_map.insert(name.into(), attributes);
        self
    }

    /// 追加一个顶层结构元素。
    pub fn with_element(mut self, element: StructureElement) -> Self {
        self.elements.push(element);
        self
    }
}

/// 结构属性对象（14.7.5.2 Table 327）。
///
/// `value` 直接作为属性对象写出：字典内联，流提升为间接对象后写引用。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureAttribute {
    /// 属性对象（字典或流，`/O` 为必填非空名字）。
    pub value: Value,
    /// 元素 `/A` 数组中的修订号；`None` 表示缺省 0 且不写出。
    pub revision: Option<u32>,
}

impl StructureAttribute {
    /// 由属性对象字典/流构造（无修订号）。
    pub fn new(value: Value) -> Self {
        Self {
            value,
            revision: None,
        }
    }

    /// 设置数组 `/A` 中的修订号。
    pub fn with_revision(mut self, revision: u32) -> Self {
        self.revision = Some(revision);
        self
    }

    /// 用户属性属性对象（14.7.5.3 Table 328）。
    ///
    /// 生成 `/O /UserProperties` + `/P` 属性数组，顺序与传入一致；
    /// 要求 PDF 1.6，且文档 `/MarkInfo /UserProperties` 为 true。
    pub fn user_properties(properties: Vec<UserProperty>) -> Self {
        let mut array = Array::new();
        for property in properties {
            array = array.push(Value::Dict(property.into_dict()));
        }
        Self::new(Value::Dict(
            Dict::new()
                .insert("O", Value::Name("UserProperties".into()))
                .insert("P", Value::Array(array)),
        ))
    }
}

/// 用户属性（14.7.5.3 Table 329）。
#[derive(Debug, Clone, PartialEq)]
pub struct UserProperty {
    /// `/N`：属性名（文本串，必填非空）。
    pub name: String,
    /// `/V`：属性值（任意值，必填）。
    pub value: Value,
    /// `/F`：属性值的格式化表示（可选文本串）。
    pub formatted: Option<String>,
    /// `/H`：是否隐藏（缺省 false；true 时写出 `/H true`）。
    pub hidden: bool,
}

impl UserProperty {
    /// 新建用户属性：`/N` + `/V`。
    pub fn new(name: impl Into<String>, value: Value) -> Self {
        Self {
            name: name.into(),
            value,
            formatted: None,
            hidden: false,
        }
    }

    /// 设置 `/F` 格式化表示。
    pub fn with_formatted(mut self, formatted: impl Into<String>) -> Self {
        self.formatted = Some(formatted.into());
        self
    }

    /// 设置 `/H` 隐藏标志。
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// 用户属性字典（Table 329）。
    fn into_dict(self) -> Dict {
        let mut dict = Dict::new()
            .insert("N", text_string_value(&self.name))
            .insert("V", self.value);
        if let Some(formatted) = &self.formatted {
            dict = dict.insert("F", text_string_value(formatted));
        }
        if self.hidden {
            dict = dict.insert("H", Value::Bool(true));
        }
        dict
    }
}

/// 结构元素（14.7.2 Table 323）。
///
/// `/Pg`（Table 323，可选、须为页对象的间接引用）以 [`Self::with_page`]
/// 建模并写出；它同时是 `/K` 整数简写内容项的定位页（14.7.4.2：整数
/// MCID 只在序列位于元素 `/Pg` 页面流时合法，见
/// [`Self::with_inline_marked_content`]）。MCR 字典自带的 `/Pg` 覆盖
/// 元素 `/Pg`（Table 324）。`/A` 属性对象（14.7.5）以
/// [`StructureAttribute`] 按声明顺序写出；`/Stm` `/StmOwn` 不在本切片内。
#[derive(Debug, Clone, PartialEq)]
pub struct StructureElement {
    /// `/S`：结构类型（非空 PDF 名字）。
    pub structure_type: String,
    /// `/Pg`：内容项所在页的 0 基页索引；写出为页对象的间接引用。
    pub page: Option<usize>,
    /// `/ID`：元素标识（非空、唯一的 ASCII 字节串）。
    pub id: Option<Vec<u8>>,
    /// `/T`：人类可读标题。
    pub title: Option<String>,
    /// `/Lang`：自然语言标识（PDF 1.4）。
    pub lang: Option<String>,
    /// `/Alt`：替代描述。
    pub alt: Option<String>,
    /// `/E`：缩写展开形式（PDF 1.5）。
    pub expanded: Option<String>,
    /// `/ActualText`：精确替换文本（PDF 1.4）。
    pub actual_text: Option<String>,
    /// `/C`：属性类名与可选修订号，按声明顺序写出。
    pub classes: Vec<(String, Option<u32>)>,
    /// `/A`：属性对象与可选修订号，按声明顺序写出。
    pub attributes: Vec<StructureAttribute>,
    /// `/R`：结构元素修订号；0（缺省）不写出。
    pub revision: u32,
    /// `/K`：子结构元素与内容项，按文档顺序。
    pub kids: Vec<StructureKid>,
}

impl StructureElement {
    /// 新建结构元素：只有 `/S`，其余条目缺省。
    pub fn new(structure_type: impl Into<String>) -> Self {
        Self {
            structure_type: structure_type.into(),
            page: None,
            id: None,
            title: None,
            lang: None,
            alt: None,
            expanded: None,
            actual_text: None,
            classes: Vec::new(),
            attributes: Vec::new(),
            revision: 0,
            kids: Vec::new(),
        }
    }

    /// 设置 `/ID` 元素标识（原始字节）。
    pub fn with_id(mut self, id: impl Into<Vec<u8>>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// 设置 `/T` 标题。
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 设置 `/Lang` 语言（要求 PDF 1.4）。
    pub fn with_lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = Some(lang.into());
        self
    }

    /// 设置 `/Alt` 替代描述。
    pub fn with_alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = Some(alt.into());
        self
    }

    /// 设置 `/E` 缩写展开形式（要求 PDF 1.5）。
    pub fn with_expanded(mut self, expanded: impl Into<String>) -> Self {
        self.expanded = Some(expanded.into());
        self
    }

    /// 设置 `/ActualText` 替换文本（要求 PDF 1.4）。
    pub fn with_actual_text(mut self, actual_text: impl Into<String>) -> Self {
        self.actual_text = Some(actual_text.into());
        self
    }

    /// 追加一个属性类名（无修订号）。
    pub fn with_class(mut self, name: impl Into<String>) -> Self {
        self.classes.push((name.into(), None));
        self
    }

    /// 追加一个带修订号的属性类名。
    pub fn with_class_revision(mut self, name: impl Into<String>, revision: u32) -> Self {
        self.classes.push((name.into(), Some(revision)));
        self
    }

    /// 追加一个 `/A` 属性对象（14.7.5.2）。
    pub fn with_attribute(mut self, attribute: StructureAttribute) -> Self {
        self.attributes.push(attribute);
        self
    }

    /// 追加一个用户属性属性对象（14.7.5.3）；要求 PDF 1.6 且文档
    /// `/MarkInfo /UserProperties` 为 true。
    pub fn with_user_properties(self, properties: Vec<UserProperty>) -> Self {
        self.with_attribute(StructureAttribute::user_properties(properties))
    }

    /// 设置 `/R` 结构元素修订号；0 表示缺省、不写出。
    pub fn with_revision(mut self, revision: u32) -> Self {
        self.revision = revision;
        self
    }

    /// 追加一个子项（子元素或 MCR 内容项）。
    pub fn with_kid(mut self, kid: StructureKid) -> Self {
        self.kids.push(kid);
        self
    }

    /// 追加一个子结构元素。
    pub fn with_element(self, child: StructureElement) -> Self {
        self.with_kid(StructureKid::element(child))
    }

    /// 追加一个标记内容引用（页面 0 基索引 + MCID）。
    ///
    /// 写出为带显式 `/Pg` 的 MCR 字典（14.7.4.2 Table 324），与元素是否
    /// 携带 `/Pg` 无关；整数简写见 [`Self::with_inline_marked_content`]。
    pub fn with_marked_content(self, page: usize, mcid: i64) -> Self {
        self.with_kid(StructureKid::marked_content(page, mcid))
    }

    /// 追加一个整数简写的标记内容标识符（14.7.4.2）。
    ///
    /// 写出为 `/K` 里的裸整数 MCID。规范只允许「序列所在页即元素
    /// `/Pg` 指定页」这一常见场景使用简写，因此本元素必须以
    /// [`Self::with_page`] 携带同一个 `page`；`/Pg` 缺失或页码不一致时
    /// `build` 在任何对象号分配之前返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError)，
    /// 零字节输出。
    pub fn with_inline_marked_content(self, page: usize, mcid: i64) -> Self {
        self.with_kid(StructureKid::inline_marked_content(page, mcid))
    }

    /// 设置 `/Pg`（Table 323）：内容项所在页的 0 基页索引，写出为
    /// 页对象的间接引用。它是 [`Self::with_inline_marked_content`]
    /// 简写项的定位页；页索引越界在 `build` 的对象号分配之前拒绝。
    pub fn with_page(mut self, page: usize) -> Self {
        self.page = Some(page);
        self
    }

    /// 追加一个对象引用内容项（OBJR，Table 325）。
    ///
    /// `annotation` 为 [`PdfBuilder::add_annotation`](crate::core::builder::PdfBuilder::add_annotation)
    /// 返回的注解句柄：写出时该注解字典整体作为结构内容项，经 `/Obj`
    /// 间接引用进入 `/K`，注解字典同时追加 `/StructParent` 父树键。
    pub fn with_object_reference(self, annotation: usize) -> Self {
        self.with_kid(StructureKid::object_reference(annotation))
    }
}

/// 结构元素的 `/K` 子项。
#[derive(Debug, Clone, PartialEq)]
pub enum StructureKid {
    /// 子结构元素（拥有所有权）。
    Element(Box<StructureElement>),
    /// 标记内容引用 MCR（Table 324）：页面 0 基索引 + 标记内容标识符。
    MarkedContent {
        /// 0 基页索引。
        page: usize,
        /// 标记内容标识符（非负）。
        mcid: i64,
    },
    /// 整数简写的标记内容标识符（14.7.4.2）：写出为 `/K` 里的裸整数，
    /// 仅当页面等于元素的 `/Pg`（[`StructureElement::with_page`]）时合法。
    InlineMarkedContent {
        /// 0 基页索引（必须与元素 `/Pg` 一致）。
        page: usize,
        /// 标记内容标识符（非负）。
        mcid: i64,
    },
    /// 对象引用 OBJR（Table 325）：整个 PDF 对象作为内容项。
    ObjectReference {
        /// 注解文档级句柄（[`PdfBuilder::add_annotation`](crate::core::builder::PdfBuilder::add_annotation)）。
        annotation: usize,
    },
}

impl StructureKid {
    /// 以子结构元素构造子项。
    pub fn element(element: StructureElement) -> Self {
        Self::Element(Box::new(element))
    }

    /// 以 MCR 构造内容项子项。
    pub fn marked_content(page: usize, mcid: i64) -> Self {
        Self::MarkedContent { page, mcid }
    }

    /// 以整数简写 MCID 构造内容项子项（14.7.4.2，见
    /// [`StructureKid::InlineMarkedContent`]）。
    pub fn inline_marked_content(page: usize, mcid: i64) -> Self {
        Self::InlineMarkedContent { page, mcid }
    }

    /// 以 OBJR 注解引用构造内容项子项。
    pub fn object_reference(annotation: usize) -> Self {
        Self::ObjectReference { annotation }
    }
}

/// 目录 `/MarkInfo` 标记信息（14.7.1 Table 321，PDF 1.4）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StructureMarkInfo {
    /// `/Marked`：文档是否符合标记内容约定（缺省 false，写出时显式声明）。
    pub marked: bool,
    /// `/UserProperties`：是否含用户属性（PDF 1.6，true 时写出）。
    pub user_properties: bool,
    /// `/Suspects`：是否含可疑标记内容（PDF 1.6，true 时写出）。
    pub suspects: bool,
}

impl StructureMarkInfo {
    /// 全部标志 false 的标记信息。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `/Marked`。
    pub fn with_marked(mut self, marked: bool) -> Self {
        self.marked = marked;
        self
    }

    /// 设置 `/UserProperties`（要求 PDF 1.6）。
    pub fn with_user_properties(mut self, user_properties: bool) -> Self {
        self.user_properties = user_properties;
        self
    }

    /// 设置 `/Suspects`（要求 PDF 1.6）。
    pub fn with_suspects(mut self, suspects: bool) -> Self {
        self.suspects = suspects;
        self
    }
}
