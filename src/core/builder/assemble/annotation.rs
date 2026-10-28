//! 注解字典写出（12.5.1–12.5.5）：通用条目 + `/A` `/Dest` `/Popup` `/FS`
//! 引用解析。

use crate::core::builder::navigation::destination_value;
use crate::core::builder::types::text_string_value;
use crate::core::builder::types::{Action, AnnotationDestination, PdfAnnotation};
use crate::core::reader::navigation::Destination;
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

use super::real_array;

/// 把单个注解写成字典；对象号已由 assemble 主流程按句柄分配。
///
/// `page_refs` 用于把 `/Dest` 与 `GoTo` 动作中的 0 基页号解析为
/// Page 字典间接引用；`annotation_nums` 用于解析 `/Popup` 句柄；
/// `form_nums` 用于把 `/AP` 外观流句柄解析为 Form XObject 流引用；
/// `embedded_nums` 用于把 `/FS` 句柄解析为文件规格引用。
pub(super) fn annotation_dict(
    annot: &PdfAnnotation,
    handle: usize,
    annotation_nums: &[i64],
    page_refs: &[(i64, i64)],
    form_nums: &[i64],
    embedded_nums: &[i64],
) -> Result<Dict, WriteError> {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Annot".into()))
        .insert("Subtype", Value::Name(annot.kind().name().into()))
        .insert("Rect", real_array(&annot.rect()));
    if annot.flags() != 0 {
        dict = dict.insert("F", Value::Integer(i64::from(annot.flags())));
    }
    if let Some(contents) = annot.contents() {
        dict = dict.insert("Contents", text_string_value(contents));
    }
    if let Some(title) = annot.title() {
        dict = dict.insert("T", text_string_value(title));
    }
    if let Some(name) = annot.name() {
        dict = dict.insert("NM", text_string_value(name));
    }
    if let Some(date) = annot.modified_date() {
        dict = dict.insert("M", text_string_value(date));
    }
    if let Some(date) = annot.creation_date() {
        dict = dict.insert("CreationDate", text_string_value(date));
    }
    if let Some(color) = annot.color() {
        dict = dict.insert("C", real_array(color));
    }
    if let Some(color) = annot.interior_color() {
        dict = dict.insert("IC", real_array(color));
    }
    if let Some(opacity) = annot.opacity() {
        dict = dict.insert("CA", Value::Float(opacity));
    }
    if let Some(mode) = annot.blend_mode() {
        dict = dict.insert("BM", Value::Name(mode.into()));
    }
    if let Some(bs) = border_style_dict(annot) {
        dict = dict.insert("BS", Value::Dict(bs));
    }
    if let Some(quad) = annot.quad_points() {
        dict = dict.insert("QuadPoints", real_array(quad));
    }
    if let Some(action) = annot.action() {
        // `GoTo` 的 `/D` 页号在这里解析为间接引用；其余动作沿用
        // `to_dict` 的直接构造。
        let action_dict = match action {
            Action::GoTo(destination) => {
                let value = explicit_destination_value(destination, page_refs)?;
                Dict::new()
                    .insert("S", Value::Name("GoTo".into()))
                    .insert("D", value)
            }
            other => other.to_dict(),
        };
        dict = dict.insert("A", Value::Dict(action_dict));
    }
    if let Some(destination) = annot.destination() {
        let value = annotation_destination_value(destination, page_refs)?;
        dict = dict.insert("Dest", value);
    }
    if let Some(popup) = annot.popup() {
        let num = annotation_nums.get(popup).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /Popup references unknown annotation handle {popup}"
            ))
        })?;
        dict = dict.insert("Popup", Value::Ref(*num, 0));
    }
    if let Some(target) = annot.filespec() {
        let num = embedded_nums.get(target).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /FS references unknown embedded file handle {target}"
            ))
        })?;
        dict = dict.insert("FS", Value::Ref(*num, 0));
    }
    let mut appearance = Dict::new();
    for (key, form) in [
        ("N", annot.appearance_normal()),
        ("D", annot.appearance_down()),
        ("R", annot.appearance_rollover()),
    ] {
        if let Some(form) = form {
            let num = form_nums.get(form).ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "annotation handle {handle}: /AP /{key} references unknown form handle {form}"
                ))
            })?;
            appearance = appearance.insert(key, Value::Ref(*num, 0));
        }
    }
    if !appearance.0.is_empty() {
        dict = dict.insert("AP", Value::Dict(appearance));
    }
    Ok(dict)
}

/// `/BS` 边框样式字典：`/W` `/S` `/D` 仅在提供时写出；三项全空返回 `None`。
fn border_style_dict(annot: &PdfAnnotation) -> Option<Dict> {
    if annot.border_width().is_none()
        && annot.border_style().is_none()
        && annot.border_dash().is_empty()
    {
        return None;
    }
    let mut dict = Dict::new();
    if let Some(width) = annot.border_width() {
        dict = dict.insert("W", Value::Float(width));
    }
    if let Some(style) = annot.border_style() {
        dict = dict.insert("S", Value::Name(style.name().into()));
    }
    if !annot.border_dash().is_empty() {
        dict = dict.insert("D", real_array(annot.border_dash()));
    }
    Some(dict)
}

/// 把 [`AnnotationDestination`] 写成 `/Dest` 值（显式数组或命名文本串）。
fn annotation_destination_value(
    destination: &AnnotationDestination,
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    match destination {
        AnnotationDestination::Explicit(destination) => {
            explicit_destination_value(destination, page_refs)
        }
        AnnotationDestination::Named(name) => Ok(text_string_value(name)),
    }
}

/// 显式目标数组：先校验 0 基页号在页集合内，再交给导航层解析为引用。
fn explicit_destination_value(
    destination: &Destination,
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    if destination.page() >= page_refs.len() {
        return Err(WriteError::InvalidObject(format!(
            "annotation destination references unknown page {}",
            destination.page()
        )));
    }
    destination_value(destination, page_refs)
}
