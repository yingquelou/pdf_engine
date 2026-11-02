use super::element::StructureElement;

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
