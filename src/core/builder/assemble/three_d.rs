//! 3D artwork 与 3D 注解条目写出（13.6 Table 298/300）。
//!
//! 每个通过 `add_three_d_artwork` 注册的 artwork 写成独立间接流对象，
//! 注解 `/3DD` 引用其对象号；`/3DV` 的下标、`/IN` 名称与 `F`/`L` 在此
//! 相对该 artwork 的 `/VA` 校验。3D 特性自 PDF 1.6 引入，版本门禁与
//! 取值校验都在任何对象写出前完成（零字节输出契约）。

use crate::core::builder::types::{PdfAnnotation, ThreeDArtwork};
use crate::core::types::{Dict, Document, Object, PdfFeature, PdfVersion, Value};
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

/// 为每个已注册 artwork 分配一个间接流对象号（按句柄顺序）。
pub(super) fn allocate(artworks: &[ThreeDArtwork], next: &mut i64) -> Vec<i64> {
    artworks
        .iter()
        .map(|_| {
            let number = *next;
            *next += 1;
            number
        })
        .collect()
}

/// 写出 3D 流对象并登记 [`PdfFeature::ThreeDAnnotations`]。
pub(super) fn write_objects(doc: &mut Document, artworks: &[ThreeDArtwork], nums: &[i64]) {
    for (artwork, number) in artworks.iter().zip(nums) {
        doc.set_object(Object::new(*number, 0, artwork.to_stream_value()));
        doc.features
            .borrow_mut()
            .insert(PdfFeature::ThreeDAnnotations);
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
