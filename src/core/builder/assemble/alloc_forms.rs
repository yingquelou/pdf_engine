//! 对象号分配（二）：Form XObject 引用集收集（页面绘制、注解 `/AP`/`/MK`/`/RO`、
//! Widget、Rendition、3D 视图 `/O`）与各表单自有字体/图片资源的并入。

use crate::core::builder::types::ThreeDViewSelection;
use crate::core::builder::{Action, GoTo3DViewSelection, PdfBuilder};
use crate::core::writer::WriteError;

use super::form_field;
use super::images::allocate_image_objects;
use super::plan::{FormPlan, PoolPlan, Preflight};

impl PdfBuilder {
    /// 收集实际被引用的 Form XObject 句柄并分配流对象号，同时把各表单
    /// 自有字体/图片并入全局去重池；分配顺序与拆分前一致。
    pub(super) fn allocate_form_objects(
        &self,
        preflight: &Preflight,
        pools: &mut PoolPlan,
        num: &mut i64,
    ) -> Result<FormPlan, WriteError> {
        // Form XObject：只为页面实际引用的表单分配流对象号（按句柄顺序），
        // 注册但未绘制的表单不占用对象号。被 ExtGState 软蒙版引用的表单
        // 同样需要写出，但不登记为页面 XObject 资源。
        let mut used_forms: std::collections::BTreeSet<usize> = self
            .pages
            .iter()
            .flat_map(|p| p.forms.iter().copied())
            .collect();
        for (state, _) in &pools.gs_pool {
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
        if let Some(form) = &preflight.acro_form {
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
        // 3D 视图 `/O` 叠加外观（Table 304、13.6.6）与注解 `/AP` 共用
        // Form XObject 池：`/VA` 各视图、3D 流内联 `/DV` 与注解内联
        // `/3DV` 的叠加表单都须分配对象号（即使未被页面绘制）；越界句柄
        // 在此拒绝。
        for (handle, artwork) in self.three_d_artworks.iter().enumerate() {
            for form in artwork.overlay_handles() {
                if form >= self.forms.len() {
                    return Err(WriteError::InvalidObject(format!(
                        "3D artwork handle {handle}: view /O references unknown form handle {form}"
                    )));
                }
                used_forms.insert(form);
            }
        }
        for (handle, entry) in self.annotations.iter().enumerate() {
            let Some(ThreeDViewSelection::Inline(view)) = entry.annot.three_d_view() else {
                continue;
            };
            let Some(form) = view.overlay() else {
                continue;
            };
            if form >= self.forms.len() {
                return Err(WriteError::InvalidObject(format!(
                    "annotation handle {handle}: /3DV /O references unknown form handle {form}"
                )));
            }
            used_forms.insert(form);
        }
        // `/ExData` 的 `/3DV` 是独立视图字典（13.6.6 Table 313），其 `/O`
        // 叠加表单同样须分配对象号，避免写出时引用未分配的表单。
        for (handle, entry) in self.annotations.iter().enumerate() {
            let Some(markup) = entry.annot.ex_data() else {
                continue;
            };
            let Some(form) = markup.view().overlay() else {
                continue;
            };
            if form >= self.forms.len() {
                return Err(WriteError::InvalidObject(format!(
                    "annotation handle {handle}: /ExData /3DV /O references unknown form handle {form}"
                )));
            }
            used_forms.insert(form);
        }
        // Go-To-3D-View 动作（Table 216）的内联 `/V` 视图同样可带 `/O`。
        for action in self.used_actions() {
            let Action::GoTo3DView(action) = action else {
                continue;
            };
            let GoTo3DViewSelection::Inline(view) = action.selection() else {
                continue;
            };
            let Some(form) = view.overlay() else {
                continue;
            };
            if form >= self.forms.len() {
                return Err(WriteError::InvalidObject(format!(
                    "GoTo3DView action /V /O references unknown form handle {form}"
                )));
            }
            used_forms.insert(form);
        }
        let mut form_nums = vec![0i64; self.forms.len()];
        for &handle in &used_forms {
            form_nums[handle] = *num;
            *num += 1;
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
                let pooled = pools
                    .font_pool
                    .iter()
                    .position(|(g, _)| font.same_dedup_key(g));
                let obj_num = match pooled {
                    Some(i) => pools.font_pool[i].1,
                    None => {
                        let n = *num;
                        *num += 1;
                        pools.font_pool.push((font, n));
                        n
                    }
                };
                fonts.push(obj_num);
            }
            form_font_nums[handle] = fonts;
            let mut images = Vec::with_capacity(resources.images.len());
            for image in resources.images {
                let pooled = pools.image_pool.iter().position(|(g, _)| g == &image);
                let obj_num = match pooled {
                    Some(i) => pools.image_pool[i].1.base,
                    None => {
                        let nums = allocate_image_objects(&image, num);
                        let base = nums.base;
                        pools.image_pool.push((image, nums));
                        base
                    }
                };
                images.push(obj_num);
            }
            form_image_nums[handle] = images;
        }

        Ok(FormPlan {
            used_forms,
            form_nums,
            form_font_nums,
            form_image_nums,
        })
    }
}
