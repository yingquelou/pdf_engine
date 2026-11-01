//! 间接对象写出（页字典之后）：注解字典、字段/Widget 字典与嵌入文件、声音、3D、
//! media clip section 各流对象，并回填最近一次分配的对象号供导航层解析引用。

use crate::core::builder::PdfBuilder;
use crate::core::types::{Document, Object, Value};
use crate::core::writer::WriteError;

use super::plan::{AnnotPlan, FormPlan, GroupPlan, PoolPlan};
use super::{annotation, annotation_popup, embedded_file, form_field, rendition, sound, three_d};

impl PdfBuilder {
    /// 写出注解、字段树与文档级对象的间接对象，并回填对象号表。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assemble_indirect_objects(
        &mut self,
        doc: &mut Document,
        pools: &PoolPlan,
        forms: &FormPlan,
        groups: &GroupPlan,
        annot_plan: &AnnotPlan<'_>,
    ) -> Result<(), WriteError> {
        // 注解间接对象：在页字典写出后组装，`/Dest` 与 `GoTo` 动作的
        // 页号在此解析为 Page 引用，`/Popup` 句柄解析为注解引用，
        // `/FS` 句柄解析为文件规格引用。
        // 注意 `page_slots` 是 (Page 号, 内容流号) 对，须先映射为
        // (对象号, 生成号) 引用表再交给导航层。
        let page_refs: Vec<(i64, i64)> = pools.page_slots.iter().map(|(pn, _)| (*pn, 0)).collect();
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
                annotation_nums: &annot_plan.annotation_nums,
                form_nums: &forms.form_nums,
                embedded_nums: &annot_plan.embedded_nums,
                ocg_nums: &groups.ocg_nums,
                sound_nums: &annot_plan.sound_nums,
                three_d_nums: &annot_plan.three_d_slots.annotation_nums,
                three_d_artworks: &self.three_d_artworks,
                rendition_nums: &self.rendition_nums,
                rendition_cursor: Some(&self.rendition_cursor),
            };
            let dict =
                annotation::annotation_dict(&entry.annot, handle, &popup_pairs, &page_refs, &refs)?;
            doc.set_object(Object::new(
                annot_plan.annotation_nums[handle],
                0,
                Value::Dict(dict),
            ));
        }
        // 导航层随后写大纲 `/A`；Hide 与 Set-OCG-State 动作的目标句柄
        // 在此解析为对象号。
        self.annotation_nums = annot_plan.annotation_nums.clone();
        self.annotation_pages = annot_plan.annotation_pages.clone();
        self.ocg_nums = groups.ocg_nums.clone();
        self.sound_nums = annot_plan.sound_nums.clone();
        self.form_nums = forms.form_nums.clone();
        self.embedded_nums = annot_plan.embedded_nums.clone();

        // 表单字段/Widget 间接对象：`/P` 指向页面字典，`/Kids`/`/Parent`
        // 已按字段树对象号互引，`/AP` 解析为 Form XObject 流引用。
        for slot in &annot_plan.field_slots {
            let dict = form_field::field_dict(
                slot,
                &annot_plan.field_slots,
                &page_refs,
                &forms.form_nums,
            )?;
            doc.set_object(Object::new(slot.num, 0, Value::Dict(dict)));
        }

        // 嵌入文件对对象：文件规格 + 嵌入文件流。
        embedded_file::write_objects(
            doc,
            &self.embedded,
            &annot_plan.embedded_slots,
            self.version,
        );

        // 声音流对象（13.3 Table 294）：对象号已在 `annotation_nums` 之前分配。
        sound::write_objects(doc, &self.sounds, &annot_plan.sound_nums);

        // 3D artwork 流、`/OnInstantiate` JavaScript 流与引用字典
        // （13.6 Table 300/303）：对象号已在上方分配；`form_nums` 供视图
        // `/O` 引用叠加外观。
        three_d::write_objects(
            doc,
            &self.three_d_artworks,
            &annot_plan.three_d_slots,
            &forms.form_nums,
        );

        // media clip section 目标对象（13.2.6 Table 277）：`/D` 逐级引用；
        // 对象号已在注解与大纲写出前分配。
        {
            let actions = self.used_actions();
            rendition::write_objects(
                doc,
                &actions,
                &self.rendition_targets,
                &forms.form_nums,
                &annot_plan.embedded_nums,
            )?;
        }
        Ok(())
    }
}
