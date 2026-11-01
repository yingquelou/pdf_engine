//! 写前门禁（零字节契约）：文档语言、Span/Artifact 属性表、结构标记、表单、
//! 嵌入文件、声音、3D、输出意图、OPI、分色、页面边界/视口与演示等版本检查与取值校验，
//! 全部在任何对象号分配与字节写出之前完成，产物汇总到 [`Preflight`]。

use std::collections::BTreeMap;

use crate::core::builder::navigation::{structure_content, tag_suspect};
use crate::core::builder::types::ThreeDViewSelection;
use crate::core::builder::types::reversed_chars;
use crate::core::builder::types::viewer_preferences;
use crate::core::builder::{Action, GoTo3DViewSelection, PdfBuilder};
use crate::core::content::{MarkedContentProperties, Op};
use crate::core::types::{PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::plan::Preflight;
use super::{embedded_file, form_field, opi, output_intent, separation, sound, three_d};

impl PdfBuilder {
    /// 执行全部写前门禁并返回其产物。
    ///
    /// 非法取值返回 [`WriteError::InvalidObject`]，版本不足返回
    /// [`WriteError::UnsupportedVersion`]；检查顺序与拆分前完全一致。
    pub(super) fn assemble_preflight(&self) -> Result<Preflight, WriteError> {
        // 文档默认语言（14.9.2.1）：目标版本门禁在任何对象号分配与写出
        // 之前完成；语法已由 `LanguageIdentifier` 保证，无需再解析。
        if self.document_language.is_some() && !self.version.supports(PdfFeature::DocumentLanguage)
        {
            return Err(WriteError::UnsupportedVersion(format!(
                "document language (/Lang) requires PDF {}, got {}",
                PdfFeature::DocumentLanguage.min_version(),
                self.version
            )));
        }

        // Span 属性表（14.9.2.1、14.9.3–14.9.5）：`/Lang` 与 `/E` 自
        // PDF 1.4、`/Alt` 与 `/ActualText` 自 PDF 1.5 引入。这些条目只
        // 出现在内容流 BDC 的内联属性表中，读端特性画像不扫描内容流，
        // 故写端在此自行扫描；门禁同样在任何对象号分配与写出之前完成，
        // 保持零字节失败契约。
        let mut span_property_lists = false;
        let mut span_alternate_text = false;
        for page in &self.pages {
            for op in page.content.ops() {
                let Op::BeginMarkedContentProperties {
                    tag,
                    properties: MarkedContentProperties::Dict(dict),
                } = op
                else {
                    continue;
                };
                if tag.as_str() != "Span" {
                    continue;
                }
                span_property_lists |= dict.get("Lang").is_some() || dict.get("E").is_some();
                span_alternate_text |=
                    dict.get("Alt").is_some() || dict.get("ActualText").is_some();
            }
        }
        if span_property_lists && !self.version.supports(PdfFeature::SpanPropertyLists) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{} require PDF {}, got {}",
                PdfFeature::SpanPropertyLists,
                PdfFeature::SpanPropertyLists.min_version().as_str(),
                self.version.as_str()
            )));
        }
        if span_alternate_text && !self.version.supports(PdfFeature::SpanAlternateText) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{} require PDF {}, got {}",
                PdfFeature::SpanAlternateText,
                PdfFeature::SpanAlternateText.min_version().as_str(),
                self.version.as_str()
            )));
        }

        // Artifact 属性表（14.8.2.2.2 Table 330）：`/Type` `/BBox`
        // `/Attached` 自 PDF 1.4、`/Type /Background` 与 `/Subtype` 自
        // PDF 1.7 引入。条目同样只出现在内容流 BDC 的内联属性表中，写端
        // 在此自行扫描；原始字典先按 Table 330 校验，校验失败返回
        // [`WriteError::InvalidObject`]，门禁同样在任何对象号分配与写出
        // 之前完成，保持零字节失败契约。
        let mut artifact_property_lists = false;
        let mut artifact_background = false;
        let mut artifact_subtype = false;
        let mut artifact_pdf17 = false;
        for page in &self.pages {
            for op in page.content.ops() {
                let Op::BeginMarkedContentProperties {
                    tag,
                    properties: MarkedContentProperties::Dict(dict),
                } = op
                else {
                    continue;
                };
                if tag.as_str() != "Artifact" {
                    continue;
                }
                let flags = crate::core::builder::types::artifact::validate_artifact_dict(dict)
                    .map_err(|message| {
                        WriteError::InvalidObject(format!("Artifact property list: {message}"))
                    })?;
                artifact_property_lists = true;
                artifact_background = artifact_background || flags.background;
                artifact_subtype = artifact_subtype || flags.subtype;
                artifact_pdf17 = artifact_pdf17 || flags.pdf17;
            }
        }
        if artifact_property_lists && !self.version.supports(PdfFeature::ArtifactPropertyLists) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{} require PDF {}, got {}",
                PdfFeature::ArtifactPropertyLists,
                PdfFeature::ArtifactPropertyLists.min_version().as_str(),
                self.version.as_str()
            )));
        }
        if artifact_pdf17 {
            if artifact_background && !self.version.supports(PdfFeature::ArtifactBackground) {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} require PDF {}, got {}",
                    PdfFeature::ArtifactBackground,
                    PdfFeature::ArtifactBackground.min_version().as_str(),
                    self.version.as_str()
                )));
            }
            if artifact_subtype && !self.version.supports(PdfFeature::ArtifactSubtype) {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} require PDF {}, got {}",
                    PdfFeature::ArtifactSubtype,
                    PdfFeature::ArtifactSubtype.min_version().as_str(),
                    self.version.as_str()
                )));
            }
        }

        // 结构标记内容（14.8.2）：页面 BDC 内联属性表里的 `/MCID` 必须与
        // 结构树的 MCR 子项逐页一致。原始字典只接受整数值——写端一律写
        // `Value::Integer`，非整数说明调用方手工拼了属性表。`Artifact` 带
        // `/MCID` 直接拒绝：14.8.2.3.1 规定 artifact 只进页面内容顺序、不得
        // 被赋予结构身份。门禁同样在任何对象号分配与写出之前完成，保持
        // 零字节失败契约。
        let mut structure_page_mcids: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
        for (page, page_build) in self.pages.iter().enumerate() {
            for op in page_build.content.ops() {
                let Op::BeginMarkedContentProperties {
                    tag,
                    properties: MarkedContentProperties::Dict(dict),
                } = op
                else {
                    continue;
                };
                let Some(value) = dict.get("MCID") else {
                    continue;
                };
                if tag.as_str() == "Artifact" {
                    return Err(WriteError::InvalidObject(format!(
                        "structure marked content: page {page} Artifact must not carry /MCID (14.8.2.3.1)"
                    )));
                }
                let Value::Integer(mcid) = value else {
                    return Err(WriteError::InvalidObject(format!(
                        "structure marked content: page {page} /MCID must be an integer, got {}",
                        value.type_name()
                    )));
                };
                structure_page_mcids.entry(page).or_default().push(*mcid);
            }
        }
        structure_content::validate(&structure_page_mcids, self.navigation.structure_tree())
            .map_err(|message| {
                WriteError::InvalidObject(format!("structure marked content: {message}"))
            })?;

        // Tag suspects（14.8.2.3.1）：标签为精确 `TagSuspect` 的标记内容必须
        // 写成规范的 `/TagSuspect /Ordering` 属性表，且文档要在目录声明
        // `/MarkInfo << /Suspects true >>`、目标版本不低于 PDF 1.6。门禁同样
        // 在任何对象号分配与写出之前完成，保持零字节失败契约。
        let tag_suspects =
            tag_suspect::validate(&self.pages, self.navigation.mark_info(), self.version)?;

        // 反序显示串（14.8.2.3.3）：标签为 `ReversedChars` 的标记内容里，
        // 每个显示串必须非空、纯可打印 ASCII、首尾之外无 U+0020 空格，
        // 区段必须有显示串且在内容流内以 EMC 闭合；目标版本不低于 PDF 1.4。
        // 门禁同样在任何对象号分配与写出之前完成，保持零字节失败契约。
        let reversed_characters = reversed_chars::validate(&self.pages, self.version)?;

        // 交互表单（12.7.2–12.7.4）：先克隆出配置，避免后续与 `self`
        // 其他字段的借用冲突；字段树在任何对象写出前完成校验，非法输入
        // 保持零字节契约。AcroForm 自 PDF 1.2 起引入。
        let acro_form = self.acro_form.clone();
        if let Some(form) = &acro_form {
            if self.version < PdfVersion::V1_2 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "AcroForm fields require PDF {}, got {}",
                    PdfVersion::V1_2.as_str(),
                    self.version.as_str()
                )));
            }
            form_field::validate(form, self.pages.len(), self.forms.len(), self.version)?;
            if form_field::has_additional_actions(form)
                && !self
                    .version
                    .supports(PdfFeature::FormFieldAdditionalActions)
            {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} require PDF {}, got {}",
                    PdfFeature::FormFieldAdditionalActions,
                    PdfFeature::FormFieldAdditionalActions
                        .min_version()
                        .as_str(),
                    self.version.as_str()
                )));
            }
        }

        // 表单数据动作（12.7.5）：取值校验与版本门禁在任何对象写出前完成。
        let action_features = self.action_requirements()?;

        // 嵌入文件（7.11）：逐项校验与版本门禁在任何对象写出前完成。
        embedded_file::validate_registration(&self.embedded, self.version)?;
        // 声音对象（13.3 Table 294）：采样参数与 PDF 1.2 门禁同样在
        // 任何对象写出前完成。
        sound::validate_registration(&self.sounds, self.version)?;
        // 3D artwork（13.6 Table 300）：视图/默认视图与 PDF 1.6 门禁同样在
        // 任何对象写出前完成。
        three_d::validate_registration(&self.three_d_artworks, self.version)?;
        // 输出意图（14.11.5 Table 365）：Table 365 条目契约与 PDF 1.4 门禁
        // 同样在任何对象写出前完成。
        output_intent::validate_registration(&self.output_intents, self.version)?;
        // OPI 代理字典（14.11.7 Table 368–370）：图像（含软蒙版、显式蒙版、
        // 替代图像与页面缩略图）与表单的条目契约与 PDF 1.2 门禁在任何对象号
        // 分配前完成。
        opi::validate_images(
            self.pages
                .iter()
                .flat_map(|page| page.images.iter().chain(page.thumbnail.iter())),
            self.version,
        )?;
        opi::validate_forms(&self.forms, self.version)?;
        // 分色字典（14.11.4 Table 364）：组契约与 PDF 1.3 门禁在任何对象
        // 写出前完成；返回的页映射供页写出循环挂 `/SeparationInfo`。
        let separation_page_map =
            separation::validate_groups(&self.separation_groups, self.pages.len(), self.version)?;
        // 页面边界（14.11.2 Table 30）与框颜色信息（Table 360/361）：矩形
        // 取值、框样式条目契约与 PDF 1.3/1.4 门禁同样在任何对象写出前完成。
        for (index, page) in self.pages.iter().enumerate() {
            if let Some(boundaries) = &page.boundaries {
                boundaries.validate(self.version, &format!("page {index}"))?;
            }
        }
        // 3D 视图的 PDF 1.7 条目（`/RM` `/LS` `/SA` `/NA` `/NR`，13.6.4.1
        // Table 304）：覆盖 artwork `/VA` 与内联 `/DV`、注解内联 `/3DV`
        // 及 GoTo3DView 内联 `/V` 四处来源；版本门禁同样在任何对象写出前
        // 完成，特性稍后写入特征画像。
        let actions = self.used_actions();
        let three_d_views =
            self.three_d_artworks
                .iter()
                .flat_map(|artwork| artwork.all_views())
                .chain(self.annotations.iter().filter_map(
                    |entry| match entry.annot.three_d_view() {
                        Some(ThreeDViewSelection::Inline(view)) => Some(&**view),
                        _ => None,
                    },
                ))
                // `/ExData` 的 `/3DV`（13.6.6 Table 313）同样是完整视图字典。
                .chain(
                    self.annotations
                        .iter()
                        .filter_map(|entry| entry.annot.ex_data().map(|markup| markup.view())),
                )
                .chain(actions.iter().filter_map(|action| match action {
                    Action::GoTo3DView(action) => match action.selection() {
                        GoTo3DViewSelection::Inline(view) => Some(&**view),
                        _ => None,
                    },
                    _ => None,
                }));
        let three_d_view_extensions =
            three_d::validate_view_extensions(three_d_views, self.version)?;
        // 集合（12.3.5）与集合项（7.11.6）：`/D` 名称树与 `/CI` ↔ `/Schema`
        // 交叉校验同样在任何对象写出前完成。
        embedded_file::validate_collection_references(
            &self.embedded,
            self.navigation.collection(),
        )?;

        // 页面视口字典（12.9.2 Table 260）：PDF 1.6 门禁与取值校验在
        // 任何对象写出前完成。
        if self.pages.iter().any(|page| !page.viewports.is_empty()) {
            if !self.version.supports(PdfFeature::PageViewports) {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} require PDF {}, got {}",
                    PdfFeature::PageViewports,
                    PdfFeature::PageViewports.min_version().as_str(),
                    self.version.as_str()
                )));
            }
            for (index, page) in self.pages.iter().enumerate() {
                for viewport in &page.viewports {
                    viewport.validate().map_err(|message| {
                        WriteError::InvalidObject(format!("page {index} viewport: {message}"))
                    })?;
                }
            }
        }

        // 页面演示条目（12.4.4 Table 30）：`/Dur` 与 `/Trans` 为 PDF 1.1
        // 引入，PDF 1.5 样式与 `/SS` `/B` 需要更高版本；校验同样在
        // 任何对象写出前完成。
        let has_presentation = self
            .pages
            .iter()
            .any(|page| page.transition.is_some() || page.duration.is_some());
        if has_presentation {
            if !self.version.supports(PdfFeature::PagePresentation) {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} require PDF {}, got {}",
                    PdfFeature::PagePresentation,
                    PdfFeature::PagePresentation.min_version().as_str(),
                    self.version.as_str()
                )));
            }
            for (index, page) in self.pages.iter().enumerate() {
                if let Some(seconds) = page.duration
                    && (!seconds.is_finite() || seconds < 0.0)
                {
                    return Err(WriteError::InvalidObject(format!(
                        "page {index} /Dur must be a non-negative finite number"
                    )));
                }
                if let Some(transition) = &page.transition {
                    transition.validate().map_err(|error| match error {
                        WriteError::InvalidObject(message) => {
                            WriteError::InvalidObject(format!("page {index} transition: {message}"))
                        }
                        other => other,
                    })?;
                    let required = transition.min_version();
                    if self.version < required {
                        return Err(WriteError::UnsupportedVersion(format!(
                            "page {index} transition entries require PDF {}, got {}",
                            required.as_str(),
                            self.version.as_str()
                        )));
                    }
                }
            }
        }
        // 查看器首选项（12.2 Table 150）：取值契约（打印页码子区间与份数
        // 边界为库策略，见模块文档）与逐档位版本门禁在任何对象号分配与
        // 写出之前完成，失败保持零字节契约；确认后的特性档位由特征画像
        // 登记。
        let viewer_pref_features = self
            .viewer_preferences
            .as_ref()
            .map(|prefs| viewer_preferences::validate(prefs, self.version))
            .transpose()?;
        let viewer_pref_features = viewer_pref_features.unwrap_or_default();
        Ok(Preflight {
            acro_form,
            action_features,
            separation_page_map,
            three_d_view_extensions,
            span_property_lists,
            span_alternate_text,
            artifact_property_lists,
            artifact_background,
            artifact_subtype,
            tag_suspects,
            reversed_characters,
            viewer_pref_features,
        })
    }
}
