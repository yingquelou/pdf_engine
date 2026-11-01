//! 对象号分配（四）：注解间接对象与页绑定/子类型门禁、AcroForm 字段树
//! 前序号、嵌入文件、声音、3D artwork 与 rendition section 链。

use crate::core::builder::PdfBuilder;
use crate::core::builder::types::AnnotationKind;
use crate::core::types::{PdfFeature, PdfVersion};
use crate::core::writer::WriteError;

use super::plan::{AnnotPlan, Preflight};
use super::{annotation, annotation_exdata, embedded_file, form_field, rendition, sound, three_d};

impl PdfBuilder {
    /// 为注解、字段树与各文档级对象池分配对象号；顺序与拆分前一致。
    ///
    /// 返回的字段树节点借用 [`Preflight`] 克隆出的 AcroForm 配置，故其
    /// 生命周期与 `preflight` 一致。
    pub(super) fn allocate_annotation_objects<'a>(
        &mut self,
        preflight: &'a Preflight,
        num: &mut i64,
    ) -> Result<AnnotPlan<'a>, WriteError> {
        // 注解：注册的每条注解都写成独立间接对象并按句柄分配对象号
        // （`/Popup` 互引需要预先建表）；页绑定、字典参数与子类型版本
        // 在此校验。`/ExData` 的 `/3DA` 目标须为另一条 3D 注解，跨注解
        // 校验同样在任何对象号分配前完成。
        annotation_exdata::validate_markup_3d(
            &self
                .annotations
                .iter()
                .map(|entry| &entry.annot)
                .collect::<Vec<_>>(),
        )?;
        let mut annotation_nums = vec![0i64; self.annotations.len()];
        let mut annotation_pages = vec![0usize; self.annotations.len()];
        let mut page_annotation_handles: Vec<Vec<usize>> = vec![Vec::new(); self.pages.len()];
        let mut page_trap_net_counts: Vec<usize> = vec![0; self.pages.len()];
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
            // `/ExData` 3D 标记（13.6.6）PDF 1.7 门禁。
            annotation::markup_3d_feature_gate(&entry.annot, self.version)?;
            // Link 专有条目按 Table 173 的版本门起分组门禁。
            annotation::link_feature_gate(&entry.annot, self.version)?;
            // Trap 网络跟踪条目（14.11.6 Table 366）PDF 1.4 / 1.3 门禁。
            annotation::trap_network_feature_gate(&entry.annot, self.version)?;
            // 14.11.6：每页至多一个 Trap 网络注解，且其 `/AP /N` 表单
            // （含状态子字典的每个表单）必须已注册并声明 `/PCM`。
            if entry.annot.kind() == AnnotationKind::TrapNet {
                page_trap_net_counts[entry.page] += 1;
                if page_trap_net_counts[entry.page] > 1 {
                    return Err(WriteError::InvalidObject(format!(
                        "at most one trap network annotation per page (page {})",
                        entry.page
                    )));
                }
                let mut appearance_forms: Vec<usize> = entry
                    .annot
                    .appearance_states()
                    .iter()
                    .map(|(_, form)| *form)
                    .collect();
                if let Some(form) = entry.annot.appearance_normal() {
                    appearance_forms.push(form);
                }
                for form in appearance_forms {
                    let form_object = self.forms.get(form).ok_or_else(|| {
                        WriteError::InvalidObject(format!(
                            "TrapNet annotation handle {handle}: appearance references unknown form handle {form}"
                        ))
                    })?;
                    if form_object.pcm.is_none() {
                        return Err(WriteError::InvalidObject(format!(
                            "TrapNet annotation handle {handle}: appearance form handle {form} must declare /PCM"
                        )));
                    }
                }
            }
            page_annotation_handles[entry.page].push(handle);
            annotation_nums[handle] = *num;
            annotation_pages[handle] = entry.page;
            *num += 1;
        }

        // AcroForm 字段树：按前序分配对象号（根到叶），并把各 Widget
        // 对象号按生效页号分组，稍后并入页面 `/Annots`。
        let (field_slots, field_roots) = match &preflight.acro_form {
            Some(form) => form_field::allocate_fields(form, num),
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
        let embedded_slots = embedded_file::allocate(&self.embedded, num);
        let embedded_nums: Vec<i64> = embedded_slots
            .iter()
            .map(|slot| slot.filespec_num)
            .collect();

        // 声音对象（13.3 Table 294）：每个注册的声音分配一个独立流对象号，
        // 供 Sound 动作与 Sound 注解的 `/Sound` 引用；注册校验已在任何
        // 对象号分配前完成。
        let sound_nums = sound::allocate(&self.sounds, num);

        // 3D artwork（13.6 Table 300/303）：每个注册项先分配独立流对象号；
        // 声明 `/OnInstantiate` 时再分配 JavaScript 流对象号，启用引用字典
        // 时再分配 Table 303 字典对象号。注解 `/3DD` 指向字典（若存在），
        // 否则直接指向流。
        let three_d_slots = three_d::allocate(&self.three_d_artworks, num);

        // media clip section（13.2.6 Table 277）：section 链上每个 `/D`
        // 目标分配独立对象号；最外层 clip 内联在 rendition `/C` 中，不占
        // 对象号。分配顺序与注解/大纲实际写出顺序一致，内联写入时由共享
        // 游标按同一顺序取首级目标号。
        let rendition_numbers = {
            let actions = self.used_actions();
            rendition::allocate(&actions, num)
        };
        self.rendition_nums = rendition_numbers.roots;
        self.rendition_targets = rendition_numbers.targets;
        self.rendition_cursor.set(0);

        Ok(AnnotPlan {
            annotation_nums,
            annotation_pages,
            page_annotation_handles,
            page_widget_nums,
            field_slots,
            field_roots,
            embedded_slots,
            embedded_nums,
            sound_nums,
            three_d_slots,
        })
    }
}
