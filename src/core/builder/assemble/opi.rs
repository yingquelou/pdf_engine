//! OPI 代理字典（14.11.7 Table 368–370）：校验与写出。
//!
//! `/OPI` 可出现在图像与表单 XObject 字典中，自 PDF 1.2 引入。校验在
//! 任何对象号分配前完成，非法输入保持零字节契约；写出时直接内联版本
//! 字典，不额外分配对象。

use super::{ImageData, ImageMask};
use crate::core::builder::{FormXObject, OpiProxy};
use crate::core::types::{Dict, Document, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

/// 校验一组图像及其软蒙版、显式蒙版、替代图像的 OPI 条目。
pub(super) fn validate_images<'a>(
    images: impl IntoIterator<Item = &'a ImageData>,
    version: PdfVersion,
) -> Result<(), WriteError> {
    for image in images {
        validate_image(image, version)?;
    }
    Ok(())
}

/// 校验表单自身及其自有资源图像的 OPI 条目。
pub(super) fn validate_forms(forms: &[FormXObject], version: PdfVersion) -> Result<(), WriteError> {
    for (handle, form) in forms.iter().enumerate() {
        if let Some(opi) = &form.opi {
            validate_proxy(opi, version, &format!("form XObject handle {handle}"))?;
        }
        if let Some(resources) = &form.resources {
            for image in &resources.images {
                validate_image(image, version)?;
            }
        }
    }
    Ok(())
}

fn validate_image(image: &ImageData, version: PdfVersion) -> Result<(), WriteError> {
    if let Some(opi) = &image.opi {
        validate_proxy(opi, version, "image XObject")?;
    }
    if let Some(smask) = &image.smask {
        validate_image(smask, version)?;
    }
    if let Some(ImageMask::Explicit(mask)) = &image.mask {
        validate_image(mask, version)?;
    }
    for alternate in &image.alternates {
        validate_image(alternate, version)?;
    }
    Ok(())
}

fn validate_proxy(opi: &OpiProxy, version: PdfVersion, context: &str) -> Result<(), WriteError> {
    if !version.supports(PdfFeature::OpiDictionaries) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::OpiDictionaries,
            PdfFeature::OpiDictionaries.min_version().as_str(),
            version.as_str()
        )));
    }
    opi.validate()
        .map_err(|message| WriteError::InvalidObject(format!("{context} /OPI: {message}")))
}

/// 把 `/OPI` 版本字典并入流字典，并登记特性画像。
pub(super) fn insert_opi(doc: &mut Document, dict: Dict, opi: &Option<OpiProxy>) -> Dict {
    let Some(opi) = opi else {
        return dict;
    };
    doc.features
        .borrow_mut()
        .insert(PdfFeature::OpiDictionaries);
    dict.insert("OPI", Value::Dict(opi.to_dict()))
}
