//! 对象号分配（三）：可选内容组、渐变与图案——只为实际被引用的句柄分配
//! 对象号，并在此完成组参数校验。

use crate::core::builder::{Action, PdfBuilder};
use crate::core::writer::WriteError;

use super::plan::{FormPlan, GroupPlan};

impl PdfBuilder {
    /// 分配可选内容组、渐变与图案的对象号；顺序与拆分前一致。
    pub(super) fn allocate_group_objects(
        &self,
        forms: &FormPlan,
        num: &mut i64,
    ) -> Result<GroupPlan, WriteError> {
        // 可选内容组：只为页面实际标记过的组分配对象号（注册但未使用
        // 的组不写出，与 Form XObject 一致）。句柄在 `begin_optional_content`
        // 中已校验上界，此处再校验组参数。
        let mut used_ocgs: std::collections::BTreeSet<usize> = self
            .pages
            .iter()
            .flat_map(|p| p.optional_content.iter().copied())
            .collect();
        // 表单级 `/OC` 与表单自有 `/Properties` 引用的组同样需要写出。
        for &handle in &forms.used_forms {
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
            ocg_nums[handle] = *num;
            *num += 1;
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
            shading_nums[handle] = *num;
            *num += 1;
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
            pattern_nums[handle] = *num;
            *num += 1;
        }

        Ok(GroupPlan {
            used_ocgs,
            ocg_nums,
            used_shadings,
            shading_nums,
            used_patterns,
            pattern_nums,
        })
    }
}
