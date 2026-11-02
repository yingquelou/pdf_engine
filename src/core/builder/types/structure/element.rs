use super::attributes::{StructureAttribute, UserProperty};
use super::kid::{StructureKid, StructureObjectReference};

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
