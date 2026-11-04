//! ExtGState 对象写出：先按引用写出函数类条目（`/TR` `/BG` `/BG2` `/UCR`
//! `/UCR2`、`/SMask /TR` 与 `/HT` 内函数），再写图形状态参数字典。

use crate::core::builder::{GraphicsStateFunctions, PdfBuilder, same_font};
use crate::core::types::{Document, Object, PdfFeature, Value};
use crate::core::writer::WriteError;

use super::plan::{FormPlan, PoolPlan};
use super::utils::write_function;

impl PdfBuilder {
    /// 写出被 ExtGState 引用的函数与图形状态参数对象；写出顺序与拆分前
    /// 一致（同一函数句柄只写一次）。
    pub(super) fn assemble_graphics_objects(
        &self,
        doc: &mut Document,
        num: &mut i64,
        pools: &PoolPlan,
        forms: &FormPlan,
    ) -> Result<(), WriteError> {
        // 混合色彩空间 `/CS` 与 `/CSS` 是透明模型（§11.6.3）的 PDF 1.4 条目；
        // 走本仓库既有的 `PdfFeature::Transparency` 门禁：先登记特征、再让
        // writer 的 `validate_content_version` 对照目标版本给出
        // `UnsupportedVersion`。装配阶段仍返回 Result，构建失败保持零字节。
        if pools
            .gs_pool
            .iter()
            .any(|(state, _)| state.hybrid_color_space.is_some())
        {
            doc.features.borrow_mut().insert(PdfFeature::Transparency);
        }

        // 函数类条目（/TR /BG /BG2 /UCR /UCR2，以及 /SMask /TR 与 /HT 内
        // 函数）：只写被 ExtGState 引用的函数，同一句柄只写一次。Type 0/4
        // 分配独立流对象并返回引用；Type 2/3 内联为直接字典，Type 3 的子
        // 函数由 `write_function` 递归生成。除 ExtGState `/TR` 外均要求
        // 产生单一输出值。
        let mut graphics_function_values: std::collections::BTreeMap<usize, Value> =
            std::collections::BTreeMap::new();
        for (state, _) in &pools.gs_pool {
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
                    let value = write_function(doc, function, num)?;
                    entry.insert(value);
                }
            }
        }

        // 图形状态参数对象：/Font 条目写成字体字典的间接引用。
        for (state, gs_num) in &pools.gs_pool {
            let font_num = match &state.font {
                Some(gs_font) => Some(
                    pools
                        .font_pool
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
                .map(|soft_mask| forms.form_nums[soft_mask.group]);
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
        Ok(())
    }
}
