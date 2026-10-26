//! 逻辑态 → `Document`：对象号分配、资源字典与字体/图片对象组装。
//!
//! 主流程保留在本模块；具体对象构造按类别拆分为子模块：
//! - `fonts`：普通字体字典与字体程序嵌入（含 TrueType / Type 1 子集）；
//! - `type3`：Type 3 字体字典与字符过程流；
//! - `cid`：CIDFont 后代字典、Type 0 父字体与相关流；
//! - `cmap`：嵌入编码 CMap 与 ToUnicode CMap。

mod cid;
mod cmap;
mod fonts;
mod shading;
mod type3;

use super::{
    CidFont, ColorSpace, Font, Function, GraphicsState, GraphicsStateFunctions, GroupXObject,
    ImageData, ImageMask, OptionalContentGroup, Pattern, PdfBuilder, same_font,
};
use crate::core::types::{Array, Dict, Document, Object, PdfFeature, StreamData, Trailer, Value};
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
        // 软蒙版与显式蒙版各分配紧随基图的独立流对象号。
        struct ImageObjectNums {
            base: i64,
            smask: Option<i64>,
            mask: Option<i64>,
        }
        let mut image_pool: Vec<(ImageData, ImageObjectNums)> = Vec::new();
        let mut page_image_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.images.len());
            for img in &p.images {
                let pos = image_pool.iter().position(|(g, _)| g == img);
                let obj_num = match pos {
                    Some(i) => image_pool[i].1.base,
                    None => {
                        let base = num;
                        num += 1;
                        let smask = if img.smask.is_some() {
                            let m = num;
                            num += 1;
                            Some(m)
                        } else {
                            None
                        };
                        let mask = if matches!(img.mask, Some(ImageMask::Explicit(_))) {
                            let m = num;
                            num += 1;
                            Some(m)
                        } else {
                            None
                        };
                        image_pool.push((img.clone(), ImageObjectNums { base, smask, mask }));
                        base
                    }
                };
                v.push(obj_num);
            }
            page_image_nums.push(v);
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
        let mut form_nums = vec![0i64; self.forms.len()];
        for &handle in &used_forms {
            form_nums[handle] = num;
            num += 1;
        }

        // 可选内容组：只为页面实际标记过的组分配对象号（注册但未使用
        // 的组不写出，与 Form XObject 一致）。句柄在 `begin_optional_content`
        // 中已校验上界，此处再校验组参数。
        let used_ocgs: std::collections::BTreeSet<usize> = self
            .pages
            .iter()
            .flat_map(|p| p.optional_content.iter().copied())
            .collect();
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

        // 图片 XObject 流对象；软蒙版与显式蒙版作为紧随其后的独立流写出。
        for (img, nums) in &image_pool {
            img.validate().map_err(|message| {
                WriteError::InvalidObject(format!("image XObject: {message}"))
            })?;
            let mut stream_dict = if img.image_mask {
                image_mask_dict(img)
            } else {
                let mut dict = Dict::new()
                    .insert("Type", Value::Name("XObject".into()))
                    .insert("Subtype", Value::Name("Image".into()))
                    .insert("Width", Value::Integer(img.width as i64))
                    .insert("Height", Value::Integer(img.height as i64))
                    .insert(
                        "ColorSpace",
                        color_space_value(&img.color_space, &mut doc, &mut num)?,
                    )
                    .insert("BitsPerComponent", Value::Integer(img.bits as i64));
                if let Some(decode) = &img.decode {
                    dict = dict.insert("Decode", real_array(decode));
                }
                if img.interpolate {
                    dict = dict.insert("Interpolate", Value::Bool(true));
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

        // 函数类条目（/TR /BG /BG2 /UCR /UCR2）：只写被 ExtGState 引用的
        // 函数，同一句柄只写一次。Type 0/4 分配独立流对象并返回引用；
        // Type 2/3 内联为直接字典，Type 3 的子函数由 `write_function`
        // 递归生成。黑版生成与底色去除函数必须产生单一输出值。
        let mut graphics_function_values: std::collections::BTreeMap<usize, Value> =
            std::collections::BTreeMap::new();
        for (state, _) in &gs_pool {
            for (role, handle) in state.function_handles() {
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
            let functions = GraphicsStateFunctions {
                transfer: function_value(state.transfer_function),
                black_generation: function_value(state.black_generation),
                black_generation_2: function_value(state.black_generation_2),
                undercolor_removal: function_value(state.undercolor_removal),
                undercolor_removal_2: function_value(state.undercolor_removal_2),
            };
            let dict = state.to_value(font_num, soft_mask_group_num, functions)?;
            doc.set_object(Object::new(*gs_num, 0, Value::Dict(dict)));
        }

        // trailer：/Root -> Catalog(1)。/Size 由 writer 按**实际写出**的
        // 最大对象号 + 1 计算（经典表用 `ctx.max_num + 1`，XRef 流用自身
        // 分配到的对象号），此处显式给出以保证与对象号一致。
        let max_num = num - 1;
        doc.trailer = Trailer {
            dict: Dict::new()
                .insert("Root", Value::Ref(catalog_num, 0))
                .insert("Size", Value::Integer(max_num + 1)),
        };
        Ok(doc)
    }
}

fn real_array(values: &[f64]) -> Value {
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
