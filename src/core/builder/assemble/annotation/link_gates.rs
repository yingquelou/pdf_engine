use crate::core::builder::types::{AnnotationKind, PdfAnnotation};
use crate::core::types::{PdfFeature, PdfVersion};
use crate::core::writer::WriteError;

/// Link 专有条目对应的特性集合（Table 173）：`/H` PDF 1.2、`/PA` PDF 1.3、
/// `/QuadPoints` 与 `/BS` PDF 1.6。
pub(in crate::core::builder::assemble) fn link_features(annot: &PdfAnnotation) -> Vec<PdfFeature> {
    let link = annot.link();
    let mut features = Vec::new();
    if link.highlight().is_some() {
        features.push(PdfFeature::LinkAnnotationHighlighting);
    }
    if link.pressed_action().is_some() {
        features.push(PdfFeature::LinkAnnotationPressedAction);
    }
    if annot.kind() == AnnotationKind::Link {
        if annot.quad_points().is_some() {
            features.push(PdfFeature::LinkAnnotationQuadPoints);
        }
        if annot.border_width().is_some()
            || annot.border_style().is_some()
            || !annot.border_dash().is_empty()
        {
            features.push(PdfFeature::LinkAnnotationBorderStyle);
        }
    }
    features
}

/// Link 专有条目的版本门禁（Table 173）：`/H` PDF 1.2、`/PA` PDF 1.3、
/// `/QuadPoints` 与 `/BS` PDF 1.6。
pub(in crate::core::builder::assemble) fn link_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    for feature in link_features(annot) {
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

/// 旧式 `/Border` 虚线项（Table 164，PDF 1.1）的版本门禁。
pub(in crate::core::builder::assemble) fn border_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    if let Some(border) = annot.border_array()
        && border.has_dash()
        && !version.supports(PdfFeature::AnnotationBorderDash)
    {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::AnnotationBorderDash,
            PdfFeature::AnnotationBorderDash.min_version().as_str(),
            version.as_str()
        )));
    }
    Ok(())
}
