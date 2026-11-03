//! 页树写出：每页 Page 字典（媒体框、资源字典、`/SeparationInfo`、`/Group`、`/VP`、
//! `/Dur`、`/Trans`、`/Thumb`、`/Annots`）与其内容流对象。

use crate::core::builder::PdfBuilder;
use crate::core::builder::types::AnnotationKind;
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, StreamData, Value};
use crate::core::writer::WriteError;

use super::colorspace::color_space_value;
use super::plan::{AnnotPlan, FormPlan, GroupPlan, PoolPlan, Preflight};
use super::separation;
use super::utils::group_dict;

impl PdfBuilder {
    /// 逐页写出 Page 字典与内容流；资源名分配顺序与拆分前一致。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assemble_page_tree(
        &mut self,
        doc: &mut Document,
        num: &mut i64,
        preflight: &Preflight,
        pools: &PoolPlan,
        forms: &FormPlan,
        groups: &GroupPlan,
        annot_plan: &AnnotPlan<'_>,
        pages_num: i64,
    ) -> Result<(), WriteError> {
        // 每页 Page 字典 + 内容流
        let color_spaces = &self.color_spaces;
        let mut color_space_values: Vec<Option<Value>> = vec![None; color_spaces.len()];
        let separation_groups = &self.separation_groups;
        let mut separation_color_space_values: Vec<Option<Value>> =
            vec![None; separation_groups.len()];
        for (i, p) in self.pages.iter_mut().enumerate() {
            let (page_num, content_num) = pools.page_slots[i];
            // 页面边界（14.11.2）：显式设置时以其 `/MediaBox` 取代页尺寸；
            // 未设置时保持 `page(PageSize)` 的既有行为。
            let mediabox = match &p.boundaries {
                Some(boundaries) => boundaries.media_array(),
                None => {
                    let (w, h) = p.size.dimensions();
                    Array::new()
                        .push(Value::Integer(0))
                        .push(Value::Integer(0))
                        .push(Value::Float(w))
                        .push(Value::Float(h))
                }
            };

            // 资源：Font 子字典（/F1 -> ref, ..., /CF1 -> ref, ...）、XObject 子字典（/Im1 -> ref, ...）
            let mut font_dict = Dict::new();
            for (k, fnum) in pools.page_font_nums[i].iter().enumerate() {
                font_dict = font_dict.insert(format!("F{}", k + 1), Value::Ref(*fnum, 0));
            }
            // CIDFont 资源：/CF1 -> Type 0 父对象引用
            for (k, type0_num) in pools.page_cid_font_nums[i].iter().enumerate() {
                font_dict = font_dict.insert(format!("CF{}", k + 1), Value::Ref(*type0_num, 0));
            }
            let mut xobj_dict = Dict::new();
            for (k, inum) in pools.page_image_nums[i].iter().enumerate() {
                xobj_dict = xobj_dict.insert(format!("Im{}", k + 1), Value::Ref(*inum, 0));
            }
            for (k, handle) in p.forms.iter().enumerate() {
                xobj_dict = xobj_dict.insert(
                    format!("Fm{}", k + 1),
                    Value::Ref(forms.form_nums[*handle], 0),
                );
            }
            let mut gs_dict = Dict::new();
            for (k, gnum) in pools.page_gs_nums[i].iter().enumerate() {
                gs_dict = gs_dict.insert(format!("GS{}", k + 1), Value::Ref(*gnum, 0));
            }
            // 可选内容属性名：/OC1 -> OCG 引用，供 `/OC /OC1 BDC` 使用。
            let mut properties_dict = Dict::new();
            for (k, handle) in p.optional_content.iter().enumerate() {
                properties_dict = properties_dict.insert(
                    format!("OC{}", k + 1),
                    Value::Ref(groups.ocg_nums[*handle], 0),
                );
            }
            // 渐变与着色图案资源名：/Sh1、/P1 按页内首次使用顺序分配。
            let mut shading_dict_res = Dict::new();
            for (k, handle) in p.shadings.iter().enumerate() {
                shading_dict_res = shading_dict_res.insert(
                    format!("Sh{}", k + 1),
                    Value::Ref(groups.shading_nums[*handle], 0),
                );
            }
            let mut pattern_dict_res = Dict::new();
            for (k, handle) in p.patterns.iter().enumerate() {
                pattern_dict_res = pattern_dict_res.insert(
                    format!("P{}", k + 1),
                    Value::Ref(groups.pattern_nums[*handle], 0),
                );
            }
            // 命名颜色空间：按页内首次引用顺序登记 /CS1、/CS2……；
            // 同一句柄复用缓存的字典值，ICCBased/函数流对象只写一次。
            let mut color_space_dict_res = Dict::new();
            for (k, handle) in p.color_spaces.iter().enumerate() {
                if color_space_values[*handle].is_none() {
                    let space = &color_spaces[*handle];
                    space.validate().map_err(|message| {
                        WriteError::InvalidObject(format!("color space: {message}"))
                    })?;
                    let value = color_space_value(space, doc, num)?;
                    color_space_values[*handle] = Some(value);
                }
                color_space_dict_res = color_space_dict_res.insert(
                    format!("CS{}", k + 1),
                    color_space_values[*handle].clone().unwrap(),
                );
            }

            let mut resources = Dict::new();
            if !font_dict.0.is_empty() {
                resources = resources.insert("Font", Value::Dict(font_dict));
            }
            if !xobj_dict.0.is_empty() {
                resources = resources.insert("XObject", Value::Dict(xobj_dict));
            }
            if !gs_dict.0.is_empty() {
                resources = resources.insert("ExtGState", Value::Dict(gs_dict));
            }
            if !properties_dict.0.is_empty() {
                resources = resources.insert("Properties", Value::Dict(properties_dict));
            }
            if !color_space_dict_res.0.is_empty() {
                resources = resources.insert("ColorSpace", Value::Dict(color_space_dict_res));
            }
            if !shading_dict_res.0.is_empty() {
                resources = resources.insert("Shading", Value::Dict(shading_dict_res));
            }
            if !pattern_dict_res.0.is_empty() {
                resources = resources.insert("Pattern", Value::Dict(pattern_dict_res));
            }

            let mut page_dict = Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(pages_num, 0))
                .insert("MediaBox", Value::Array(mediabox))
                .insert("Contents", Value::Ref(content_num, 0))
                .insert("Resources", Value::Dict(resources));
            if let Some(boundaries) = &p.boundaries {
                page_dict = boundaries.extend_page_dict(page_dict);
                let mut features = doc.features.borrow_mut();
                for feature in boundaries.features() {
                    features.insert(feature);
                }
            }
            // 分色字典（14.11.4 Table 364）：同组各页写出相同的 `/Pages`；
            // 组的颜色空间只序列化一次，多页共享同一批流对象引用。
            if let Some(group_index) = preflight.separation_page_map[i] {
                let group = &separation_groups[group_index];
                if let Some(space) = &group.info.color_space
                    && separation_color_space_values[group_index].is_none()
                {
                    separation_color_space_values[group_index] =
                        Some(color_space_value(space, doc, num)?);
                }
                page_dict = page_dict.insert(
                    "SeparationInfo",
                    Value::Dict(separation::info_dict(
                        group,
                        &pools.page_slots,
                        separation_color_space_values[group_index].clone(),
                    )),
                );
                doc.features.borrow_mut().insert(PdfFeature::SeparationInfo);
            }
            if let Some(group) = &p.group {
                group.validate().map_err(|message| {
                    WriteError::InvalidObject(format!("page group: {message}"))
                })?;
                page_dict = page_dict.insert("Group", Value::Dict(group_dict(group, doc, num)?));
                doc.features.borrow_mut().insert(PdfFeature::Transparency);
            }
            if !p.viewports.is_empty() {
                let mut array = Array::new();
                for viewport in &p.viewports {
                    array = array.push(Value::Dict(viewport.to_dict()));
                }
                page_dict = page_dict.insert("VP", Value::Array(array));
                doc.features.borrow_mut().insert(PdfFeature::PageViewports);
            }
            if let Some(seconds) = p.duration {
                page_dict = page_dict.insert("Dur", Value::Float(seconds));
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::PagePresentation);
            }
            if let Some(transition) = &p.transition {
                page_dict = page_dict.insert("Trans", Value::Dict(transition.to_dict()?));
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::PagePresentation);
            }
            if let Some(thumbnail_num) = pools.page_thumbnail_nums[i] {
                page_dict = page_dict.insert("Thumb", Value::Ref(thumbnail_num, 0));
            }
            if !annot_plan.page_annotation_handles[i].is_empty()
                || !annot_plan.page_widget_nums[i].is_empty()
            {
                let mut annots = Array::new();
                // 14.11.6：Trap 网络注解必须是页面 `/Annots` 的最后一项。
                // 其余注解保持注册相对顺序，Widget 紧随其后，Trap 网络最后。
                for &handle in &annot_plan.page_annotation_handles[i] {
                    if self.annotations[handle].annot.kind() != AnnotationKind::TrapNet {
                        annots = annots.push(Value::Ref(annot_plan.annotation_nums[handle], 0));
                    }
                }
                for &num in &annot_plan.page_widget_nums[i] {
                    annots = annots.push(Value::Ref(num, 0));
                }
                for &handle in &annot_plan.page_annotation_handles[i] {
                    if self.annotations[handle].annot.kind() == AnnotationKind::TrapNet {
                        annots = annots.push(Value::Ref(annot_plan.annotation_nums[handle], 0));
                    }
                }
                page_dict = page_dict.insert("Annots", Value::Array(annots));
            }
            doc.set_object(Object::new(page_num, 0, Value::Dict(page_dict)));

            // 内容流序列化；透明度等特性已由 ExtGState 构建路径登记。
            // 属性表占位若未被 `allocate_marked_property_objects` 替换，这里
            // 类型化拒绝而非写出 `<句柄> -1 R`；本阶段失败时由 `build` 的
            // 「未写出即返回 Err」保证零字节契约。
            let content_bytes = p
                .content
                .try_to_bytes()
                .map_err(|error| WriteError::InvalidObject(format!("page {i} content: {error}")))?;
            let len = content_bytes.len();
            doc.set_object(Object::new(
                content_num,
                0,
                Value::Stream(
                    Dict::new().insert("Length", Value::Integer(len as i64)),
                    StreamData::Owned(content_bytes),
                ),
            ));
        }
        Ok(())
    }
}
