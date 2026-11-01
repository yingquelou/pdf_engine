//! 资源对象写出：字体（普通与 Type 3）、CIDFont 树、图片 XObject 与从属流，
//! 以及可选内容组、渐变与图案对象。

use crate::core::builder::{Pattern, PdfBuilder};
use crate::core::types::{Document, Object, Value};
use crate::core::writer::WriteError;

use super::images::write_image;
use super::plan::{GroupPlan, PoolPlan};
use super::shading::{pattern_dict, shading_object_value, tiling_pattern_stream};
use super::{cid, fonts, type3};

impl PdfBuilder {
    /// 写出字体、CIDFont 与图片 XObject（含软蒙版、显式蒙版与替代图像）。
    pub(super) fn assemble_font_and_image_objects(
        &self,
        doc: &mut Document,
        num: &mut i64,
        pools: &PoolPlan,
    ) -> Result<(), WriteError> {
        // 字体对象
        for (f, fnum) in &pools.font_pool {
            // Type 3 字体自成一路：字符过程代替字体程序，没有 BaseFont。
            let font_dict = match &f.type3 {
                Some(type3) => type3::type3_font_dict(doc, f, type3, num)?,
                None => fonts::font_dict(doc, f, num)?,
            };
            doc.set_object(Object::new(*fnum, 0, Value::Dict(font_dict)));
        }

        // CIDFont 对象：Type 0 父 + CIDFont 后代 + FontDescriptor + CIDSet 流
        for (cid_font, type0_num) in &pools.cid_font_pool {
            cid::write_cid_font(doc, cid_font, *type0_num, num)?;
        }

        // 图片 XObject 流对象；软蒙版、显式蒙版与替代图像作为紧随其后的
        // 独立流写出（替代图像不进入页面资源字典）。
        for (img, nums) in &pools.image_pool {
            write_image(doc, img, nums, num)?;
        }
        Ok(())
    }

    /// 写出可选内容组、渐变与图案对象。
    pub(super) fn assemble_document_group_objects(
        &mut self,
        doc: &mut Document,
        num: &mut i64,
        groups: &GroupPlan,
    ) -> Result<(), WriteError> {
        // 可选内容组对象（Table 99）：/Type /OCG + 文本串名称，可选 /Intent。
        for &handle in &groups.used_ocgs {
            let dict = self.optional_content_groups[handle]
                .to_dict()
                .map_err(|message| {
                    WriteError::InvalidObject(format!("optional content group: {message}"))
                })?;
            doc.set_object(Object::new(groups.ocg_nums[handle], 0, Value::Dict(dict)));
        }

        // 渐变对象（Table 78 / Table 84 / Table 85）：Type 1–3 为字典，
        // Type 4/5 为位打包流；函数经 `write_function` 递归写出
        // （Type 0/4 分配独立流对象）。
        for &handle in &groups.used_shadings {
            let value = shading_object_value(&self.shadings[handle], doc, num)?;
            doc.set_object(Object::new(groups.shading_nums[handle], 0, value));
        }

        // 图案对象：PatternType 2 为字典（内联渐变），PatternType 1 为
        // 带内容流的平铺图案（Table 75 / Table 77）。
        for &handle in &groups.used_patterns {
            let value = match &mut self.patterns[handle] {
                Pattern::Shading(pattern) => Value::Dict(pattern_dict(pattern, doc, num)?),
                Pattern::Tiling(pattern) => tiling_pattern_stream(pattern, doc, num)?,
            };
            doc.set_object(Object::new(groups.pattern_nums[handle], 0, value));
        }
        Ok(())
    }
}
