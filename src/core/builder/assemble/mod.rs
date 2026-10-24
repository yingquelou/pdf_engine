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
mod type3;

use super::{CidFont, Font, ImageData, PdfBuilder, same_font};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};
use crate::core::writer::WriteError;

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
        let mut image_pool: Vec<(ImageData, i64)> = Vec::new();
        let mut page_image_nums: Vec<Vec<i64>> = Vec::with_capacity(self.pages.len());
        for p in &self.pages {
            let mut v = Vec::with_capacity(p.images.len());
            for img in &p.images {
                let pos = image_pool.iter().position(|(g, _)| g == img);
                let obj_num = match pos {
                    Some(i) => image_pool[i].1,
                    None => {
                        let n = num;
                        num += 1;
                        image_pool.push((img.clone(), n));
                        n
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

        // —— 构造 Document ——
        let mut doc = Document::new();

        // Catalog
        doc.set_object(Object::new(
            catalog_num,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Catalog".into()))
                    .insert("Pages", Value::Ref(pages_num, 0)),
            ),
        ));

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

            let mut resources = Dict::new();
            if !font_dict.0.is_empty() {
                resources = resources.insert("Font", Value::Dict(font_dict));
            }
            if !xobj_dict.0.is_empty() {
                resources = resources.insert("XObject", Value::Dict(xobj_dict));
            }

            doc.set_object(Object::new(
                page_num,
                0,
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("Page".into()))
                        .insert("Parent", Value::Ref(pages_num, 0))
                        .insert("MediaBox", Value::Array(mediabox))
                        .insert("Contents", Value::Ref(content_num, 0))
                        .insert("Resources", Value::Dict(resources)),
                ),
            ));

            // 内容流：让 ContentStream 序列化字节，同时合并其特征画像
            // 到 Document（供 writer::validate_content_version 做版本门控）。
            let content_bytes = p.content.to_bytes();
            for f in p.content.features().iter() {
                doc.features.borrow_mut().insert(*f);
            }
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


        // 图片 XObject 流对象
        for (img, inum) in &image_pool {
            let stream_dict = Dict::new()
                .insert("Type", Value::Name("XObject".into()))
                .insert("Subtype", Value::Name("Image".into()))
                .insert("Width", Value::Integer(img.width as i64))
                .insert("Height", Value::Integer(img.height as i64))
                .insert("ColorSpace", Value::Name(img.color_space.as_name().into()))
                .insert("BitsPerComponent", Value::Integer(img.bits as i64));
            doc.set_object(Object::new(
                *inum,
                0,
                Value::Stream(stream_dict, StreamData::Owned(img.data.clone())),
            ));
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
