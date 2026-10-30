//! 逻辑态 → `Document`：对象号分配、资源字典与字体/图片对象组装。
//!
//! 主流程保留在本模块；具体对象构造按类别拆分为子模块：
//! - `fonts`：普通字体字典与字体程序嵌入（含 TrueType / Type 1 子集）；
//! - `type3`：Type 3 字体字典与字符过程流；
//! - `cid`：CIDFont 后代字典、Type 0 父字体与相关流；
//! - `cmap`：嵌入编码 CMap 与 ToUnicode CMap；
//! - `annotation`：注解字典（`/Annots` 间接对象与目标引用解析）；
//! - `annotation_popup`：`/Popup` 与 `/Parent` 的单侧声明补出与配对冲突校验；
//! - `embedded_file`：嵌入文件与文件规格（`/EmbeddedFiles` 名称树）；
//! - `form_field`：AcroForm 字段树对象分配与 Widget 字典写出；
//! - `acro_form`：AcroForm `/DR` 默认资源字典（字体/ExtGState/ColorSpace/ProcSet）；
//! - `sound`：声音对象流（13.3 Table 294）的分配、写出与注解 `/Sound` 引用。

mod acro_form;
mod action;
mod annotation;
mod annotation_mk;
mod annotation_popup;
mod cid;
mod cmap;
mod embedded_file;
mod fonts;
mod form_field;
mod shading;
mod sound;
mod type3;

use super::{
    Action, CidFont, ColorSpace, Font, Function, GraphicsState, GraphicsStateFunctions,
    GroupXObject, ImageData, ImageMask, OptionalContentGroup, Pattern, PdfBuilder, same_font,
};
use crate::core::builder::types::{AnnotationKind, text_string_value};
use crate::core::types::{
    Array, Dict, Document, Object, PdfFeature, PdfVersion, StreamData, Trailer, Value,
};
use crate::core::writer::WriteError;
use shading::{pattern_dict, shading_object_value, tiling_pattern_stream};
use std::fmt::Write as _;

impl PdfBuilder {
    /// 把逻辑态翻译为底层 `Document`。
    ///
    /// CIDFont 的嵌入程序类型必须与子类型匹配，否则返回
    /// [`WriteError::InvalidObject`]。
    pub(super) fn assemble_document(&mut self) -> Result<Document, WriteError> {
        // —— 对象号分配 ——
        // Catalog = 1, Pages = 2 固定；其后按页、字体、图片依次递增。
        let catalog_num: i64 = 1;
        let pages_num: i64 = 2;
        let mut num = 3i64;

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

        // 第一遍：为每页分配 Page 字典号 + 内容流号，并为字体/图片分配号。
        // 但字体/图片对象号要等遍历各页后才能确定连续段，故先扫一遍。
        // 这里采用「按页交织」分配：每页的 Page、Content 紧邻，
        // 字体和图片对象在所有页处理完后统一分配——但 PDF 对象号
        // 无需连续，只要引用正确即可。为简单与可断言，采用：
        //   3.. : 每页先 Page 再 Content（共 2 * N 个）
        //   之后 : 字体对象
        //   之后 : 图片对象
        let mut page_slots: Vec<(i64, i64)> = Vec::with_capacity(self.pages.len());
        for _ in 0..self.pages.len() {
            let pn = num;
            num += 1;
            let cn = num;
            num += 1;
            page_slots.push((pn, cn));
        }

        // 字体去重池：跨页合并同一 (name,size,encoding) 字体，复用同一对象号。
        let mut font_pool: Vec<(Font, i64)> = Vec::new();
        // 每页用到的字体在页内是 1-based 索引；映射到全局对象号。
        // 页内字体列表顺序即 text() 调用首次出现顺序；同一对象号在
        // font_pool 中按首次出现分配。
        let mut page_font_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.fonts.len());
            for f in &p.fonts {
                let pos = font_pool.iter().position(|(g, _)| same_font(g, f));
                let obj_num = match pos {
                    Some(i) => font_pool[i].1,
                    None => {
                        let n = num;
                        num += 1;
                        font_pool.push((f.clone(), n));
                        n
                    }
                };
                v.push(obj_num);
            }
            page_font_nums.push(v);
        }

        // 图片去重池：按内容完全相等合并（同像素即同一 XObject）。
        // 软蒙版、显式蒙版与替代图像各分配独立流对象号。
        let mut image_pool: Vec<(ImageData, ImageObjectNums)> = Vec::new();
        let mut page_image_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.images.len());
            for img in &p.images {
                let pos = image_pool.iter().position(|(g, _)| g == img);
                let obj_num = match pos {
                    Some(i) => image_pool[i].1.base,
                    None => {
                        let nums = allocate_image_objects(img, &mut num);
                        let base = nums.base;
                        image_pool.push((img.clone(), nums));
                        base
                    }
                };
                v.push(obj_num);
            }
            page_image_nums.push(v);
        }

        // 页面缩略图（12.3.4 `/Thumb`）：与页面绘制图像共用去重池，
        // 但只作为页面字典条目引用，不进入内容资源字典。
        let mut page_thumbnail_nums: Vec<Option<i64>> = Vec::with_capacity(self.pages.len());
        for (index, page) in self.pages.iter().enumerate() {
            let Some(thumbnail) = &page.thumbnail else {
                page_thumbnail_nums.push(None);
                continue;
            };
            validate_thumbnail(thumbnail).map_err(|message| {
                WriteError::InvalidObject(format!("page {index} /Thumb: {message}"))
            })?;
            let obj_num = match image_pool.iter().position(|(g, _)| g == thumbnail) {
                Some(i) => image_pool[i].1.base,
                None => {
                    let nums = allocate_image_objects(thumbnail, &mut num);
                    let base = nums.base;
                    image_pool.push((thumbnail.clone(), nums));
                    base
                }
            };
            page_thumbnail_nums.push(Some(obj_num));
        }

        // CIDFont 去重池：跨页合并同一 CidFont，复用同一 Type 0 父对象号。
        let mut cid_font_pool: Vec<(CidFont, i64)> = Vec::new();
        let mut page_cid_font_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.cid_fonts.len());
            for f in &p.cid_fonts {
                let pos = cid_font_pool.iter().position(|(g, _)| g == f);
                let obj_num = match pos {
                    Some(i) => cid_font_pool[i].1,
                    None => {
                        let n = num;
                        num += 1;
                        cid_font_pool.push((f.clone(), n));
                        n
                    }
                };
                v.push(obj_num);
            }
            page_cid_font_nums.push(v);
        }

        // ExtGState 去重池：跨页合并相同参数配置，复用同一对象号。
        let mut gs_pool: Vec<(GraphicsState, i64)> = Vec::new();
        let mut page_gs_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.graphics_states.len());
            for state in &p.graphics_states {
                let pos = gs_pool.iter().position(|(g, _)| g == state);
                let obj_num = match pos {
                    Some(i) => gs_pool[i].1,
                    None => {
                        let n = num;
                        num += 1;
                        gs_pool.push((state.clone(), n));
                        n
                    }
                };
                v.push(obj_num);
            }
            page_gs_nums.push(v);
        }

        // ExtGState 软蒙版的 /G 必须引用已注册且带 /Group 的 Form XObject。
        for (state, _) in &gs_pool {
            let Some(soft_mask) = &state.soft_mask else {
                continue;
            };
            let form = self.forms.get(soft_mask.group).ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "invalid graphics state: /SMask /G references unknown form handle {}",
                    soft_mask.group
                ))
            })?;
            if form.group.is_none() {
                return Err(WriteError::InvalidObject(
                    "invalid graphics state: /SMask /G must reference a transparency group Form XObject"
                        .into(),
                ));
            }
        }

        // Form XObject：只为页面实际引用的表单分配流对象号（按句柄顺序），
        // 注册但未绘制的表单不占用对象号。被 ExtGState 软蒙版引用的表单
        // 同样需要写出，但不登记为页面 XObject 资源。
        let mut used_forms: std::collections::BTreeSet<usize> = self
            .pages
            .iter()
            .flat_map(|p| p.forms.iter().copied())
            .collect();
        for (state, _) in &gs_pool {
            if let Some(soft_mask) = &state.soft_mask {
                used_forms.insert(soft_mask.group);
            }
        }
        // 注解 `/AP` 外观流与 Redact `/RO` 叠加外观引用同一 Form XObject 池
        // （12.5.5、Table 192）：`/N` 单一流、`/N` 状态子字典各状态、`/D`、
        // `/R` 与 `/RO` 都计入；句柄越界在此拒绝，引用到的表单与页面绘制
        // 表单一样分配对象号。
        for (handle, entry) in self.annotations.iter().enumerate() {
            let mut handles: Vec<usize> = [
                entry.annot.appearance_normal(),
                entry.annot.appearance_down(),
                entry.annot.appearance_rollover(),
            ]
            .into_iter()
            .flatten()
            .collect();
            handles.extend(
                entry
                    .annot
                    .appearance_states()
                    .iter()
                    .map(|(_, form)| *form),
            );
            for form in handles {
                if form >= self.forms.len() {
                    return Err(WriteError::InvalidObject(format!(
                        "annotation handle {handle}: /AP references unknown form handle {form}"
                    )));
                }
                used_forms.insert(form);
            }
            // `/MK` 的三个图标同样引用 Form XObject 池，越界句柄单独报错。
            for form in entry
                .annot
                .appearance_characteristics()
                .into_iter()
                .flat_map(|characteristics| characteristics.icon_handles())
                .flatten()
            {
                if form >= self.forms.len() {
                    return Err(WriteError::InvalidObject(format!(
                        "annotation handle {handle}: /MK references unknown form handle {form}"
                    )));
                }
                used_forms.insert(form);
            }
            // `/RO` 的越界句柄单独报错，与 `/AP` 的消息区分。
            if let Some(form) = entry.annot.redact().overlay_form() {
                if form >= self.forms.len() {
                    return Err(WriteError::InvalidObject(format!(
                        "annotation handle {handle}: /RO references unknown form handle {form}"
                    )));
                }
                used_forms.insert(form);
            }
        }
        // 表单字段 Widget 的 `/AP` 引用同一 Form XObject 池；句柄上界
        // 已在 `form_field::validate` 校验。
        if let Some(form) = &acro_form {
            for handle in form_field::appearance_handles(form) {
                used_forms.insert(handle);
            }
        }
        // Rendition 动作的 media clip `/D` 可引用 Form XObject（13.2.5，
        // Table 274）；与页面绘制表单一样在写出前分配对象号。
        for action in self.used_actions() {
            let Action::Rendition(rendition) = action else {
                continue;
            };
            for form in rendition.form_handles() {
                if form >= self.forms.len() {
                    return Err(WriteError::InvalidObject(format!(
                        "Rendition action media clip /D references unknown form handle {form}"
                    )));
                }
                used_forms.insert(form);
            }
        }
        let mut form_nums = vec![0i64; self.forms.len()];
        for &handle in &used_forms {
            form_nums[handle] = num;
            num += 1;
        }

        // Form 自有资源：字体/图片并入全局去重池（与页面资源共享对象），
        // 仅为实际绘制的表单分配；未提供资源的表单继续继承绘制页资源。
        let mut form_font_nums: Vec<Vec<i64>> = vec![Vec::new(); self.forms.len()];
        let mut form_image_nums: Vec<Vec<i64>> = vec![Vec::new(); self.forms.len()];
        for &handle in &used_forms {
            let Some(resources) = self.forms[handle].resources.clone() else {
                continue;
            };
            let mut fonts = Vec::with_capacity(resources.fonts.len());
            for font in resources.fonts {
                let obj_num = match font_pool.iter().position(|(g, _)| same_font(g, &font)) {
                    Some(i) => font_pool[i].1,
                    None => {
                        let n = num;
                        num += 1;
                        font_pool.push((font, n));
                        n
                    }
                };
                fonts.push(obj_num);
            }
            form_font_nums[handle] = fonts;
            let mut images = Vec::with_capacity(resources.images.len());
            for image in resources.images {
                let obj_num = match image_pool.iter().position(|(g, _)| g == &image) {
                    Some(i) => image_pool[i].1.base,
                    None => {
                        let nums = allocate_image_objects(&image, &mut num);
                        let base = nums.base;
                        image_pool.push((image, nums));
                        base
                    }
                };
                images.push(obj_num);
            }
            form_image_nums[handle] = images;
        }

        // 可选内容组：只为页面实际标记过的组分配对象号（注册但未使用
        // 的组不写出，与 Form XObject 一致）。句柄在 `begin_optional_content`
        // 中已校验上界，此处再校验组参数。
        let mut used_ocgs: std::collections::BTreeSet<usize> = self
            .pages
            .iter()
            .flat_map(|p| p.optional_content.iter().copied())
            .collect();
        // 表单级 `/OC` 与表单自有 `/Properties` 引用的组同样需要写出。
        for &handle in &used_forms {
            let form = &self.forms[handle];
            if let Some(group) = form.optional_content
                && group >= self.optional_content_groups.len()
            {
                return Err(WriteError::InvalidObject(format!(
                    "form XObject: /OC references unknown optional content group handle {group}"
                )));
            }
            if let Some(resources) = &form.resources {
                for &group in &resources.optional_content {
                    if group >= self.optional_content_groups.len() {
                        return Err(WriteError::InvalidObject(format!(
                            "form XObject: /Properties references unknown optional content group handle {group}"
                        )));
                    }
                    used_ocgs.insert(group);
                }
            }
            if let Some(group) = form.optional_content {
                used_ocgs.insert(group);
            }
        }
        // Set-OCG-State 动作（12.6.4.12）引用的组即使未被页面标记也须写出；
        // 句柄上界已由 `action_requirements` 校验。
        for action in self.used_actions() {
            let Action::SetOcgState(state) = action else {
                continue;
            };
            for sequence in state.sequences() {
                used_ocgs.extend(sequence.groups().iter().copied());
            }
        }
        let mut ocg_nums = vec![0i64; self.optional_content_groups.len()];
        for &handle in &used_ocgs {
            self.optional_content_groups[handle]
                .validate()
                .map_err(|message| {
                    WriteError::InvalidObject(format!("optional content group: {message}"))
                })?;
            ocg_nums[handle] = num;
            num += 1;
        }

        // 渐变与着色图案：只为页面实际引用的句柄分配对象号。图案字典
        // 内联引用其渐变，因此图案内部的渐变不额外占用独立对象。
        let used_shadings: std::collections::BTreeSet<usize> = self
            .pages
            .iter()
            .flat_map(|p| p.shadings.iter().copied())
            .collect();
        let mut shading_nums = vec![0i64; self.shadings.len()];
        for &handle in &used_shadings {
            self.shadings[handle]
                .validate()
                .map_err(|message| WriteError::InvalidObject(format!("shading: {message}")))?;
            shading_nums[handle] = num;
            num += 1;
        }
        let used_patterns: std::collections::BTreeSet<usize> = self
            .pages
            .iter()
            .flat_map(|p| p.patterns.iter().copied())
            .collect();
        let mut pattern_nums = vec![0i64; self.patterns.len()];
        for &handle in &used_patterns {
            self.patterns[handle]
                .validate()
                .map_err(WriteError::InvalidObject)?;
            pattern_nums[handle] = num;
            num += 1;
        }

        // 注解：注册的每条注解都写成独立间接对象并按句柄分配对象号
        // （`/Popup` 互引需要预先建表）；页绑定、字典参数与子类型版本
        // 在此校验。
        let mut annotation_nums = vec![0i64; self.annotations.len()];
        let mut page_annotation_handles: Vec<Vec<usize>> = vec![Vec::new(); self.pages.len()];
        for (handle, entry) in self.annotations.iter().enumerate() {
            if entry.page >= self.pages.len() {
                return Err(WriteError::InvalidObject(format!(
                    "annotation handle {handle} attached to unknown page {}",
                    entry.page
                )));
            }
            entry
                .annot
                .validate()
                .map_err(|message| WriteError::InvalidObject(format!("annotation: {message}")))?;
            let required = entry.annot.kind().min_version();
            if self.version < required {
                return Err(WriteError::UnsupportedVersion(format!(
                    "{} annotations require PDF {}, got {}",
                    entry.annot.kind().name(),
                    required.as_str(),
                    self.version.as_str()
                )));
            }
            // `/AP` 外观流自 PDF 1.2 起引入（表 167），与子类型门禁独立。
            if entry.annot.has_appearance() && self.version < PdfVersion::V1_2 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "annotation appearance streams (/AP) require PDF 1.2, got {}",
                    self.version.as_str()
                )));
            }
            // `/State` 与 `/StateModel` 为 PDF 1.3 的文本注解标记状态条目。
            if entry.annot.has_markup_state()
                && !self.version.supports(PdfFeature::AnnotationMarkupState)
            {
                return Err(WriteError::UnsupportedVersion(format!(
                    "text annotation markup state (/State, /StateModel) require PDF {}, got {}",
                    PdfFeature::AnnotationMarkupState.min_version().as_str(),
                    self.version.as_str()
                )));
            }
            // 旧式 `/Border` 虚线项（Table 164）PDF 1.1 门禁。
            annotation::border_feature_gate(&entry.annot, self.version)?;
            // Line 专有条目按 Table 175/176 的版本门起分组门禁。
            annotation::line_feature_gate(&entry.annot, self.version)?;
            // FreeText 专有条目按 Table 174 的版本门起分组门禁。
            annotation::free_text_feature_gate(&entry.annot, self.version)?;
            // 几何类专有条目按 Table 177/178 的版本门起分组门禁。
            annotation::geometry_feature_gate(&entry.annot, self.version)?;
            // `/Measure` 测量字典（12.9）PDF 1.7 门禁。
            annotation::measure_feature_gate(&entry.annot, self.version)?;
            // Link 专有条目按 Table 173 的版本门起分组门禁。
            annotation::link_feature_gate(&entry.annot, self.version)?;
            page_annotation_handles[entry.page].push(handle);
            annotation_nums[handle] = num;
            num += 1;
        }

        // AcroForm 字段树：按前序分配对象号（根到叶），并把各 Widget
        // 对象号按生效页号分组，稍后并入页面 `/Annots`。
        let (field_slots, field_roots) = match &acro_form {
            Some(form) => form_field::allocate_fields(form, &mut num),
            None => (Vec::new(), Vec::new()),
        };
        let mut page_widget_nums: Vec<Vec<i64>> = vec![Vec::new(); self.pages.len()];
        for slot in &field_slots {
            if slot.field.is_widget()
                && let Some(page) = slot.page
            {
                page_widget_nums[page].push(slot.num);
            }
        }

        // 嵌入文件：每项一对（文件规格，嵌入流）对象；注解 `/FS` 与目录
        // `/Names /EmbeddedFiles` 在此之后解析为对象号。
        let embedded_slots = embedded_file::allocate(&self.embedded, &mut num);
        let embedded_nums: Vec<i64> = embedded_slots
            .iter()
            .map(|slot| slot.filespec_num)
            .collect();

        // 声音对象（13.3 Table 294）：每个注册的声音分配一个独立流对象号，
        // 供 Sound 动作与 Sound 注解的 `/Sound` 引用；注册校验已在任何
        // 对象号分配前完成。
        let sound_nums = sound::allocate(&self.sounds, &mut num);

        // —— 构造 Document ——
        let mut doc = Document::new();
        if !used_shadings.is_empty() {
            doc.features.borrow_mut().insert(PdfFeature::Shading);
        }
        for &handle in &used_patterns {
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
            for feature in annotation::link_features(&entry.annot) {
                doc.features.borrow_mut().insert(feature);
            }
            if entry.annot.kind() == AnnotationKind::Movie {
                doc.features.borrow_mut().insert(PdfFeature::MovieObjects);
            }
        }
        // 表单数据动作与字段名条目（12.7.5.2–12.7.5.4），写入特征画像。
        for feature in &action_features {
            doc.features.borrow_mut().insert(*feature);
        }
        // 表单附加动作与计算顺序自 PDF 1.3 引入，写入特征画像供版本推导。
        if let Some(form) = &acro_form {
            if form_field::has_additional_actions(form) {
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::FormFieldAdditionalActions);
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

        // Catalog
        let mut catalog_dict = Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert("Pages", Value::Ref(pages_num, 0));
        if !used_ocgs.is_empty() {
            catalog_dict = catalog_dict.insert(
                "OCProperties",
                Value::Dict(optional_content_properties_dict(
                    &used_ocgs,
                    &self.optional_content_groups,
                    &ocg_nums,
                )),
            );
        }
        // XMP 元数据流（14.3.2）：独立 `/Type /Metadata /Subtype /XML`
        // 流对象，目录 `/Metadata` 间接引用；要求目标版本 ≥1.4。
        // 原文透传与类型化包互斥，歧义按调用方错误处理，不静默择一。
        if self.xmp_metadata.is_some() && self.xmp_packet.is_some() {
            return Err(WriteError::InvalidObject(
                "raw XMP metadata and typed XMP packet are both set; use only one".into(),
            ));
        }
        if self.xmp_metadata.is_some() || self.xmp_packet.is_some() {
            if self.version < PdfVersion::V1_4 {
                return Err(WriteError::UnsupportedVersion(format!(
                    "XMP metadata (/Metadata) require PDF {}, got {}",
                    PdfVersion::V1_4.as_str(),
                    self.version.as_str()
                )));
            }
            let xml = match (&self.xmp_metadata, &self.xmp_packet) {
                (Some(raw), None) => raw.clone(),
                (None, Some(packet)) => packet
                    .to_xml()
                    .map_err(|error| {
                        WriteError::InvalidObject(format!("typed XMP packet: {error}"))
                    })?
                    .into_bytes(),
                _ => unreachable!("exactly one XMP source is set"),
            };
            let meta_num = num;
            num += 1;
            let meta_dict = Dict::new()
                .insert("Type", Value::Name("Metadata".into()))
                .insert("Subtype", Value::Name("XML".into()))
                .insert("Length", Value::Integer(xml.len() as i64));
            doc.set_object(Object::new(
                meta_num,
                0,
                Value::Stream(meta_dict, StreamData::Owned(xml)),
            ));
            catalog_dict = catalog_dict.insert("Metadata", Value::Ref(meta_num, 0));
        }
        // 交互表单目录条目（Table 218）：`/Fields` 引用顶层字段，
        // `/NeedAppearances` 与 `/DA` 仅在声明时写出。
        if let Some(form) = &acro_form {
            let fields: Vec<Value> = field_roots
                .iter()
                .map(|&index| Value::Ref(field_slots[index].num, 0))
                .collect();
            let mut acro = Dict::new().insert("Fields", Value::Array(Array(fields)));
            if form.need_appearances() {
                acro = acro.insert("NeedAppearances", Value::Bool(true));
            }
            if let Some(appearance) = form.default_appearance() {
                acro = acro.insert("DA", text_string_value(appearance));
            }
            // `/DR` 默认资源（Table 218/34）：字体与页面去重共享对象；
            // ExtGState/ColorSpace/ProcSet 由子模块构建。
            if let Some(dr) =
                acro_form::default_resources(form, &mut doc, &mut num, &mut font_pool)?
            {
                acro = acro.insert("DR", dr);
            }
            // `/CO` 计算顺序：显式名录按声明顺序引用字段对象；缺省时按
            // 字段树前序自动列出全部带 `/C` 动作的字段。
            let order = form_field::calculation_order_nums(form, &field_slots)?;
            if !order.is_empty() {
                let refs: Vec<Value> = order.iter().map(|&n| Value::Ref(n, 0)).collect();
                acro = acro.insert("CO", Value::Array(Array(refs)));
            }
            catalog_dict = catalog_dict.insert("AcroForm", Value::Dict(acro));
        }
        // 嵌入文件目录条目（7.11.4）：`/Names /EmbeddedFiles` 名称树；
        // 导航层随后写 `/Dests` `/Pages` 时与本字典合并。
        if !embedded_slots.is_empty() {
            let names = Dict::new().insert(
                "EmbeddedFiles",
                embedded_file::embedded_files_node(&self.embedded, &embedded_slots),
            );
            catalog_dict = catalog_dict.insert("Names", Value::Dict(names));
        }
        doc.set_object(Object::new(catalog_num, 0, Value::Dict(catalog_dict)));

        // Pages Kids 数组：按页顺序引用各 Page 字典。
        let kids: Vec<Value> = page_slots
            .iter()
            .map(|(pn, _)| Value::Ref(*pn, 0))
            .collect();
        let kids_arr = kids.iter().fold(Array::new(), |acc, v| acc.push(v.clone()));
        doc.set_object(Object::new(
            pages_num,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Pages".into()))
                    .insert("Kids", Value::Array(kids_arr))
                    .insert("Count", Value::Integer(self.pages.len() as i64)),
            ),
        ));

        // 每页 Page 字典 + 内容流
        let color_spaces = &self.color_spaces;
        let mut color_space_values: Vec<Option<Value>> = vec![None; color_spaces.len()];
        for (i, p) in self.pages.iter_mut().enumerate() {
            let (page_num, content_num) = page_slots[i];
            let (w, h) = p.size.dimensions();
            let mediabox = Array::new()
                .push(Value::Integer(0))
                .push(Value::Integer(0))
                .push(Value::Float(w))
                .push(Value::Float(h));

            // 资源：Font 子字典（/F1 -> ref, ..., /CF1 -> ref, ...）、XObject 子字典（/Im1 -> ref, ...）
            let mut font_dict = Dict::new();
            for (k, fnum) in page_font_nums[i].iter().enumerate() {
                font_dict = font_dict.insert(format!("F{}", k + 1), Value::Ref(*fnum, 0));
            }
            // CIDFont 资源：/CF1 -> Type 0 父对象引用
            for (k, type0_num) in page_cid_font_nums[i].iter().enumerate() {
                font_dict = font_dict.insert(format!("CF{}", k + 1), Value::Ref(*type0_num, 0));
            }
            let mut xobj_dict = Dict::new();
            for (k, inum) in page_image_nums[i].iter().enumerate() {
                xobj_dict = xobj_dict.insert(format!("Im{}", k + 1), Value::Ref(*inum, 0));
            }
            for (k, handle) in p.forms.iter().enumerate() {
                xobj_dict =
                    xobj_dict.insert(format!("Fm{}", k + 1), Value::Ref(form_nums[*handle], 0));
            }
            let mut gs_dict = Dict::new();
            for (k, gnum) in page_gs_nums[i].iter().enumerate() {
                gs_dict = gs_dict.insert(format!("GS{}", k + 1), Value::Ref(*gnum, 0));
            }
            // 可选内容属性名：/OC1 -> OCG 引用，供 `/OC /OC1 BDC` 使用。
            let mut properties_dict = Dict::new();
            for (k, handle) in p.optional_content.iter().enumerate() {
                properties_dict = properties_dict
                    .insert(format!("OC{}", k + 1), Value::Ref(ocg_nums[*handle], 0));
            }
            // 渐变与着色图案资源名：/Sh1、/P1 按页内首次使用顺序分配。
            let mut shading_dict_res = Dict::new();
            for (k, handle) in p.shadings.iter().enumerate() {
                shading_dict_res = shading_dict_res
                    .insert(format!("Sh{}", k + 1), Value::Ref(shading_nums[*handle], 0));
            }
            let mut pattern_dict_res = Dict::new();
            for (k, handle) in p.patterns.iter().enumerate() {
                pattern_dict_res = pattern_dict_res
                    .insert(format!("P{}", k + 1), Value::Ref(pattern_nums[*handle], 0));
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
                    let value = color_space_value(space, &mut doc, &mut num)?;
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
            if let Some(group) = &p.group {
                group.validate().map_err(|message| {
                    WriteError::InvalidObject(format!("page group: {message}"))
                })?;
                page_dict =
                    page_dict.insert("Group", Value::Dict(group_dict(group, &mut doc, &mut num)?));
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
            if let Some(thumbnail_num) = page_thumbnail_nums[i] {
                page_dict = page_dict.insert("Thumb", Value::Ref(thumbnail_num, 0));
            }
            if !page_annotation_handles[i].is_empty() || !page_widget_nums[i].is_empty() {
                let mut annots = Array::new();
                for &handle in &page_annotation_handles[i] {
                    annots = annots.push(Value::Ref(annotation_nums[handle], 0));
                }
                for &num in &page_widget_nums[i] {
                    annots = annots.push(Value::Ref(num, 0));
                }
                page_dict = page_dict.insert("Annots", Value::Array(annots));
            }
            doc.set_object(Object::new(page_num, 0, Value::Dict(page_dict)));

            // 内容流序列化；透明度等特性已由 ExtGState 构建路径登记。
            let content_bytes = p.content.to_bytes();
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

        // 注解间接对象：在页字典写出后组装，`/Dest` 与 `GoTo` 动作的
        // 页号在此解析为 Page 引用，`/Popup` 句柄解析为注解引用，
        // `/FS` 句柄解析为文件规格引用。
        // 注意 `page_slots` 是 (Page 号, 内容流号) 对，须先映射为
        // (对象号, 生成号) 引用表再交给导航层。
        let page_refs: Vec<(i64, i64)> = page_slots.iter().map(|(pn, _)| (*pn, 0)).collect();
        // 12.5.2/Table 163/183：`/Popup` 与 `/Parent` 只需声明一侧，另一侧
        // 在此补出；配对矛盾与无父弹出框在写出任何注解字典前拒绝。
        let popup_pairs = annotation_popup::pair_popups(
            &self
                .annotations
                .iter()
                .map(|entry| &entry.annot)
                .collect::<Vec<_>>(),
        )?;
        for (handle, entry) in self.annotations.iter().enumerate() {
            let refs = annotation::AnnotationRefs {
                annotation_nums: &annotation_nums,
                form_nums: &form_nums,
                embedded_nums: &embedded_nums,
                ocg_nums: &ocg_nums,
                sound_nums: &sound_nums,
            };
            let dict =
                annotation::annotation_dict(&entry.annot, handle, &popup_pairs, &page_refs, &refs)?;
            doc.set_object(Object::new(annotation_nums[handle], 0, Value::Dict(dict)));
        }
        // 导航层随后写大纲 `/A`；Hide 与 Set-OCG-State 动作的目标句柄
        // 在此解析为对象号。
        self.annotation_nums = annotation_nums.clone();
        self.ocg_nums = ocg_nums.clone();
        self.sound_nums = sound_nums.clone();
        self.form_nums = form_nums.clone();
        self.embedded_nums = embedded_nums.clone();

        // 表单字段/Widget 间接对象：`/P` 指向页面字典，`/Kids`/`/Parent`
        // 已按字段树对象号互引，`/AP` 解析为 Form XObject 流引用。
        for slot in &field_slots {
            let dict = form_field::field_dict(slot, &field_slots, &page_refs, &form_nums)?;
            doc.set_object(Object::new(slot.num, 0, Value::Dict(dict)));
        }

        // 嵌入文件对对象：文件规格 + 嵌入文件流。
        embedded_file::write_objects(&mut doc, &self.embedded, &embedded_slots, self.version);

        // 声音流对象（13.3 Table 294）：对象号已在 `annotation_nums` 之前分配。
        sound::write_objects(&mut doc, &self.sounds, &sound_nums);

        // 字体对象
        for (f, fnum) in &font_pool {
            // Type 3 字体自成一路：字符过程代替字体程序，没有 BaseFont。
            let font_dict = match &f.type3 {
                Some(type3) => type3::type3_font_dict(&mut doc, f, type3, &mut num)?,
                None => fonts::font_dict(&mut doc, f, &mut num)?,
            };
            doc.set_object(Object::new(*fnum, 0, Value::Dict(font_dict)));
        }

        // CIDFont 对象：Type 0 父 + CIDFont 后代 + FontDescriptor + CIDSet 流
        for (cid_font, type0_num) in &cid_font_pool {
            cid::write_cid_font(&mut doc, cid_font, *type0_num, &mut num)?;
        }

        // 图片 XObject 流对象；软蒙版、显式蒙版与替代图像作为紧随其后的
        // 独立流写出（替代图像不进入页面资源字典）。
        for (img, nums) in &image_pool {
            write_image(&mut doc, img, nums, &mut num)?;
        }

        // Form XObject 流对象：BBox 必填，Matrix 仅在提供时写出。
        for &handle in &used_forms {
            let form = &mut self.forms[handle];
            form.validate()
                .map_err(|message| WriteError::InvalidObject(format!("form XObject: {message}")))?;
            let content_bytes = form.content.to_bytes();
            let len = content_bytes.len();
            let mut dict = Dict::new()
                .insert("Type", Value::Name("XObject".into()))
                .insert("Subtype", Value::Name("Form".into()))
                .insert("FormType", Value::Integer(1))
                .insert("BBox", real_array(&form.bbox))
                .insert("Length", Value::Integer(len as i64));
            if let Some(matrix) = form.matrix {
                dict = dict.insert("Matrix", real_array(&matrix));
            }
            if let Some(group) = &form.group {
                dict = dict.insert("Group", Value::Dict(group_dict(group, &mut doc, &mut num)?));
                doc.features.borrow_mut().insert(PdfFeature::Transparency);
            }
            // 自有资源字典：显式提供时总是写出（即使为空），从而替代而
            // 非合并继承自绘制页面的资源。
            if let Some(resources) = &form.resources {
                let mut form_resources = Dict::new();
                if !form_font_nums[handle].is_empty() {
                    let mut font_dict = Dict::new();
                    for (k, fnum) in form_font_nums[handle].iter().enumerate() {
                        font_dict = font_dict.insert(format!("F{}", k + 1), Value::Ref(*fnum, 0));
                    }
                    form_resources = form_resources.insert("Font", Value::Dict(font_dict));
                }
                if !form_image_nums[handle].is_empty() {
                    let mut xobj_dict = Dict::new();
                    for (k, inum) in form_image_nums[handle].iter().enumerate() {
                        xobj_dict = xobj_dict.insert(format!("Im{}", k + 1), Value::Ref(*inum, 0));
                    }
                    form_resources = form_resources.insert("XObject", Value::Dict(xobj_dict));
                }
                if !resources.optional_content.is_empty() {
                    let mut properties = Dict::new();
                    for (k, group) in resources.optional_content.iter().enumerate() {
                        properties = properties
                            .insert(format!("OC{}", k + 1), Value::Ref(ocg_nums[*group], 0));
                    }
                    form_resources = form_resources.insert("Properties", Value::Dict(properties));
                }
                dict = dict.insert("Resources", Value::Dict(form_resources));
            }
            if let Some(group) = form.optional_content {
                dict = dict.insert("OC", Value::Ref(ocg_nums[group], 0));
                doc.features
                    .borrow_mut()
                    .insert(PdfFeature::OptionalContent);
            }
            doc.set_object(Object::new(
                form_nums[handle],
                0,
                Value::Stream(dict, StreamData::Owned(content_bytes)),
            ));
        }

        // 可选内容组对象（Table 99）：/Type /OCG + 文本串名称，可选 /Intent。
        for &handle in &used_ocgs {
            let dict = self.optional_content_groups[handle]
                .to_dict()
                .map_err(|message| {
                    WriteError::InvalidObject(format!("optional content group: {message}"))
                })?;
            doc.set_object(Object::new(ocg_nums[handle], 0, Value::Dict(dict)));
        }

        // 渐变对象（Table 78 / Table 84 / Table 85）：Type 1–3 为字典，
        // Type 4/5 为位打包流；函数经 `write_function` 递归写出
        // （Type 0/4 分配独立流对象）。
        for &handle in &used_shadings {
            let value = shading_object_value(&self.shadings[handle], &mut doc, &mut num)?;
            doc.set_object(Object::new(shading_nums[handle], 0, value));
        }

        // 图案对象：PatternType 2 为字典（内联渐变），PatternType 1 为
        // 带内容流的平铺图案（Table 75 / Table 77）。
        for &handle in &used_patterns {
            let value = match &mut self.patterns[handle] {
                Pattern::Shading(pattern) => {
                    Value::Dict(pattern_dict(pattern, &mut doc, &mut num)?)
                }
                Pattern::Tiling(pattern) => tiling_pattern_stream(pattern, &mut doc, &mut num)?,
            };
            doc.set_object(Object::new(pattern_nums[handle], 0, value));
        }

        // 函数类条目（/TR /BG /BG2 /UCR /UCR2，以及 /SMask /TR 与 /HT 内
        // 函数）：只写被 ExtGState 引用的函数，同一句柄只写一次。Type 0/4
        // 分配独立流对象并返回引用；Type 2/3 内联为直接字典，Type 3 的子
        // 函数由 `write_function` 递归生成。除 ExtGState `/TR` 外均要求
        // 产生单一输出值。
        let mut graphics_function_values: std::collections::BTreeMap<usize, Value> =
            std::collections::BTreeMap::new();
        for (state, _) in &gs_pool {
            if let Some(halftone) = &state.halftone {
                halftone.validate().map_err(|message| {
                    WriteError::InvalidObject(format!("invalid graphics state: /HT {message}"))
                })?;
            }
            let auxiliary = state.auxiliary_function_handles();
            for (role, handle) in state.function_handles().chain(auxiliary) {
                let function = self.functions.get(handle).ok_or_else(|| {
                    WriteError::InvalidObject(format!(
                        "invalid graphics state: /{role} references unknown function handle {handle}"
                    ))
                })?;
                if role != "TR" && function.output_count() != Some(1) {
                    return Err(WriteError::InvalidObject(format!(
                        "invalid graphics state: /{role} function must produce 1 output"
                    )));
                }
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    graphics_function_values.entry(handle)
                {
                    let value = write_function(&mut doc, function, &mut num)?;
                    entry.insert(value);
                }
            }
        }

        // 图形状态参数对象：/Font 条目写成字体字典的间接引用。
        for (state, gs_num) in &gs_pool {
            let font_num = match &state.font {
                Some(gs_font) => Some(
                    font_pool
                        .iter()
                        .find(|(font, _)| same_font(font, &gs_font.font))
                        .map(|(_, font_num)| *font_num)
                        .ok_or_else(|| {
                            WriteError::InvalidObject(format!(
                                "graphics state font {} is not registered on any page",
                                gs_font.font.name
                            ))
                        })?,
                ),
                None => None,
            };
            let soft_mask_group_num = state
                .soft_mask
                .as_ref()
                .map(|soft_mask| form_nums[soft_mask.group]);
            let function_value = |handle: Option<usize>| {
                handle.map(|handle| graphics_function_values[&handle].clone())
            };
            let halftone_value = match &state.halftone {
                None => None,
                Some(halftone) => {
                    let mut resolve = |handle| graphics_function_values.get(&handle).cloned();
                    let value = halftone.to_value(&mut resolve).map_err(|message| {
                        WriteError::InvalidObject(format!("invalid graphics state: /HT {message}"))
                    })?;
                    Some(value)
                }
            };
            let functions = GraphicsStateFunctions {
                transfer: function_value(state.transfer_function),
                black_generation: function_value(state.black_generation),
                black_generation_2: function_value(state.black_generation_2),
                undercolor_removal: function_value(state.undercolor_removal),
                undercolor_removal_2: function_value(state.undercolor_removal_2),
                smask_transfer: state
                    .soft_mask
                    .as_ref()
                    .and_then(|soft_mask| function_value(soft_mask.transfer_function)),
                halftone: halftone_value,
            };
            let dict = state.to_value(font_num, soft_mask_group_num, functions)?;
            doc.set_object(Object::new(*gs_num, 0, Value::Dict(dict)));
        }

        // trailer：/Root -> Catalog(1)。/Size 由 writer 按**实际写出**的
        // 最大对象号 + 1 计算（经典表用 `ctx.max_num + 1`，XRef 流用自身
        // 分配到的对象号），此处显式给出以保证与对象号一致。
        // /Info（14.3.3）：非空时分配独立对象并以间接引用挂接。
        let mut trailer_dict = Dict::new().insert("Root", Value::Ref(catalog_num, 0));
        if !self.document_info.is_empty() {
            let info_num = num;
            num += 1;
            doc.set_object(Object::new(
                info_num,
                0,
                Value::Dict(self.document_info.to_dict(self.version)?),
            ));
            trailer_dict = trailer_dict.insert("Info", Value::Ref(info_num, 0));
        }
        // /ID（14.4）：显式设置时写 trailer 数组；不调用则保持缺省。
        if let Some(id) = &self.file_identifier {
            trailer_dict = trailer_dict.insert("ID", id.to_value());
        }
        let max_num = num - 1;
        doc.trailer = Trailer {
            dict: trailer_dict.insert("Size", Value::Integer(max_num + 1)),
        };
        Ok(doc)
    }
}

pub(super) fn real_array(values: &[f64]) -> Value {
    Value::Array(Array(values.iter().map(|v| Value::Float(*v)).collect()))
}

/// 把函数写成对象/字典并返回可嵌入的值。
///
/// Type 0/4 必须为流：分配对象号、补 `/Length` 后写出，返回间接引用；
/// Type 2/3 返回直接字典，其中 Type 3 的子函数递归处理。
fn write_function(
    doc: &mut Document,
    function: &Function,
    num: &mut i64,
) -> Result<Value, WriteError> {
    let mut nested = |nested_function: &Function| -> Result<Value, String> {
        write_function(doc, nested_function, num).map_err(|error| error.to_string())
    };
    let object = function
        .to_object(&mut nested)
        .map_err(|message| WriteError::InvalidObject(format!("function: {message}")))?;
    match object.stream {
        Some(data) => {
            let object_num = *num;
            *num += 1;
            let dict = object
                .dict
                .insert("Length", Value::Integer(data.len() as i64));
            doc.set_object(Object::new(
                object_num,
                0,
                Value::Stream(dict, StreamData::Owned(data)),
            ));
            Ok(Value::Ref(object_num, 0))
        }
        None => Ok(Value::Dict(object.dict)),
    }
}

/// 目录 `/OCProperties`（Table 100）：使用组列表 + 默认配置（Table 101）。
///
/// `/BaseState` 固定为 `/ON`，每组初始可见性由 `/ON` / `/OFF` 精确表达；
/// `/Order` 按组注册顺序列出（仅实际使用的组）。
fn optional_content_properties_dict(
    used: &std::collections::BTreeSet<usize>,
    groups: &[OptionalContentGroup],
    ocg_nums: &[i64],
) -> Dict {
    let all: Vec<Value> = used
        .iter()
        .map(|&handle| Value::Ref(ocg_nums[handle], 0))
        .collect();
    let on: Vec<Value> = used
        .iter()
        .filter(|&&handle| groups[handle].default_visible)
        .map(|&handle| Value::Ref(ocg_nums[handle], 0))
        .collect();
    let off: Vec<Value> = used
        .iter()
        .filter(|&&handle| !groups[handle].default_visible)
        .map(|&handle| Value::Ref(ocg_nums[handle], 0))
        .collect();
    let default = Dict::new()
        .insert("BaseState", Value::Name("ON".into()))
        .insert("ON", Value::Array(Array(on)))
        .insert("OFF", Value::Array(Array(off)))
        .insert("Order", Value::Array(Array(all.clone())));
    Dict::new()
        .insert("OCGs", Value::Array(Array(all)))
        .insert("D", Value::Dict(default))
}

/// 组属性字典（Table 96）：Form XObject 与 Page 的 `/Group` 共用。
fn group_dict(group: &GroupXObject, doc: &mut Document, num: &mut i64) -> Result<Dict, WriteError> {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Group".into()))
        .insert("S", Value::Name("Transparency".into()));
    if let Some(color_space) = &group.color_space {
        dict = dict.insert("CS", color_space_value(color_space, doc, num)?);
    }
    if group.isolated {
        dict = dict.insert("I", Value::Bool(true));
    }
    if group.knockout {
        dict = dict.insert("K", Value::Bool(true));
    }
    Ok(dict)
}

/// 12.3.4：页面缩略图须为常规图像，颜色空间限于 DeviceGray、
/// DeviceRGB 或基于二者的 Indexed。
fn validate_thumbnail(img: &ImageData) -> Result<(), String> {
    if img.image_mask {
        return Err("thumbnail must not be an image mask".into());
    }
    let supported = match &img.color_space {
        ColorSpace::DeviceGray | ColorSpace::DeviceRGB => true,
        ColorSpace::Indexed(indexed) => matches!(
            indexed.base.as_ref(),
            ColorSpace::DeviceGray | ColorSpace::DeviceRGB
        ),
        _ => false,
    };
    if !supported {
        return Err(
            "thumbnail color space must be DeviceGray, DeviceRGB or Indexed based on them".into(),
        );
    }
    Ok(())
}

/// 图像及其从属对象（软蒙版、显式蒙版、替代图像）的对象号。
struct ImageObjectNums {
    base: i64,
    smask: Option<i64>,
    mask: Option<i64>,
    alternates: Vec<ImageObjectNums>,
}

/// 按「基图 → SMask → Mask → 替代图像」顺序分配对象号。
fn allocate_image_objects(img: &ImageData, num: &mut i64) -> ImageObjectNums {
    let base = *num;
    *num += 1;
    let smask = img.smask.as_ref().map(|_| {
        let mask_num = *num;
        *num += 1;
        mask_num
    });
    let mask = if matches!(img.mask, Some(ImageMask::Explicit(_))) {
        let mask_num = *num;
        *num += 1;
        Some(mask_num)
    } else {
        None
    };
    let alternates = img
        .alternates
        .iter()
        .map(|alternate| allocate_image_objects(alternate, num))
        .collect();
    ImageObjectNums {
        base,
        smask,
        mask,
        alternates,
    }
}

/// 写出图像 XObject 及其软蒙版、显式蒙版与替代图像。
///
/// 替代图像以 `<< /Image n 0 R >>` 字典数组挂在基图 `/Alternates`
/// 下（Table 90），自身再作为普通图像流对象写出。
fn write_image(
    doc: &mut Document,
    img: &ImageData,
    nums: &ImageObjectNums,
    num: &mut i64,
) -> Result<(), WriteError> {
    img.validate()
        .map_err(|message| WriteError::InvalidObject(format!("image XObject: {message}")))?;
    let mut stream_dict = if img.image_mask {
        image_mask_dict(img)
    } else {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("XObject".into()))
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(img.width as i64))
            .insert("Height", Value::Integer(img.height as i64))
            .insert("ColorSpace", color_space_value(&img.color_space, doc, num)?)
            .insert("BitsPerComponent", Value::Integer(img.bits as i64));
        if let Some(decode) = &img.decode {
            dict = dict.insert("Decode", real_array(decode));
        }
        if img.interpolate {
            dict = dict.insert("Interpolate", Value::Bool(true));
        }
        if let Some(intent) = img.intent {
            dict = dict.insert("Intent", Value::Name(intent.name().into()));
            doc.features.borrow_mut().insert(PdfFeature::ImageIntent);
        }
        if !nums.alternates.is_empty() {
            let alternates: Vec<Value> = nums
                .alternates
                .iter()
                .map(|alternate| {
                    Value::Dict(Dict::new().insert("Image", Value::Ref(alternate.base, 0)))
                })
                .collect();
            dict = dict.insert("Alternates", Value::Array(Array(alternates)));
            doc.features
                .borrow_mut()
                .insert(PdfFeature::AlternateImages);
        }
        dict
    };
    if let Some(smask_num) = nums.smask {
        stream_dict = stream_dict.insert("SMask", Value::Ref(smask_num, 0));
    }
    match &img.mask {
        None => {}
        Some(ImageMask::Explicit(_)) => {
            if let Some(mask_num) = nums.mask {
                stream_dict = stream_dict.insert("Mask", Value::Ref(mask_num, 0));
            }
            doc.features.borrow_mut().insert(PdfFeature::ImageMaskEntry);
        }
        Some(ImageMask::ColorKey(ranges)) => {
            let array = Array(ranges.iter().map(|value| Value::Integer(*value)).collect());
            stream_dict = stream_dict.insert("Mask", Value::Array(array));
            doc.features.borrow_mut().insert(PdfFeature::ImageMaskEntry);
        }
    }
    doc.set_object(Object::new(
        nums.base,
        0,
        Value::Stream(stream_dict, StreamData::Owned(img.data.clone())),
    ));

    if let (Some(smask_num), Some(smask)) = (nums.smask, &img.smask) {
        let mut mask_dict = Dict::new()
            .insert("Type", Value::Name("XObject".into()))
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(smask.width as i64))
            .insert("Height", Value::Integer(smask.height as i64))
            .insert("ColorSpace", Value::Name("DeviceGray".into()))
            .insert("BitsPerComponent", Value::Integer(smask.bits as i64));
        if let Some(decode) = &smask.decode {
            mask_dict = mask_dict.insert("Decode", real_array(decode));
        }
        if smask.interpolate {
            mask_dict = mask_dict.insert("Interpolate", Value::Bool(true));
        }
        // Table 89：`/Matte` 属于软蒙版图像，分量数按基图颜色空间解释。
        if let Some(matte) = &img.matte {
            mask_dict = mask_dict.insert("Matte", real_array(matte));
            doc.features.borrow_mut().insert(PdfFeature::SoftMaskMatte);
        }
        doc.set_object(Object::new(
            smask_num,
            0,
            Value::Stream(mask_dict, StreamData::Owned(smask.data.clone())),
        ));
        doc.features.borrow_mut().insert(PdfFeature::Transparency);
    }

    if let (Some(mask_num), Some(ImageMask::Explicit(mask))) = (nums.mask, &img.mask) {
        doc.set_object(Object::new(
            mask_num,
            0,
            Value::Stream(image_mask_dict(mask), StreamData::Owned(mask.data.clone())),
        ));
    }

    for (alternate, alternate_nums) in img.alternates.iter().zip(&nums.alternates) {
        write_image(doc, alternate, alternate_nums, num)?;
    }
    Ok(())
}

/// 图像蒙版（`/ImageMask true`）的流字典：不写 `/ColorSpace`，
/// 位深固定为 1。
fn image_mask_dict(mask: &ImageData) -> Dict {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("XObject".into()))
        .insert("Subtype", Value::Name("Image".into()))
        .insert("ImageMask", Value::Bool(true))
        .insert("Width", Value::Integer(mask.width as i64))
        .insert("Height", Value::Integer(mask.height as i64))
        .insert("BitsPerComponent", Value::Integer(1));
    if let Some(decode) = &mask.decode {
        dict = dict.insert("Decode", real_array(decode));
    }
    if mask.interpolate {
        dict = dict.insert("Interpolate", Value::Bool(true));
    }
    dict
}

fn named_space(name: &str, dict: Dict) -> Value {
    Value::Array(Array(vec![Value::Name(name.into()), Value::Dict(dict)]))
}

/// 图片 `/ColorSpace`：设备空间写名字，CIE-Based 写 `[名称 字典]`，
/// ICCBased 额外分配配置文件流对象；Separation/DeviceN 写出备用空间
/// 与 tint 变换函数（Type 0/4 流函数分配独立对象）。
fn color_space_value(
    space: &ColorSpace,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    Ok(match space {
        ColorSpace::DeviceGray | ColorSpace::DeviceRGB | ColorSpace::DeviceCMYK => {
            Value::Name(space.device_name().expect("device color space").into())
        }
        ColorSpace::CalGray(cal) => {
            let mut dict = Dict::new().insert("WhitePoint", real_array(&cal.white_point));
            if let Some(black_point) = cal.black_point {
                dict = dict.insert("BlackPoint", real_array(&black_point));
            }
            if let Some(gamma) = cal.gamma {
                dict = dict.insert("Gamma", Value::Float(gamma));
            }
            named_space("CalGray", dict)
        }
        ColorSpace::CalRGB(cal) => {
            let mut dict = Dict::new().insert("WhitePoint", real_array(&cal.white_point));
            if let Some(black_point) = cal.black_point {
                dict = dict.insert("BlackPoint", real_array(&black_point));
            }
            if let Some(gamma) = cal.gamma {
                dict = dict.insert("Gamma", real_array(&gamma));
            }
            if let Some(matrix) = cal.matrix {
                dict = dict.insert("Matrix", real_array(&matrix));
            }
            named_space("CalRGB", dict)
        }
        ColorSpace::Lab(lab) => {
            let mut dict = Dict::new().insert("WhitePoint", real_array(&lab.white_point));
            if let Some(black_point) = lab.black_point {
                dict = dict.insert("BlackPoint", real_array(&black_point));
            }
            if let Some(range) = lab.range {
                dict = dict.insert("Range", real_array(&range));
            }
            named_space("Lab", dict)
        }
        ColorSpace::ICCBased(icc) => {
            let icc_num = *num;
            *num += 1;
            let mut dict = Dict::new().insert("N", Value::Integer(i64::from(icc.n)));
            if let Some(alternate) = &icc.alternate {
                dict = dict.insert("Alternate", color_space_value(alternate, doc, num)?);
            }
            if let Some(range) = &icc.range {
                dict = dict.insert("Range", real_array(range));
            }
            doc.set_object(Object::new(
                icc_num,
                0,
                Value::Stream(dict, StreamData::Owned(icc.data.clone())),
            ));
            Value::Array(Array(vec![
                Value::Name("ICCBased".into()),
                Value::Ref(icc_num, 0),
            ]))
        }
        ColorSpace::Indexed(indexed) => Value::Array(Array(vec![
            Value::Name("Indexed".into()),
            color_space_value(&indexed.base, doc, num)?,
            Value::Integer(i64::from(indexed.hival)),
            Value::HexString(hex_bytes(&indexed.lookup)),
        ])),
        ColorSpace::Separation(separation) => Value::Array(Array(vec![
            Value::Name("Separation".into()),
            Value::Name(separation.colorant.clone()),
            color_space_value(&separation.alternate, doc, num)?,
            write_function(doc, &separation.tint_transform, num)?,
        ])),
        ColorSpace::DeviceN(devicen) => {
            let names = Array(
                devicen
                    .colorants
                    .iter()
                    .map(|colorant| Value::Name(colorant.clone()))
                    .collect(),
            );
            Value::Array(Array(vec![
                Value::Name("DeviceN".into()),
                Value::Array(names),
                color_space_value(&devicen.alternate, doc, num)?,
                write_function(doc, &devicen.tint_transform, num)?,
            ]))
        }
    })
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}
