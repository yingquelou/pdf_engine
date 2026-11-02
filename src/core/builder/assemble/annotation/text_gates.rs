use crate::core::builder::types::{AnnotationKind, PdfAnnotation};
use crate::core::types::{PdfFeature, PdfVersion};
use crate::core::writer::WriteError;

/// Trap 网络注解跟踪条目的版本门禁：`/LastModified` 自 PDF 1.4 引入
/// （Table 366）；子类型本身自 PDF 1.3 引入，由 `kind().min_version()` 门禁。
pub(in crate::core::builder::assemble) fn trap_network_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    let has_last_modified = annot
        .trap_network()
        .is_some_and(|entries| entries.last_modified().is_some());
    if has_last_modified && !version.supports(PdfFeature::TrapNetworkModificationDate) {
        return Err(WriteError::UnsupportedVersion(format!(
            "TrapNet annotation /LastModified requires PDF {}, got {}",
            PdfVersion::V1_4.as_str(),
            version.as_str()
        )));
    }
    Ok(())
}

/// Line 专有条目的版本门禁（Table 175/176）。
///
/// `/LE` 与端点样式取值各自有版本门起：`/LE` 与 6 种基础样式 PDF 1.4，
/// `Butt`/`ROpenArrow`/`RClosedArrow` PDF 1.5，`Slash` PDF 1.6；
/// `/LL` `/LLE` `/Cap` `/IT` PDF 1.6，`/LLO` `/CP` `/CO` PDF 1.7。
pub(in crate::core::builder::assemble) fn line_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    let line = annot.line();
    for feature in [
        (
            line.has_arrow_entries(),
            PdfFeature::LineAnnotationArrowEndings,
        ),
        (line.has_leader_entries(), PdfFeature::LineAnnotationLeader),
        (
            line.has_caption_entries(),
            PdfFeature::LineAnnotationCaption,
        ),
    ] {
        let (declared, feature) = feature;
        if declared && !version.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {}",
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    if let Some(endings) = line.endings() {
        for style in endings.iter() {
            if version < style.min_version() {
                return Err(WriteError::UnsupportedVersion(format!(
                    "line annotation /LE /{} requires PDF {}, got {}",
                    style.name(),
                    style.min_version().as_str(),
                    version.as_str()
                )));
            }
        }
    }
    Ok(())
}

/// FreeText 专有条目的版本门禁（Table 174）。
///
/// `/Q` PDF 1.4；`/RC` `/DS` PDF 1.5；`/CL` `/IT` `/BE` `/RD` `/LE` PDF 1.6。
/// `/DA` 随 FreeText 子类型自 PDF 1.3 起可用，不单独门禁。`/BE` 与 `/RD`
/// 与几何类注解共用，但 FreeText 上仍属 Table 174 的 PDF 1.6 条目；
/// Square/Circle/Polygon 由 [`geometry_feature_gate`] 按 PDF 1.5 门禁。
pub(in crate::core::builder::assemble) fn free_text_feature_gate(
    annot: &PdfAnnotation,
    version: PdfVersion,
) -> Result<(), WriteError> {
    let free_text = annot.free_text();
    let has_shared_border_entries =
        free_text.border_effect().is_some() || free_text.text_rectangle().is_some();
    for (declared, feature) in [
        (free_text.has_quadding(), PdfFeature::FreeTextQuadding),
        (
            free_text.has_rich_content(),
            PdfFeature::FreeTextRichContent,
        ),
        (
            free_text.has_callout_entries()
                || (annot.kind() == AnnotationKind::FreeText && has_shared_border_entries),
            PdfFeature::FreeTextCallout,
        ),
    ] {
        if declared && !version.supports(feature) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{feature} require PDF {}, got {}",
                feature.min_version().as_str(),
                version.as_str()
            )));
        }
    }
    Ok(())
}
