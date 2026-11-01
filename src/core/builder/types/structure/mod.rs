//! 逻辑结构写端模型（ISO 32000-1 14.7.1–14.7.5、14.8.5.2、14.8.5.4–14.8.5.7）。
//!
//! 建模写端可写出的条目：目录 `/MarkInfo`（Table 321）、结构树根
//! `/StructTreeRoot`（Table 322）、结构元素（Table 323）、标记内容引用
//! MCR（Table 324）与结构属性对象 `/A`（14.7.5 Table 327–329，含用户
//! 属性）。标准属性所有者（14.8.5.2 Table 341）与 Layout/List/PrintField/
//! Table 条目（14.8.5.4–14.8.5.7 Table 342–349）由子模块 [`layout`]、
//! [`standard`] 与 [`format_specific`] 提供类型化构造器。
//! 对象引用 OBJR（Table 325）以 [`StructureKid::ObjectReference`] 建模，
//! `/Obj` 的目标类别是 [`StructureObjectReference`]：注解字典、文档级 Form
//! XObject 与页面资源里的图像 XObject（14.7.4.3 的「an entire PDF object,
//! such as an XObject or an annotation」）；标记内容序列以 MCR 字典
//! （[`StructureKid::MarkedContent`]）或 14.7.4.2 的整数简写
//! （[`StructureKid::InlineMarkedContent`]，须配合元素 `/Pg`）两式建模；
//! 序列位于页面内容流之外的流形态 MCR（Table 324 的 `/Stm` `/StmOwn`，
//! 14.7.4.2 EXAMPLE 5）以 [`StructureKid::StreamMarkedContent`] 建模。
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
/// [`StructureAttribute`] 按声明顺序写出；流形态 MCR 的 `/Stm`
/// `/StmOwn` 见 [`StructureKid::StreamMarkedContent`]。
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

    /// 追加一个流形态标记内容引用（MCR `/Stm`，14.7.4.2 Table 324
    /// EXAMPLE 5）。
    ///
    /// `stream` 为 [`PdfBuilder::form_xobject`](crate::core::builder::PdfBuilder::form_xobject)
    /// 的返回值——写端目前唯一可解析成间接对象号的内容流来源；`mcid` 是
    /// 序列在该流内的标记内容标识符。默认不带 `/StmOwn`、`/Pg` 沿用元素
    /// `/Pg`（Table 324：两者皆无时 `build` 在任何对象号分配之前返回
    /// [`WriteError::InvalidObject`](crate::core::writer::WriteError)，
    /// 零字节）。需要 `/StmOwn`（如外观流的持有注解）或覆盖 `/Pg` 时直接
    /// 构造 [`StructureKid::StreamMarkedContent`] 变体并经
    /// [`Self::with_kid`] 挂上。
    pub fn with_stream_marked_content(self, stream: usize, mcid: i64) -> Self {
        self.with_kid(StructureKid::stream_marked_content(stream, mcid))
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
    /// 目标以 [`StructureObjectReference`] 给出：注解句柄、文档级 Form
    /// XObject 句柄或某页资源中的图像 XObject（14.7.4.3 允许「an entire
    /// PDF object, such as an XObject or an annotation」作为内容项）。
    /// 写出时该对象整体经 `/Obj` 间接引用进入 `/K`，其字典同时追加
    /// `/StructParent` 父树键（Table 326）。
    ///
    /// 传裸 `usize` 即注解句柄（[`PdfBuilder::add_annotation`](crate::core::builder::PdfBuilder::add_annotation)
    /// 的返回值），与既有写法一致。
    pub fn with_object_reference(self, object: impl Into<StructureObjectReference>) -> Self {
        self.with_kid(StructureKid::object_reference(object))
    }

    /// 追加一个 Form XObject 的对象引用内容项（14.7.4.3）。
    ///
    /// `handle` 为 [`PdfBuilder::form_xobject`](crate::core::builder::PdfBuilder::form_xobject)
    /// 的返回值；`page` 为该次绘制所在页的 0 基索引（Table 325 NOTE 2：
    /// 「If the referenced object is rendered on multiple pages, each
    /// rendering requires a separate object reference」）。`None` 表示沿用
    /// 元素自身的 `/Pg`。
    pub fn with_form_xobject_reference(self, handle: usize, page: Option<usize>) -> Self {
        self.with_object_reference(StructureObjectReference::FormXObject { handle, page })
    }

    /// 追加一个图像 XObject 的对象引用内容项（14.7.4.3）。
    ///
    /// 目标由「页 0 基索引 + 该页图像资源的 0 基序号」定位——即
    /// [`PdfBuilder::image`](crate::core::builder::PdfBuilder::image) 在该页
    /// 第几次调用（`/Im<序号+1>`）；`/Pg` 即该页。
    pub fn with_image_xobject_reference(self, page: usize, index: usize) -> Self {
        self.with_object_reference(StructureObjectReference::ImageXObject { page, index })
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
        /// `/Obj` 的目标类别（注解 / Form XObject / 图像 XObject）。
        object: StructureObjectReference,
    },
    /// 流形态标记内容引用 MCR（Table 324，14.7.4.2 EXAMPLE 5）：序列
    /// 位于页面内容流以外的内容流（`/Stm`），可选其所有者对象
    /// （`/StmOwn`）。
    ///
    /// 规范原文（Table 324，`/Stm`）是 should + shall：「This entry
    /// should be present only if the marked-content sequence resides in a
    /// content stream other than the content stream for the page」与
    /// 「(Optional; shall be an indirect reference) The content stream
    /// containing the marked-content sequence」；`/StmOwn` 是 shall：
    /// 「(Optional; shall be an indirect reference) The PDF object owning
    /// the stream identified by Stm; … annotation to which an appearance
    /// stream belongs」。`/MCID` 在 Table 324 中标注 Required，故本变体
    /// 的 `mcid` 保持必填。
    StreamMarkedContent {
        /// `/Stm` 目标：文档级 Form XObject 句柄
        /// （[`PdfBuilder::form_xobject`](crate::core::builder::PdfBuilder::form_xobject)
        /// 的返回值）；写出为其内容流的间接引用。
        stream: usize,
        /// `/StmOwn` 目标：持有该外观流的注解句柄
        /// （[`PdfBuilder::add_annotation`](crate::core::builder::PdfBuilder::add_annotation)
        /// 的返回值）；`None` 不写出。给出时构建期核对该注解的 `/AP`
        /// 确实引用 `/Stm` 的流对象，且引用未显式给 `/Pg` 时以注解绑定页
        /// 定位（详见 `navigation/structure.rs` 的范围说明）。
        owner: Option<usize>,
        /// `/Pg` 的 0 基页索引；`None` 时依次回落 `/StmOwn` 注解的绑定页
        /// 与元素 `/Pg`（Table 324：「This entry overrides any Pg entry in
        /// the structure element containing the marked-content reference;
        /// it shall be required if the structure element has no such
        /// entry」）。
        page: Option<usize>,
        /// 标记内容标识符（非负；Table 324 标注 Required）。
        mcid: i64,
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

    /// 以流形态 MCR 构造内容项子项（Table 324 `/Stm`，见
    /// [`StructureKid::StreamMarkedContent`]）；不带 `/StmOwn`，`/Pg`
    /// 沿用元素 `/Pg`。
    pub fn stream_marked_content(stream: usize, mcid: i64) -> Self {
        Self::StreamMarkedContent {
            stream,
            owner: None,
            page: None,
            mcid,
        }
    }

    /// 以 OBJR 对象引用构造内容项子项。
    ///
    /// 传裸注解句柄（`usize`）自动按 [`StructureObjectReference::Annotation`]
    /// 解释；Form/图像 XObject 传 [`StructureObjectReference`]。
    pub fn object_reference(object: impl Into<StructureObjectReference>) -> Self {
        Self::ObjectReference {
            object: object.into(),
        }
    }
}

/// OBJR 内容项（14.7.4.3 Table 325）`/Obj` 引用的目标对象类别。
///
/// 规范原文：「When a structure element's content includes an entire PDF
/// object, such as an XObject or an annotation, that is associated with a
/// page but not directly included in the page's content stream, the object
/// shall be identified in the structure element's K entry by an object
/// reference dictionary」——故目标不限于注解，Form XObject 与图像 XObject
/// 同样可作内容项。三种目标都按 Table 326 的第一行在自身字典写出
/// `/StructParent`（规范 Example 1 的 Form XObject 即写
/// `/StructParent`），而不是只用于「含标记内容序列的内容流」的
/// `/StructParents`：一个对象不得同时带两个条目（Table 326 shall：
/// 「At most one of these two entries shall be present in a given
/// object」，Table 95 对 Form XObject 另有同款 shall）。
///
/// 库策略：可寻址目标限于「写端能确定渲染页」的对象——Form 必须已被
/// [`PdfBuilder::draw_form`](crate::core::builder::PdfBuilder::draw_form)
/// 登记进某页的 `/Fm<n>` 资源，图像必须是某页 `/Im<n+1>` 资源里的图像；
/// 只作为注解 `/AP`、`/MK` 图标或表单内嵌资源出现的 XObject 暂不可作
/// OBJR 目标。所声称的页若并未绘制该表单，构建期拒绝（详见
/// `navigation/structure.rs` 的范围说明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureObjectReference {
    /// 注解字典（[`PdfBuilder::add_annotation`](crate::core::builder::PdfBuilder::add_annotation)
    /// 返回的文档级句柄）；`/Pg` 取注解绑定的页。
    Annotation(usize),
    /// 文档级 Form XObject（[`PdfBuilder::form_xobject`](crate::core::builder::PdfBuilder::form_xobject)
    /// 返回的句柄）。
    FormXObject {
        /// Form XObject 文档级句柄。
        handle: usize,
        /// 该次绘制所在页的 0 基索引；`None` 时沿用元素 `/Pg`
        /// （Table 325 `/Pg`：「This entry overrides any Pg entry in the
        /// structure element containing the object reference; it shall be
        /// used if the structure element has no such entry」）。
        page: Option<usize>,
    },
    /// 页面资源字典里的图像 XObject。
    ImageXObject {
        /// 图像所在页的 0 基页索引（即 `/Pg`）。
        page: usize,
        /// 该页图像资源的 0 基序号：[`PdfBuilder::image`](crate::core::builder::PdfBuilder::image)
        /// 在该页的第几次调用（资源名 `/Im<序号+1>`）。
        index: usize,
    },
}

impl StructureObjectReference {
    /// 以注解句柄构造目标。
    pub fn annotation(handle: usize) -> Self {
        Self::Annotation(handle)
    }

    /// 以 Form XObject 句柄构造目标（`/Pg` 沿用元素 `/Pg`）。
    pub fn form_xobject(handle: usize) -> Self {
        Self::FormXObject { handle, page: None }
    }

    /// 以 Form XObject 句柄 + 绘制页构造目标。
    pub fn form_xobject_on_page(handle: usize, page: usize) -> Self {
        Self::FormXObject {
            handle,
            page: Some(page),
        }
    }

    /// 以页索引 + 页内图像序号构造目标。
    pub fn image_xobject(page: usize, index: usize) -> Self {
        Self::ImageXObject { page, index }
    }

    /// 错误信息里用的目标类别名。
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Annotation(_) => "annotation",
            Self::FormXObject { .. } => "form XObject",
            Self::ImageXObject { .. } => "image XObject",
        }
    }
}

impl From<usize> for StructureObjectReference {
    /// 裸句柄按注解解释，保持既有 `with_object_reference(annotation)` 写法。
    fn from(handle: usize) -> Self {
        Self::Annotation(handle)
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
