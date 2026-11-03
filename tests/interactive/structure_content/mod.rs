//! 结构标记内容写出 → 解析 → 回读往返（ISO 32000-1 14.8.2、14.8.2.3.1、14.7.4）。
//!
//! 页面内容流以 `/P << /MCID n >> BDC` 标记两段真实内容，结构树用 MCR 子项
//! 声明同一页的 MCID 0 与 1：解析后页面 `/StructParents`、目录 `/ParentTree`
//! 与页上标记内容三者对齐，14.8.2 的归属诊断在写端产物上为空，文本提取
//! 不受标记影响。
//!
//! 另一条往返覆盖 14.8.2.3.1 的 tag suspects：页上混入
//! `/TagSuspect << /TagSuspect /Ordering >> BDC` 且目录写出
//! `/MarkInfo << /Suspects true >>`，回读后属性表形态、`/Suspects` 声明与
//! 归属诊断的空结果同时成立。
//!
//! 第三条往返覆盖 14.7.4.3 的 OBJR 内容项（Table 325）：同一结构元素同时
//! 挂 MCR 与注解对象引用，写出 `/K` 的 `/Type /OBJR` 字典、注解字典的
//! `/StructParent`（Table 326）与父树的单元型条目；回读后 `/Obj` 指向该
//! 注解对象、`/Pg` 与 MCR 同页、`/ParentTreeNextKey` 覆盖页面键 + OBJR 键，
//! 页面 MCID 归属与文本提取不受影响。
//!
//! 第四条起覆盖 14.8.2.2.1 的注解归属条件（14.7.4.3 与 14.7.4.4）：
//! 带 `/AP /N` 常态外观的可见注解被 OBJR 正确声称时七条诊断为空；
//! 去掉声称恰好命中 `VisibleAnnotationNotInStructure`；置 Hidden 位
//! 的注解即使带外观流也不要求入结构。文本提取在三条往返中不变。
//!
//! 第六条往返覆盖 14.7.4.2 的整数 MCID 简写：结构元素写出 `/Pg`，内容项
//! 写成 `/K` 里的裸整数（无 `/Type /MCR`），回读后与页面 `/StructParents`
//! 数组、归属诊断空结果与文本提取同时对齐。
//!
//! 第五条往返覆盖 14.7.4.4 NOTE 的自动 MCID 编号通道
//! （`begin_auto_structure_content`）：按页从 0 稠密递增、跳过显式已占用
//! 值，回读后每页父树数组无 `null` 空洞并与结构树 MCR 对齐，
//! `/ParentTreeNextKey` 严格大于全部在用父树键。
//!
//! 第七条往返覆盖 14.7.4.3 把 OBJR 目标扩展到 XObject：同一页的
//! `/K` 分别以 `/Obj` 指向页资源 `/Im1` 的图像 XObject 与 `/Fm1` 的
//! 表单 XObject，回读后 `/Obj` 与页资源引用一致、`/Pg` 指向渲染页、
//! 两个 XObject 流字典各带一个 `/StructParent`（按目标对象号升序接在
//! 页面键之后且不带 `/StructParents`，Table 326 互斥）、父树单元条目
//! 指向声称它们的元素，页面 `/StructParents` 数组与文本提取不变。
//!
//! 第八条往返覆盖 14.7.4.2 的流形态 MCR（Table 324 `/Stm` `/StmOwn`）：
//! 两个结构元素各以 `/Type /MCR /Stm <间接引用> /MCID 0` 声称 Form
//! XObject 内的标记内容序列，一个直接指向页资源 `/Fm1` 的表单流
//! （键写在该流字典上），另一个以 `/StmOwn` 指向其 `/AP /N` 恰为该
//! 表单流的 Text 注解（键写在注解字典上，`/Pg` 回落注解绑定页）。
//! 回读后 `/Stm`/`/StmOwn` 与页资源、外观引用逐一对齐，容器
//! `/StructParents` 键按容器对象号升序接在页面键 0 之后（1、2），
//! 父树数组条目指向声称元素，页面 MCID 归属与文本提取不受影响。

use super::*;

mod auto_mcid;
mod marked_content;
mod objr_annotation;
mod objr_xobject;
mod stream_mcr;
mod tag_suspect;

/// 带标记内容与一条 `/AP /N` 可见 Text 注解的整页写出。
///
/// `claim` 为真时 P 元素同时以 OBJR 声称该注解（写端会追加
/// `/StructParent` 与父树单元条目）；`hidden` 为真时注解 `/F` 置
/// Hidden 位（Table 165 bit 1）。返回写出的 PDF 字节。
fn build_annotation_appearance_page(claim: bool, hidden: bool) -> Vec<u8> {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, FormXObject, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Tagged line");
    builder.end_structure_content(page);
    let appearance = builder.form_xobject(&FormXObject::new(
        [72.0, 660.0, 92.0, 680.0],
        ContentStream::new(),
    ));
    let mut annotation = PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0])
        .with_contents("Figure note")
        .with_appearance(appearance);
    if hidden {
        annotation = annotation.with_flags(2);
    }
    let handle = builder.add_annotation(page, annotation);
    let mut paragraph = StructureElement::new("P").with_marked_content(page, 0);
    if claim {
        paragraph = paragraph.with_object_reference(handle);
    }
    builder.set_structure_tree(
        StructureTree::new()
            .with_element(StructureElement::new("Document").with_element(paragraph)),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build tagged page with appearance-stream annotation");
    buf
}

/// 页 `page_num` 的资源字典 `/XObject/<name>` 的间接引用。
fn objr_page_xobject_ref(
    doc: &pdf_engine::core::types::Document,
    page_num: i64,
    name: &str,
) -> (i64, i64) {
    let page = doc
        .get_object(page_num)
        .expect("page lookup")
        .expect("page exists")
        .value
        .as_dict()
        .expect("page dict")
        .clone();
    let xobjects = page
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page /Resources")
        .get("XObject")
        .and_then(Value::as_dict)
        .expect("page /Resources /XObject")
        .clone();
    xobjects
        .get(name)
        .and_then(Value::as_ref)
        .unwrap_or_else(|| panic!("资源 {name} 应是间接引用，实际 {xobjects:?}"))
}

/// 对象的字典（直接字典或流字典）：XObject 目标写 `/StructParent` 的位置。
fn objr_object_dict(doc: &pdf_engine::core::types::Document, num: i64) -> Dict {
    let object = doc
        .get_object(num)
        .expect("object lookup")
        .expect("object exists");
    match &object.value {
        Value::Dict(dict) => dict.clone(),
        Value::Stream(dict, _) => dict.clone(),
        other => panic!("对象 {num} 既不是字典也不是流：{}", other.type_name()),
    }
}
