//! 逻辑态 → `Document`：对象号分配、资源字典与字体/图片对象组装。

use super::{Font, ImageData, PdfBuilder, same_font};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

impl PdfBuilder {
    /// 把逻辑态翻译为底层 `Document`。
    pub(super) fn assemble_document(&mut self) -> Document {
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

            // 资源：Font 子字典（/F1 -> ref, ...）、XObject 子字典（/Im1 -> ref, ...）
            let mut font_dict = Dict::new();
            for (k, fnum) in page_font_nums[i].iter().enumerate() {
                font_dict = font_dict.insert(format!("F{}", k + 1), Value::Ref(*fnum, 0));
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
            let mut font_dict = Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(f.name.clone()));
            if let Some(encoding) = f.encoding.to_value() {
                font_dict = font_dict.insert("Encoding", encoding);
            }
            // 写入字体描述符（如果提供）
            if let Some(descriptor) = &f.descriptor {
                let descriptor_ref = num;
                num += 1;
                font_dict = font_dict.insert("FontDescriptor", Value::Ref(descriptor_ref, 0));
                doc.set_object(Object::new(descriptor_ref, 0, descriptor.to_value()));
            }
            doc.set_object(Object::new(*fnum, 0, Value::Dict(font_dict)));
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
        doc
    }
}
