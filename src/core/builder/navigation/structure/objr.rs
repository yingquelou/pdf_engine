//! OBJR 的 `/Obj` 目标按 14.7.4.3 扩展到 XObject。规范原文（14.7.4.3，
//! raw 文本 29609–29612 行）是 shall：「When a structure element's content
//! includes an entire PDF object, such as an XObject or an annotation, that
//! is associated with a page but not directly included in the page's content
//! stream, the object shall be identified in the structure element's K entry
//! by an object reference dictionary」，故目标不限于注解：
//! [`StructureObjectReference`] 给出三档——注解字典、文档级 Form XObject、
//! 页面 XObject 资源里的图像 XObject。
//!
//! 父树键归属（shall）：14.7.4.4 说承载键的条目「may appear in the page
//! object of a page containing marked-content sequences, in the stream
//! dictionary of a form or image XObject, in an annotation dictionary」（raw
//! 29656–29660 行），且 Table 326 第一行 `/StructParent`「Required for all
//! objects that are structural content items」（raw 29692 行）——三种目标
//! 都在自身字典（Form/图像是流字典）追加 `/StructParent`，与规范 Example 1
//! 的 Form XObject `/StructParent 6`（raw 29706 行）一致；父树条目是裸结构
//! 元素引用（14.7.4.4「the value shall be an indirect reference to the
//! parent structure element」）。`/StructParents` 只属于「content stream
//! containing marked-content sequences」（raw 29694 行）：页面内容流走页
//! 字典通道；流形态 MCR（14.7.4.2/14.7.4.4）按 shall「its parent tree key
//! shall be found in the StructParents entry of the page object or other
//! content stream in which the sequence resides」（raw 29722–29723 行）把
//! 键追加在序列所在流的所有者字典上——无 `/StmOwn` 时即 `/Stm`（Form
//! XObject）流字典自身，带 `/StmOwn` 时是该所有者对象字典。
//! 互斥校验按 Table 326 的 shall「At most one of these two entries shall be
//! present in a given object. An object may be either a content item in its
//! entirety or a container for marked-content sequences that are content
//! items, but not both」（raw 29697–29699 行）与 Table 95 对 Form XObject
//! 的同款 shall（raw 12142–12145 行）执行。
//!
//! 库策略（规范未强制，比 shall 更严或更窄，理由写在括号里）：
//! - 目标可寻址范围限于「写端能确定渲染页」的对象：Form 只能是
//!   [`PdfBuilder::draw_form`](crate::core::builder::PdfBuilder::draw_form)
//!   登记进页资源 `/Fm<n>` 的表单，图像只能是页资源 `/Im<n+1>` 里的图像；
//!   只作为注解 `/AP`、`/MK` 图标或表单内嵌资源出现的 XObject 暂不可作
//!   OBJR 目标（写端无法为它们确定 `/Pg`）。
//! - 所声称的 `/Pg`（引用给出或沿用元素 `/Pg`）必须真的绘制该表单，否则
//!   拒绝（Table 325 `/Pg`「the page on which the object shall be
//!   rendered」，raw 29642–29645 行）。
//! - 判重按**解析后的对象号**，故同一图像跨页去重共享同一对象时，第二次
//!   声称算重复声称（Table 322「Each integer key in the parent tree shall
//!   correspond to a single page of the document or to an individual object
//!   …」+ 14.7.4.4 的单元条目只能指向一个父元素）。
//! - 同一 Form/图像 XObject 在**多页**绘制时只允许一个元素声称它：14.7.4.3
//!   NOTE 2（raw 29616 行）要求每次渲染各用一个对象引用，而本写端每个表单
//!   句柄、每页每个图像资源序号只出一个对象号，无法为第二次渲染另出对象，
//!   故宁可拒绝也不写出「一个 `/StructParent` 指向两个父元素」的产物；同一页
//!   多次绘制同一表单只需一次声称（NOTE 2 后一句），不额外要求。
use std::collections::BTreeSet;

use crate::core::builder::navigation::invalid;
use crate::core::types::{Dict, Document, Object, Value};
use crate::core::writer::WriteError;

use super::flatten::Flattener;
use super::plan::Plan;
use crate::core::builder::types::StructureObjectReference;
/// OBJR 目标（14.7.4.3 [`StructureObjectReference`]）的解析表。
///
/// 句柄 → 对象号/页索引的映射由 `PdfBuilder::build` 在对象号分配之后
/// 传入；全部字段为空切片时只接受不带 OBJR 子项的结构树。
#[derive(Debug, Default, Clone, Copy)]
pub(in crate::core::builder) struct StructureTargets<'a> {
    /// 注解句柄 → 注解对象号（长度即注解句柄上界）。
    pub(in crate::core::builder) annotation_nums: &'a [i64],
    /// 注解句柄 → 注解绑定的 0 基页索引（与 `annotation_nums` 同长）。
    pub(in crate::core::builder) annotation_pages: &'a [usize],
    /// Form XObject 句柄 → 表单流对象号。
    pub(in crate::core::builder) form_nums: &'a [i64],
    /// Form XObject 句柄 → 绘制它的页索引（升序，去重后）。
    pub(in crate::core::builder) form_pages: &'a [Vec<usize>],
    /// 页索引 → 该页图像 XObject 对象号（序号即 `/Im<序号+1>` 的 0 基序号）。
    pub(in crate::core::builder) image_nums: &'a [Vec<i64>],
}

impl<'a> Flattener<'a> {
    /// 把一个 OBJR 目标（Table 325）解析为「目标对象号 + `/Pg` 页索引」。
    ///
    /// 三档目标的定位方式不同：注解句柄带自己的绑定页；Form XObject 是
    /// 文档级句柄，绘制页由引用给出或沿用元素 `/Pg`（Table 325 的 `/Pg`
    /// 「overrides any Pg entry in the structure element containing the
    /// object reference; it shall be used if the structure element has no
    /// such entry」）；图像 XObject 以「页 + 页内资源序号」定位，该页即
    /// 渲染页。句柄/页/序号越界、两路都取不到页、以及所声称的页并没有
    /// 绘制该表单（页资源 `/Fm<n>` 登记表里没有它）一律拒绝——Table 325
    /// 的 `/Pg` 是「the page on which the object shall be rendered」，
    /// 谎报渲染页的引用写出后读端与归属诊断都会误判。
    pub(super) fn resolve_objr(
        &self,
        object: &StructureObjectReference,
        element_page: Option<usize>,
    ) -> Result<(i64, usize), WriteError> {
        match object {
            StructureObjectReference::Annotation(handle) => {
                if *handle >= self.targets.annotation_nums.len() {
                    return Err(invalid(format!(
                        "structure OBJR annotation handle {handle} is out of range (count {})",
                        self.targets.annotation_nums.len()
                    )));
                }
                let Some(&page) = self.targets.annotation_pages.get(*handle) else {
                    return Err(invalid(format!(
                        "structure OBJR annotation handle {handle} is out of range (count {})",
                        self.targets.annotation_pages.len()
                    )));
                };
                if page >= self.page_count {
                    return Err(invalid(format!(
                        "structure OBJR annotation handle {handle} is attached to page {page} out of range (page count {})",
                        self.page_count
                    )));
                }
                Ok((self.targets.annotation_nums[*handle], page))
            }
            StructureObjectReference::FormXObject { handle, page } => {
                let Some(&num) = self.targets.form_nums.get(*handle) else {
                    return Err(invalid(format!(
                        "structure OBJR form XObject handle {handle} is out of range (count {})",
                        self.targets.form_nums.len()
                    )));
                };
                let page = match page {
                    Some(page) => {
                        if *page >= self.page_count {
                            return Err(invalid(format!(
                                "structure OBJR form XObject handle {handle} names page {page} out of range (page count {})",
                                self.page_count
                            )));
                        }
                        *page
                    }
                    // 未显式给页：沿用元素的 `/Pg`（已在 `Flattener::add`
                    // 校验过页范围），两路都没有就无从写出 `/Pg`。
                    None => element_page.ok_or_else(|| {
                        invalid(format!(
                            "structure OBJR form XObject handle {handle} needs a page: the reference gives no /Pg and the element has no /Pg (Table 325)"
                        ))
                    })?,
                };
                // 无论页来自哪一路，都必须是真正绘制该表单的页：该页的
                // `/Fm<n>` 资源里没有它，`/Pg` 就是在谎报渲染页。
                let drawn = self
                    .targets
                    .form_pages
                    .get(*handle)
                    .is_some_and(|pages| pages.contains(&page));
                if !drawn {
                    return Err(invalid(format!(
                        "structure OBJR form XObject handle {handle} is not drawn on page {page} (Table 325: /Pg is the page on which the object is rendered)"
                    )));
                }
                Ok((num, page))
            }
            StructureObjectReference::ImageXObject { page, index } => {
                let Some(nums) = self.targets.image_nums.get(*page) else {
                    return Err(invalid(format!(
                        "structure OBJR image XObject page {page} is out of range (page count {})",
                        self.page_count
                    )));
                };
                let Some(&num) = nums.get(*index) else {
                    return Err(invalid(format!(
                        "structure OBJR image XObject index {index} on page {page} is out of range (count {})",
                        nums.len()
                    )));
                };
                Ok((num, *page))
            }
        }
    }
}
/// OBJR 目标在错误信息里的称呼：类别名取自
/// [`StructureObjectReference::kind_name`]，再附上定位该目标所需的句柄
/// 或「页 + 页内资源序号」，与既有注解文案保持一致。
pub(super) fn describe_objr_target(object: &StructureObjectReference) -> String {
    let kind = object.kind_name();
    match object {
        StructureObjectReference::Annotation(handle)
        | StructureObjectReference::FormXObject { handle, .. } => {
            format!("{kind} handle {handle}")
        }
        StructureObjectReference::ImageXObject { page, index } => {
            format!("{kind} page {page} index {index}")
        }
    }
}

/// 字典（或流字典）条目的可读类型名。
fn target_dict(value: &Value) -> Option<(&Dict, bool)> {
    match value {
        Value::Dict(dict) => Some((dict, false)),
        Value::Stream(dict, _) => Some((dict, true)),
        _ => None,
    }
}

/// 校验每个 OBJR 目标对象（Table 325/326）：仍在结构对象号分配之前。
///
/// 规则：目标对象必须存在且是字典或流字典；注解目标不得是流，Form/图像
/// 目标必须带匹配的 `/Subtype`（所声称的类别）；目标字典不得已带
/// `/StructParent`（重复声称）或 `/StructParents`（Table 326 shall
/// 「At most one of these two entries shall be present in a given object」
/// ——`/StructParents` 属于「含标记内容序列的内容流」，本写端只有页字典
/// 走该通道，故目标对象号落在带 `/StructParents` 的页上也拒绝）。
pub(super) fn validate_objr_targets(
    doc: &Document,
    plan: &Plan<'_>,
    page_refs: &[(i64, i64)],
) -> Result<(), WriteError> {
    // 将要写出 `/StructParents` 的页对象号（14.7.4.4 的内容流宿主）。
    let container_nums: BTreeSet<i64> = plan
        .page_keys
        .keys()
        .map(|page| page_refs[*page].0)
        .collect();
    let objects = doc.objects_for_write();
    for (object_num, (element_index, target)) in &plan.objrs {
        let describe = describe_objr_target(target);
        let object = objects
            .iter()
            .find(|item| item.num == *object_num && item.generation == 0)
            .ok_or_else(|| {
                invalid(format!(
                    "structure OBJR {describe} object {object_num} 0 is missing"
                ))
            })?;
        let (dict, is_stream) = target_dict(&object.value).ok_or_else(|| {
            invalid(format!(
                "structure OBJR {describe} object {object_num} 0 is not a dictionary"
            ))
        })?;
        if container_nums.contains(object_num) {
            return Err(invalid(format!(
                "structure OBJR {describe} object {object_num} 0 is also a content stream carrying /StructParents; at most one of /StructParent and /StructParents may appear in an object (Table 326)"
            )));
        }
        // 所声称类别与对象形态必须一致（Table 325 的 `/Obj` 引的是哪类对象）。
        match target {
            StructureObjectReference::Annotation(_) => {
                if is_stream {
                    return Err(invalid(format!(
                        "structure OBJR {describe} object {object_num} 0 is a stream, not an annotation dictionary (Table 325)"
                    )));
                }
            }
            StructureObjectReference::FormXObject { .. }
            | StructureObjectReference::ImageXObject { .. } => {
                let expected = if matches!(target, StructureObjectReference::FormXObject { .. }) {
                    "Form"
                } else {
                    "Image"
                };
                if !is_stream || dict.get("Subtype").and_then(Value::as_name) != Some(expected) {
                    return Err(invalid(format!(
                        "structure OBJR {describe} object {object_num} 0 is not a /Subtype /{expected} XObject stream (Table 325)"
                    )));
                }
            }
        }
        for entry in ["StructParent", "StructParents"] {
            if dict.get(entry).is_some() {
                return Err(invalid(format!(
                    "structure OBJR {describe} object {object_num} 0 already has /{entry} (element {element_index} claims it); at most one of /StructParent and /StructParents may appear in an object (Table 326)"
                )));
            }
        }
    }
    Ok(())
}

/// OBJR 目标字典追加 `/StructParent`（Table 326；14.7.4.4 的对象引用子项）。
///
/// 注解字典与 Form/图像 XObject 的流字典同一路径：整对象作为内容项时
/// 规范写的都是 `/StructParent`（14.7.4.4 的 Example 1 即 Form XObject 带
/// `/StructParent`），流形态只替换字典、字节数据原样保留（`/Length` 由
/// 写出阶段按实际字节重算）。
pub(super) fn write_object_parents(doc: &mut Document, plan: &Plan<'_>) -> Result<(), WriteError> {
    for (object_num, key) in &plan.objr_keys {
        let objects = doc.objects_for_write();
        let object = objects
            .iter()
            .find(|item| item.num == *object_num && item.generation == 0)
            .ok_or_else(|| invalid(format!("structure OBJR object {object_num} 0 is missing")))?;
        let mut dict = match &object.value {
            Value::Dict(dict) => dict.clone(),
            Value::Stream(dict, data) => {
                let mut dict = dict.clone();
                dict = dict.insert("StructParent", Value::Integer(*key));
                doc.set_object(Object::new(
                    *object_num,
                    0,
                    Value::Stream(dict, data.clone()),
                ));
                continue;
            }
            _ => {
                return Err(invalid(format!(
                    "structure OBJR object {object_num} 0 is not a dictionary"
                )));
            }
        };
        dict = dict.insert("StructParent", Value::Integer(*key));
        doc.set_object(Object::new(*object_num, 0, Value::Dict(dict)));
    }
    Ok(())
}
