//! 标记类注解的 Table 170 附加条目（12.5.6.2）：`/Subj` `/IRT` `/RT` 的
//! 写出、跨注解同页校验与版本门禁。
//!
//! `/IRT` 指向另一条注解，其对象号在写出阶段才分配，因此句柄范围、自引用与
//! 「Both annotations shall be on the same page of the document」（L21699 的
//! shall）都在分配任何对象号与写出任何字节之前统一核对；失败一律报
//! `WriteError::InvalidObject`（零字节契约），与 `annotation_exdata` 同一口径。

use crate::core::builder::types::text_string_value;
use crate::core::builder::types::{PdfAnnotation, ReplyType};
use crate::core::types::{Dict, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

/// 写出注解的 `/Subj` `/IRT` `/RT`（Table 170）；缺省条目不物化。
///
/// `/IRT` 的目标句柄在 `annotation_nums` 里查对象号；`validate_in_reply_to`
/// 已在分配前确认句柄有效，这里的越界分支只是防御性兜底。
pub(super) fn write_markup_entries(
    annot: &PdfAnnotation,
    handle: usize,
    annotation_nums: &[i64],
    dict: &mut Dict,
) -> Result<(), WriteError> {
    if !annot.has_markup_universal_entries() {
        return Ok(());
    }
    if let Some(subject) = annot.subject() {
        dict.0.insert("Subj".into(), text_string_value(subject));
    }
    if let Some(target) = annot.in_reply_to() {
        let num = annotation_nums.get(target).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "annotation handle {handle}: /IRT references unknown annotation handle {target}"
            ))
        })?;
        dict.0.insert("IRT".into(), Value::Ref(*num, 0));
    }
    if let Some(reply_type) = annot.reply_type() {
        dict.0
            .insert("RT".into(), Value::Name(reply_type.name().into()));
    }
    Ok(())
}

/// 校验全部注解的 `/IRT` 目标：句柄有效、非自引用、同页，以及分组的
/// 「主注解 shall not have an IRT entry」（L21733–L21734）。
///
/// `pages[index]` 是句柄 `index` 对应注解所绑定的页号；注解字典本身不记页号，
/// 故由调用方从注册表传入。
pub(super) fn validate_in_reply_to(
    annotations: &[&PdfAnnotation],
    pages: &[usize],
) -> Result<(), WriteError> {
    for (handle, annot) in annotations.iter().enumerate() {
        let Some(target) = annot.in_reply_to() else {
            continue;
        };
        if target == handle {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /IRT must not reference the annotation itself"
            )));
        }
        let Some(target_annot) = annotations.get(target) else {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /IRT references unknown annotation handle {target}"
            )));
        };
        // Table 170 的 `/IRT` 行（L21699）：两条注解必须在同一页。
        if pages.get(handle).copied() != pages.get(target).copied() {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /IRT target must be on the same page as the annotation (Table 170)"
            )));
        }
        // 分组的从属注解以 `/RT /Group` 指向主注解，而主注解不得带 `/IRT`。
        if annot.reply_type() == Some(ReplyType::Group) && target_annot.in_reply_to().is_some() {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /RT Group must reference a primary annotation, which shall not have an /IRT entry (12.5.6.2)"
            )));
        }
    }
    Ok(())
}

/// Table 170 附加条目对应的特性集合：`/Subj` 与 `/IRT` PDF 1.5、`/RT` PDF 1.6。
pub(super) fn markup_universal_features(annot: &PdfAnnotation) -> Vec<PdfFeature> {
    let mut features = Vec::new();
    if annot.subject().is_some() {
        features.push(PdfFeature::AnnotationSubject);
    }
    if annot.in_reply_to().is_some() {
        features.push(PdfFeature::AnnotationInReplyTo);
    }
    if annot.reply_type().is_some() {
        features.push(PdfFeature::AnnotationReplyType);
    }
    features
}

/// Table 170 附加条目的版本门禁。
pub(super) fn markup_universal_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    for feature in markup_universal_features(annot) {
        if !version.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {}",
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    Ok(())
}
