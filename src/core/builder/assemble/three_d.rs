//! 3D artwork 与 3D 注解条目写出（13.6 Table 298/300/303）。
//!
//! 每个通过 `add_three_d_artwork` 注册的 artwork 写成独立间接流对象，
//! 注解 `/3DD` 引用其对象号；`/3DV` 的下标、`/IN` 名称与 `F`/`L` 在此
//! 相对该 artwork 的 `/VA` 校验。启用 `with_reference_dictionary` 时另写
//! 一个 Table 303 引用字典（`/Type /3DRef` + `/3DD` 流引用），注解改指该
//! 字典以共享运行实例；`with_on_instantiate` 的 JavaScript 写成独立间接
//! 流，由 3D 流字典 `/OnInstantiate` 引用。3D 特性自 PDF 1.6 引入，版本
//! 门禁与取值校验都在任何对象写出前完成（零字节输出契约）。

use crate::core::builder::types::{PdfAnnotation, ThreeDArtwork};
use crate::core::types::{Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Value};
use crate::core::writer::WriteError;

use super::real_array;

/// 注册校验：PDF 1.6 版本门禁与每个 artwork 的流/视图取值。
pub(super) fn validate_registration(
    artworks: &[ThreeDArtwork],
    version: PdfVersion,
) -> Result<(), WriteError> {
    if artworks.is_empty() {
        return Ok(());
    }
    if !version.supports(PdfFeature::ThreeDAnnotations) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::ThreeDAnnotations,
            PdfFeature::ThreeDAnnotations.min_version().as_str(),
            version.as_str()
        )));
    }
    for (handle, artwork) in artworks.iter().enumerate() {
        artwork.validate().map_err(|message| {
            WriteError::InvalidObject(format!("3D artwork handle {handle}: {message}"))
        })?;
    }
    Ok(())
}

/// 3D 相关对象号（下标即 artwork 句柄）。
pub(super) struct ThreeDObjectSlots {
    /// 注解 `/3DD` 的引用目标：引用字典号或流号。
    pub(super) annotation_nums: Vec<i64>,
    /// 3D 流（Table 300）对象号。
    stream_nums: Vec<i64>,
    /// 3D 引用字典（Table 303）对象号；未启用时为 `None`。
    reference_nums: Vec<Option<i64>>,
    /// `/OnInstantiate` JavaScript 流对象号；未声明时为 `None`。
    on_instantiate_nums: Vec<Option<i64>>,
}

fn take(next: &mut i64) -> i64 {
    let number = *next;
    *next += 1;
    number
}

/// 为每个已注册 artwork 按句柄顺序分配对象号：3D 流先行，其后是
/// `/OnInstantiate` 流与 Table 303 引用字典（存在时）。
pub(super) fn allocate(artworks: &[ThreeDArtwork], next: &mut i64) -> ThreeDObjectSlots {
    let mut slots = ThreeDObjectSlots {
        annotation_nums: Vec::with_capacity(artworks.len()),
        stream_nums: Vec::with_capacity(artworks.len()),
        reference_nums: Vec::with_capacity(artworks.len()),
        on_instantiate_nums: Vec::with_capacity(artworks.len()),
    };
    for artwork in artworks {
        let stream_num = take(next);
        let on_instantiate_num = artwork.on_instantiate().map(|_| take(next));
        let reference_num = artwork.wants_reference_dictionary().then(|| take(next));
        slots
            .annotation_nums
            .push(reference_num.unwrap_or(stream_num));
        slots.stream_nums.push(stream_num);
        slots.reference_nums.push(reference_num);
        slots.on_instantiate_nums.push(on_instantiate_num);
    }
    slots
}

/// 写出 3D 流、`/OnInstantiate` JavaScript 流与 3D 引用字典，并登记
/// [`PdfFeature::ThreeDAnnotations`]。
pub(super) fn write_objects(
    doc: &mut Document,
    artworks: &[ThreeDArtwork],
    slots: &ThreeDObjectSlots,
) {
    for (index, artwork) in artworks.iter().enumerate() {
        let stream_num = slots.stream_nums[index];
        doc.set_object(Object::new(
            stream_num,
            0,
            artwork.to_stream_value(slots.on_instantiate_nums[index]),
        ));
        doc.features
            .borrow_mut()
            .insert(PdfFeature::ThreeDAnnotations);
        if let (Some(number), Some(script)) =
            (slots.on_instantiate_nums[index], artwork.on_instantiate())
        {
            doc.set_object(Object::new(
                number,
                0,
                Value::Stream(
                    Dict::new().insert("Length", Value::Integer(script.len() as i64)),
                    StreamData::Owned(script.to_vec()),
                ),
            ));
        }
        if let Some(number) = slots.reference_nums[index] {
            doc.set_object(Object::new(
                number,
                0,
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("3DRef".into()))
                        .insert("3DD", Value::Ref(stream_num, 0)),
                ),
            ));
        }
    }
}

/// 把 3D 注解的 `/3DD` `/3DV` `/3DA` `/3DI` `/3DB` 写入注解字典。
pub(super) fn write_annotation_entries(
    annotation: &PdfAnnotation,
    handle: usize,
    artwork_nums: &[i64],
    artworks: &[ThreeDArtwork],
    dict: &mut Dict,
) -> Result<(), WriteError> {
    let Some(artwork_handle) = annotation.three_d() else {
        return Ok(());
    };
    let number = artwork_nums.get(artwork_handle).ok_or_else(|| {
        WriteError::InvalidObject(format!(
            "annotation handle {handle}: /3DD references unknown 3D artwork handle {artwork_handle}"
        ))
    })?;
    let artwork = artworks.get(artwork_handle).ok_or_else(|| {
        WriteError::InvalidObject(format!(
            "annotation handle {handle}: /3DD references unknown 3D artwork handle {artwork_handle}"
        ))
    })?;
    dict.0.insert("3DD".into(), Value::Ref(*number, 0));
    if let Some(view) = annotation.three_d_view() {
        let value = view
            .to_value(artwork.views(), "annotation /3DV")
            .map_err(|message| {
                WriteError::InvalidObject(format!("annotation handle {handle}: {message}"))
            })?;
        dict.0.insert("3DV".into(), value);
    }
    if let Some(activation) = annotation.three_d_activation() {
        dict.0
            .insert("3DA".into(), Value::Dict(activation.to_dict()));
    }
    if let Some(interactive) = annotation.three_d_interactive() {
        dict.0.insert("3DI".into(), Value::Bool(interactive));
    }
    if let Some(view_box) = annotation.three_d_view_box() {
        dict.0.insert("3DB".into(), real_array(view_box));
    }
    Ok(())
}
