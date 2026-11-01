//! 特征画像：按已确认的门禁产物与实际使用的资源登记 [`PdfFeature`]，供写端
//! 版本推导与读端画像比对；返回构造好的空 `Document` 供后续对象写出。

use crate::core::builder::types::AnnotationKind;
use crate::core::builder::{Pattern, PdfBuilder};
use crate::core::types::{Document, PdfFeature};

use super::plan::{GroupPlan, Preflight};
use super::{annotation, form_field};

impl PdfBuilder {
    /// 创建 `Document` 并写入特征画像；条目顺序与拆分前一致。
    pub(super) fn assemble_feature_profile(
        &self,
        preflight: &Preflight,
        groups: &GroupPlan,
    ) -> Document {
        // —— 构造 Document ——
        let doc = Document::new();
        if !groups.used_shadings.is_empty() {
            doc.features.borrow_mut().insert(PdfFeature::Shading);
        }
        for &handle in &groups.used_patterns {
            match &self.patterns[handle] {
                Pattern::Shading(_) => {
                    doc.features.borrow_mut().insert(PdfFeature::Shading);
                }
                Pattern::Tiling(_) => {
                    doc.features.borrow_mut().insert(PdfFeature::TilingPattern);
                }
            }
        }
        for entry in &self.annotations {
            if entry.annot.kind().is_markup() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::MarkupAnnotations);
            }
            // `/CA` 与 `/BM` 属透明模型（PDF 1.4）；越界值已在注解校验中拒绝。
            if entry.annot.opacity().is_some() || entry.annot.blend_mode().is_some() {
                doc.features.borrow_mut().insert(PdfFeature::Transparency);
            }
            // `/State` 与 `/StateModel` 为 PDF 1.3 的文本注解标记状态条目。
            if entry.annot.has_markup_state() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::AnnotationMarkupState);
            }
            if entry
                .annot
                .border_array()
                .is_some_and(|border| border.has_dash())
            {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::AnnotationBorderDash);
            }
            let line = entry.annot.line();
            if line.has_arrow_entries() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::LineAnnotationArrowEndings);
            }
            if line.has_leader_entries() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::LineAnnotationLeader);
            }
            if line.has_caption_entries() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::LineAnnotationCaption);
            }
            let free_text = entry.annot.free_text();
            if free_text.has_quadding() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::FreeTextQuadding);
            }
            if free_text.has_rich_content() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::FreeTextRichContent);
            }
            if free_text.has_callout_entries() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::FreeTextCallout);
            }
            for feature in annotation::geometry_features(&entry.annot) {
                doc.features.borrow_mut().insert(feature);
            }
            if entry.annot.measure().is_some() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::AnnotationMeasure);
            }
            // `/ExData` 3D 标记（13.6.6 Table 313）自 PDF 1.7 引入。
            if entry.annot.ex_data().is_some() {
                doc.features.borrow_mut().insert(PdfFeature::ThreeDMarkup);
            }
            for feature in annotation::link_features(&entry.annot) {
                doc.features.borrow_mut().insert(feature);
            }
            if entry.annot.kind() == AnnotationKind::Movie {
                doc.features.borrow_mut().insert(PdfFeature::MovieObjects);
            }
            // Printer's mark 注解（14.11.3 Table 362）自 PDF 1.4 引入。
            if entry.annot.kind() == AnnotationKind::PrinterMark {
                doc.features.borrow_mut().insert(PdfFeature::PrinterMarks);
            }
            // Trap 网络注解（14.11.6 Table 366）自 PDF 1.3 引入；
            // `/LastModified` 跟踪条目自 PDF 1.4 引入。
            if entry.annot.kind() == AnnotationKind::TrapNet {
                doc.features.borrow_mut().insert(PdfFeature::TrapNetworks);
                if entry
                    .annot
                    .trap_network()
                    .is_some_and(|entries| entries.last_modified().is_some())
                {
                    doc.features
                        .borrow_mut()
                        .insert(PdfFeature::TrapNetworkModificationDate);
                }
            }
        }
        // 表单数据动作与字段名条目（12.7.5.2–12.7.5.4），写入特征画像。
        for feature in &preflight.action_features {
            doc.features.borrow_mut().insert(*feature);
        }
        // 3D 视图的 PDF 1.7 条目（13.6.4.1 Table 304），存在时写入特征
        // 画像供版本推导。
        if preflight.three_d_view_extensions {
            doc.features
                .borrow_mut()
                .insert(PdfFeature::ThreeDViewExtensions);
        }
        // Span 属性表条目（14.9.2.1、14.9.3–14.9.5）：内容流扫描结果写入
        // 特征画像，供版本推导；门禁已在上方完成。
        if preflight.span_property_lists {
            doc.features
                .borrow_mut()
                .insert(PdfFeature::SpanPropertyLists);
        }
        if preflight.span_alternate_text {
            doc.features
                .borrow_mut()
                .insert(PdfFeature::SpanAlternateText);
        }
        // Artifact 属性表条目（14.8.2.2.2 Table 330）：内容流扫描结果写入
        // 特征画像，供版本推导；门禁已在上方完成。
        if preflight.artifact_property_lists {
            doc.features
                .borrow_mut()
                .insert(PdfFeature::ArtifactPropertyLists);
        }
        if preflight.artifact_background {
            doc.features
                .borrow_mut()
                .insert(PdfFeature::ArtifactBackground);
        }
        if preflight.artifact_subtype {
            doc.features
                .borrow_mut()
                .insert(PdfFeature::ArtifactSubtype);
        }
        // Tag suspects（14.8.2.3.1）：内容流扫描结果写入特征画像，供版本
        // 推导；门禁已在上方完成。
        if preflight.tag_suspects {
            doc.features.borrow_mut().insert(PdfFeature::TagSuspects);
        }
        // 表单附加动作与计算顺序自 PDF 1.3 引入，写入特征画像供版本推导。
        if let Some(form) = &preflight.acro_form {
            if form_field::has_additional_actions(form) {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::FormFieldAdditionalActions);
            }
            if form.has_alternate_names() {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::FormFieldAlternateName);
            }
            if form_field::has_calculate_actions(form)
                || form
                    .calculation_order()
                    .is_some_and(|order| !order.is_empty())
            {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::FormCalculationOrder);
            }
        }

        doc
    }
}
