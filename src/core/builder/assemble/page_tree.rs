//! 页树写出：每页 Page 字典（媒体框、资源字典、`/SeparationInfo`、`/Group`、`/VP`、
//! `/Dur`、`/Trans`、`/Thumb`、`/Annots`）与其内容流对象。

use crate::core::builder::PdfBuilder;
use crate::core::builder::types::AnnotationKind;
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Value};
use crate::core::writer::WriteError;

use super::colorspace::color_space_value;
use super::plan::{AnnotPlan, FormPlan, GroupPlan, MarkedPropertiesPlan, PoolPlan, Preflight};
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
        marked_properties: &MarkedPropertiesPlan,
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
            // 标记内容属性表（14.6.2）：/MC1 -> 属性表对象引用，供
            // `/Tag /MC1 BDC` 按 Table 320 授权形态引用。
            let mut properties_dict = Dict::new();
            for (k, handle) in p.optional_content.iter().enumerate() {
                properties_dict = properties_dict.insert(
                    format!("OC{}", k + 1),
                    Value::Ref(groups.ocg_nums[*handle], 0),
                );
            }
            for (name, object) in &marked_properties.page_properties[i] {
                properties_dict =
                    properties_dict.insert(name.clone(), Value::Ref(*object, 0));
            }
            // 表单若未自带 `/Resources`，其内容流里的 `MC<n>` 经本页资源解析；
            // 名字按句柄全局唯一，与页面自身/其它表单的属性表都不会撞名。
            for &form_handle in p.forms.iter() {
                for (name, object) in &marked_properties.form_properties[form_handle] {
                    properties_dict =
                        properties_dict.insert(name.clone(), Value::Ref(*object, 0));
                }
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
            // 默认颜色空间（8.6.3，Table 107，PDF 1.3）：复用颜色空间值
            // 序列化（ICCBased 等分配独立流对象），版本门禁先于写出。
            if !p.default_color_spaces.is_empty() && self.version < PdfVersion::V1_3 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "page default color spaces require PDF {}, got {}",
                    PdfVersion::V1_3.as_str(),
                    self.version.as_str()
                )));
            }
            for (kind, space) in &p.default_color_spaces {
                let value = color_space_value(space, doc, num)?;
                resources = resources.insert(kind.resource_key(), value);
            }

            // 7.7.3.3：`/Contents` 可为单流或流数组（按序连接）。
            let contents_value = if pools.page_extra_content_nums[i].is_empty() {
                Value::Ref(content_num, 0)
            } else {
                let mut array = Array::new().push(Value::Ref(content_num, 0));
                for &extra_num in &pools.page_extra_content_nums[i] {
                    array = array.push(Value::Ref(extra_num, 0));
                }
                Value::Array(array)
            };
            let mut page_dict = Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(pages_num, 0))
                .insert("MediaBox", Value::Array(mediabox))
                .insert("Contents", contents_value)
                .insert("Resources", Value::Dict(resources));
            if let Some(boundaries) = &p.boundaries {
                page_dict = boundaries.extend_page_dict(page_dict);
                let mut features = doc.features.borrow_mut();
                for feature in boundaries.features() {
                    features.insert(feature);
                }
            }
            if let Some(degrees) = p.rotate {
                if degrees % 90 != 0 {
                    return Err(WriteError::InvalidObject(format!(
                        "page {i} /Rotate {degrees} is not a multiple of 90"
                    )));
                }
                page_dict = page_dict.insert("Rotate", Value::Integer(degrees as i64));
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
            // 页面首选缩放与 Web Capture 标识符（Table 30，14.10.6，PDF 1.3）。
            if p.preferred_zoom.is_some() || p.web_capture_id.is_some() {
                if self.version < crate::core::types::PdfVersion::V1_3 {
                    return Err(WriteError::UnsupportedVersion(format!(
                        "page /PZ and /ID require PDF 1.3, got {}",
                        self.version
                    )));
                }
            }
            if let Some(zoom) = p.preferred_zoom {
                if !zoom.is_finite() || zoom <= 0.0 {
                    return Err(WriteError::InvalidObject(
                        "page /PZ must be a positive finite number".into(),
                    ));
                }
                page_dict = page_dict.insert("PZ", Value::Float(zoom));
            }
            if let Some(id) = &p.web_capture_id {
                if id.is_empty() {
                    return Err(WriteError::InvalidObject(
                        "page /ID must not be empty".into(),
                    ));
                }
                page_dict = page_dict.insert("ID", Value::HexString(hex_encode(id)));
            }
            // 页面件字典（14.5 Table 30/318，PDF 1.3）。
            if let Some(piece_info) = &p.piece_info {
                if self.version < crate::core::types::PdfVersion::V1_3 {
                    return Err(WriteError::UnsupportedVersion(format!(
                        "page /PieceInfo requires PDF 1.3, got {}",
                        self.version
                    )));
                }
                piece_info.validate()?;
                page_dict = page_dict.insert("PieceInfo", Value::Dict(piece_info.to_dict()));
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
            // 追加内容流（7.7.3.3）：各自独立成流对象，页面 `/Contents`
            // 数组按「主内容流 → 追加顺序」引用，效果等于依次连接。
            for (k, extra) in p.extra_contents.iter_mut().enumerate() {
                let bytes = extra.try_to_bytes().map_err(|error| {
                    WriteError::InvalidObject(format!("page {i} extra content {k}: {error}"))
                })?;
                let extra_num = pools.page_extra_content_nums[i][k];
                doc.set_object(Object::new(
                    extra_num,
                    0,
                    Value::Stream(
                        Dict::new().insert("Length", Value::Integer(bytes.len() as i64)),
                        StreamData::Owned(bytes),
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// 十六进制大写编码（页面 `/ID` 等字节串条目）。
fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0F) as usize] as char);
    }
    out
}
