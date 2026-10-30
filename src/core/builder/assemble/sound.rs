//! 声音对象写出（13.3 Table 294）。
//!
//! 每个通过 [`PdfBuilder::add_sound`](crate::core::builder::PdfBuilder::add_sound)
//! 注册的声音对象在 `build` 时分配一个独立间接流对象号（流对象在 PDF
//! 语法中必须间接），对象号表按句柄顺序交给 Sound 动作与 Sound 注解写出
//! `/Sound` 引用。声音对象自 PDF 1.2 引入，版本门禁在分配任何对象号之前
//! 完成；非法采样参数同样在写出前拒绝（零字节输出契约）。

use crate::core::builder::types::{PdfAnnotation, Sound};
use crate::core::types::{Document, Object, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

/// 注册校验：逐项采样参数与 PDF 1.2 版本门禁，在分配任何对象号之前调用。
pub(super) fn validate_registration(
    sounds: &[Sound],
    version: PdfVersion,
) -> Result<(), WriteError> {
    if sounds.is_empty() {
        return Ok(());
    }
    if !version.supports(PdfFeature::SoundObjects) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::SoundObjects,
            PdfFeature::SoundObjects.min_version().as_str(),
            version.as_str()
        )));
    }
    for (handle, sound) in sounds.iter().enumerate() {
        sound.validate().map_err(|message| {
            WriteError::InvalidObject(format!("sound handle {handle}: {message}"))
        })?;
    }
    Ok(())
}

/// 为每个已注册声音分配一个间接流对象号（按句柄顺序）。
pub(super) fn allocate(sounds: &[Sound], next: &mut i64) -> Vec<i64> {
    sounds
        .iter()
        .map(|_| {
            let number = *next;
            *next += 1;
            number
        })
        .collect()
}

/// 写出声音流对象并登记 [`PdfFeature::SoundObjects`]。
pub(super) fn write_objects(doc: &mut Document, sounds: &[Sound], sound_nums: &[i64]) {
    for (sound, number) in sounds.iter().zip(sound_nums) {
        doc.set_object(Object::new(*number, 0, sound.to_stream_value()));
        doc.features.borrow_mut().insert(PdfFeature::SoundObjects);
    }
}

/// Sound 注解的 `/Sound` 间接引用；句柄越界报类型化错误。
pub(super) fn annotation_sound(
    annotation: &PdfAnnotation,
    handle: usize,
    sound_nums: &[i64],
) -> Result<Option<Value>, WriteError> {
    let Some(sound) = annotation.sound() else {
        return Ok(None);
    };
    let number = sound_nums.get(sound).ok_or_else(|| {
        WriteError::InvalidObject(format!(
            "annotation handle {handle}: /Sound references unknown sound handle {sound}"
        ))
    })?;
    Ok(Some(Value::Ref(*number, 0)))
}
