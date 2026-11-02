use crate::core::builder::types::{AnnotationKind, PdfAnnotation, PolygonIntent};
use crate::core::types::{PdfFeature, PdfVersion};
use crate::core::writer::WriteError;

/// 几何类注解（Square/Circle/Polygon/PolyLine）子类型专有条目对应的特性集合。
///
/// `/BE` PDF 1.5（Square/Circle/Polygon）、`/RD` PDF 1.5（Square/Circle）、
/// `/IT /PolygonCloud` PDF 1.6、`/IT` 标注类 PDF 1.7。FreeText 的同名条目
/// 归 [`free_text_feature_gate`] 门禁，不在此重复。
pub(in crate::core::builder::assemble) fn geometry_features(
    annot: &PdfAnnotation,
) -> Vec<PdfFeature> {
    let mut features = Vec::new();
    if let Some(intent) = annot.geometry().intent() {
        features.push(match intent {
            PolygonIntent::PolygonCloud => PdfFeature::PolygonCloudIntent,
            PolygonIntent::PolyLineDimension | PolygonIntent::PolygonDimension => {
                PdfFeature::PolygonDimensionIntent
            }
        });
    }
    let kind = annot.kind();
    let border = annot.free_text();
    if border.border_effect().is_some()
        && matches!(
            kind,
            AnnotationKind::Square | AnnotationKind::Circle | AnnotationKind::Polygon
        )
    {
        features.push(PdfFeature::AnnotationBorderEffect);
    }
    if border.text_rectangle().is_some()
        && matches!(kind, AnnotationKind::Square | AnnotationKind::Circle)
    {
        features.push(PdfFeature::AnnotationRectDifferences);
    }
    features
}

/// 几何类专有条目的版本门禁（Table 177/178）。
pub(in crate::core::builder::assemble) fn geometry_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    for feature in geometry_features(annot) {
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

/// 注解 `/Measure` 测量字典（Table 261–263）自 PDF 1.7 起可用（Table 175/178）。
pub(in crate::core::builder::assemble) fn measure_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    if annot.measure().is_some() && !version.supports(PdfFeature::AnnotationMeasure) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::AnnotationMeasure,
            PdfFeature::AnnotationMeasure.min_version().as_str(),
            version.as_str()
        )));
    }
    Ok(())
}

/// 注解 `/ExData` 3D 标记外部数据字典（13.6.6 Table 313）自 PDF 1.7 引入。
pub(in crate::core::builder::assemble) fn markup_3d_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    if annot.ex_data().is_some() && !version.supports(PdfFeature::ThreeDMarkup) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::ThreeDMarkup,
            PdfFeature::ThreeDMarkup.min_version().as_str(),
            version.as_str()
        )));
    }
    Ok(())
}
