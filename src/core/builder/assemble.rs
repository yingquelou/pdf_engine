//! 逻辑态 → `Document`：对象号分配、资源字典与字体/图片对象组装。

use super::{
    CidEncoding, CidFont, CidFontSubtype, Font, FontProgram, ImageData, PdfBuilder, same_font,
};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};
use crate::core::writer::WriteError;
use std::collections::BTreeMap;

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
            // 嵌入 TrueType 程序时 `/Subtype` 必须是 TrueType；
            // Type 1 与 CFF（FontFile3 /Type1C）都使用 Type1 子类型。
            let subtype = match &f.font_program {
                Some(FontProgram::TrueType(_)) => "TrueType",
                _ => "Type1",
            };
            // 子集字体：BaseFont 加六字母标签前缀（PDF 32000-1 §9.6.4）。
            let base_font = match (&f.font_program, f.subset) {
                (Some(program), true) => format!("{}+{}", program.subset_tag(), f.name),
                _ => f.name.clone(),
            };
            let mut font_dict = Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name(subtype.into()))
                .insert("BaseFont", Value::Name(base_font));
            if let Some(encoding) = f.encoding.to_value() {
                font_dict = font_dict.insert("Encoding", encoding);
            }
            // 写入字体描述符（如果提供或有字体程序）
            if f.descriptor.is_some() || f.font_program.is_some() {
                let descriptor_ref = num;
                num += 1;
                font_dict = font_dict.insert("FontDescriptor", Value::Ref(descriptor_ref, 0));

                // 构建字体描述符字典
                let mut descriptor_dict = if let Some(descriptor) = &f.descriptor {
                    match descriptor.to_value() {
                        Value::Dict(d) => d,
                        _ => Dict::new().insert("Type", Value::Name("FontDescriptor".into())),
                    }
                } else {
                    Dict::new().insert("Type", Value::Name("FontDescriptor".into()))
                };

                // 写入字体程序流（如果有）
                if let Some(program) = &f.font_program {
                    let program_ref = num;
                    num += 1;

                    // builder 构造的 Owned 流按 writer 约定写明文；writer 会把
                    // 声明在 Owned 流上的 FlateDecode 视为读端已解压而移除。
                    let stream_dict = match program {
                        FontProgram::Type1(_) => {
                            // /FontFile - Type 1 字体程序
                            descriptor_dict =
                                descriptor_dict.insert("FontFile", Value::Ref(program_ref, 0));
                            Dict::new()
                        }
                        FontProgram::TrueType(_) => {
                            // /FontFile2 - TrueType 字体程序
                            descriptor_dict =
                                descriptor_dict.insert("FontFile2", Value::Ref(program_ref, 0));
                            Dict::new()
                        }
                        FontProgram::CFF(_) => {
                            // /FontFile3 - CFF 字体程序
                            descriptor_dict =
                                descriptor_dict.insert("FontFile3", Value::Ref(program_ref, 0));
                            Dict::new().insert("Subtype", Value::Name("Type1C".into()))
                        }
                    }
                    .insert("Length", Value::Integer(program.data().len() as i64));

                    doc.set_object(Object::new(
                        program_ref,
                        0,
                        Value::Stream(stream_dict, StreamData::Owned(program.data().to_vec())),
                    ));
                }

                doc.set_object(Object::new(descriptor_ref, 0, Value::Dict(descriptor_dict)));
            }
            doc.set_object(Object::new(*fnum, 0, Value::Dict(font_dict)));
        }

        // CIDFont 对象：Type 0 父 + CIDFont 后代 + FontDescriptor + CIDSet 流
        for (cid_font, type0_num) in &cid_font_pool {
            // 自定义编码在 build 阶段校验：wmode 仅 0/1，且至少一条映射。
            if let Some(encoding) = &cid_font.encoding {
                if encoding.wmode > 1 {
                    return Err(WriteError::InvalidObject(format!(
                        "CidEncoding wmode must be 0 or 1, got {}",
                        encoding.wmode
                    )));
                }
                if encoding.mappings.is_empty() {
                    return Err(WriteError::InvalidObject(
                        "CidEncoding requires at least one code-to-CID mapping".into(),
                    ));
                }
            }

            // 1. FontDescriptor 字典（/CIDSet 作为独立流对象引用）
            let descriptor_num = num;
            num += 1;
            let mut descriptor_value = cid_font.descriptor.to_value();
            if let Some(cid_set_data) = cid_font.descriptor.cid_set_data() {
                let cid_set_num = num;
                num += 1;
                if let Value::Dict(ref d) = descriptor_value {
                    descriptor_value =
                        Value::Dict(d.clone().insert("CIDSet", Value::Ref(cid_set_num, 0)));
                }
                let cid_set_len = cid_set_data.len();
                doc.set_object(Object::new(
                    cid_set_num,
                    0,
                    Value::Stream(
                        Dict::new().insert("Length", Value::Integer(cid_set_len as i64)),
                        StreamData::Owned(cid_set_data.to_vec()),
                    ),
                ));
            }
            // 嵌入 CIDFont 字体程序：Type2 → /FontFile2，Type0 → /FontFile3
            // （流 /Subtype /CIDFontType0C）。builder 的 Owned 流写明文。
            if let Some(program) = &cid_font.font_program {
                let (program_key, stream_subtype) = match (cid_font.subtype, program) {
                    (CidFontSubtype::CIDFontType2, FontProgram::TrueType(_)) => ("FontFile2", None),
                    (CidFontSubtype::CIDFontType0, FontProgram::CFF(_)) => {
                        ("FontFile3", Some("CIDFontType0C"))
                    }
                    (subtype, _) => {
                        let expected = match subtype {
                            CidFontSubtype::CIDFontType2 => "TrueType",
                            CidFontSubtype::CIDFontType0 => "CFF",
                        };
                        return Err(WriteError::InvalidObject(format!(
                            "{} requires a {expected} font program",
                            subtype.as_name()
                        )));
                    }
                };
                let program_num = num;
                num += 1;
                let mut stream_dict = Dict::new();
                if let Some(subtype) = stream_subtype {
                    stream_dict = stream_dict.insert("Subtype", Value::Name(subtype.into()));
                }
                stream_dict =
                    stream_dict.insert("Length", Value::Integer(program.data().len() as i64));
                doc.set_object(Object::new(
                    program_num,
                    0,
                    Value::Stream(stream_dict, StreamData::Owned(program.data().to_vec())),
                ));
                if let Value::Dict(d) = descriptor_value {
                    descriptor_value =
                        Value::Dict(d.insert(program_key, Value::Ref(program_num, 0)));
                }
            }
            doc.set_object(Object::new(descriptor_num, 0, descriptor_value));

            // 2. CIDFont 后代字典
            let cid_font_num = num;
            num += 1;
            let mut cid_dict = Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name(cid_font.subtype.as_name().into()))
                .insert("BaseFont", Value::Name(cid_font.name.clone()))
                .insert("CIDSystemInfo", cid_font.system_info.to_value())
                .insert("FontDescriptor", Value::Ref(descriptor_num, 0));
            if (cid_font.default_width - 1000.0).abs() > f64::EPSILON {
                cid_dict = cid_dict.insert("DW", Value::Float(cid_font.default_width));
            }
            if !cid_font.widths.is_empty() {
                cid_dict = cid_dict.insert("W", cid_font.widths_to_value());
            }
            doc.set_object(Object::new(cid_font_num, 0, Value::Dict(cid_dict)));

            // 3. Type 0 父字体字典；有 CID→Unicode 映射时附带 /ToUnicode CMap
            let mut type0_dict = Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name(cid_font.name.clone()))
                .insert("Encoding", Value::Name("Identity-H".into()))
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(Value::Ref(cid_font_num, 0))),
                );
            if let Some(encoding) = &cid_font.encoding {
                // 非 Identity 编码：嵌入 CMap 流作为 /Encoding 引用。
                let encoding_num = num;
                num += 1;
                let cmap = cid_encoding_cmap(encoding);
                let cmap_len = cmap.len();
                doc.set_object(Object::new(
                    encoding_num,
                    0,
                    Value::Stream(
                        Dict::new().insert("Length", Value::Integer(cmap_len as i64)),
                        StreamData::Owned(cmap),
                    ),
                ));
                type0_dict = type0_dict.insert("Encoding", Value::Ref(encoding_num, 0));
            }
            if !cid_font.to_unicode.is_empty() {
                let cmap_num = num;
                num += 1;
                let cmap = to_unicode_cmap(&cid_font.to_unicode);
                let cmap_len = cmap.len();
                doc.set_object(Object::new(
                    cmap_num,
                    0,
                    Value::Stream(
                        Dict::new().insert("Length", Value::Integer(cmap_len as i64)),
                        StreamData::Owned(cmap),
                    ),
                ));
                type0_dict = type0_dict.insert("ToUnicode", Value::Ref(cmap_num, 0));
            }
            doc.set_object(Object::new(*type0_num, 0, Value::Dict(type0_dict)));
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

/// 生成 Type 0 嵌入编码 CMap（PDF 32000-1 §9.7.5）。
///
/// 仅支持 2 字节字符码，codespace 固定 `<0000> <FFFF>`。字符码与 CID
/// 同时逐 1 递增的极大段写 `cidrange`，其余写 `cidchar`；每节至多 100 条
/// （与 §9.10.3 的分段约定一致）。CID 按 PDF CMap 语法写十进制整数。
fn cid_encoding_cmap(encoding: &CidEncoding) -> Vec<u8> {
    /// cidrange / cidchar 单节条目上限。
    const SECTION_LIMIT: usize = 100;

    let entries: Vec<(u16, u16)> = encoding
        .mappings
        .iter()
        .map(|(&code, &cid)| (code, cid))
        .collect();

    // 找出极大连续段：字符码与 CID 同时逐 1 递增。以 u32 运算，
    // u16::MAX 之后无法再递增，天然避免回绕。
    let mut in_range = vec![false; entries.len()];
    let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let mut end = start;
        while end + 1 < entries.len() {
            let next_code = u32::from(entries[end].0) + 1;
            let next_cid = u32::from(entries[end].1) + 1;
            if next_code != u32::from(entries[end + 1].0)
                || next_cid != u32::from(entries[end + 1].1)
            {
                break;
            }
            end += 1;
        }
        if end > start {
            ranges.push((entries[start].0, entries[end].0, entries[start].1));
            in_range[start..=end].fill(true);
        }
        start = end + 1;
    }

    let wmode = encoding.wmode;
    let suffix = if wmode == 1 { "V" } else { "H" };
    let mut cmap = format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CMapName /Custom-{suffix} def\n\
         /CMapType 1 def\n\
         /WMode {wmode} def\n\
         1 begincodespacerange\n\
         <0000> <FFFF>\n\
         endcodespacerange\n"
    );
    for chunk in ranges.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} begincidrange\n", chunk.len()));
        for (start_code, end_code, start_cid) in chunk {
            cmap.push_str(&format!(
                "<{start_code:04X}> <{end_code:04X}> {start_cid}\n"
            ));
        }
        cmap.push_str("endcidrange\n");
    }

    let chars: Vec<(u16, u16)> = entries
        .iter()
        .zip(&in_range)
        .filter(|(_, covered)| !**covered)
        .map(|(&entry, _)| entry)
        .collect();
    for chunk in chars.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} begincidchar\n", chunk.len()));
        for (code, cid) in chunk {
            cmap.push_str(&format!("<{code:04X}> {cid}\n"));
        }
        cmap.push_str("endcidchar\n");
    }

    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap.into_bytes()
}

/// 生成标准 ToUnicode CMap：2 字节 codespace；CID 连续且目标为单个
/// UTF-16 码元、并逐 1 递增的极大段写 `bfrange`，其余（代理对、多码元
/// 串、目标跳变）写 `bfchar`；每节至多 100 条（PDF 32000-1 §9.10.3）。
fn to_unicode_cmap(mappings: &BTreeMap<u16, String>) -> Vec<u8> {
    /// bfchar / bfrange 单节条目上限。
    const SECTION_LIMIT: usize = 100;

    // 仅「恰好一个 UTF-16 码元」的目标可参与 bfrange；
    // 代理对与多码元串只能逐条写 bfchar。
    let entries: Vec<(u16, &str, Option<u16>)> = mappings
        .iter()
        .map(|(cid, text)| {
            let mut units = text.encode_utf16();
            let first = units.next();
            let single = if units.next().is_none() { first } else { None };
            (*cid, text.as_str(), single)
        })
        .collect();

    // 找出极大连续段：CID 与目标码元同时逐 1 递增。以 u32 运算，
    // u16::MAX 之后无法再递增，天然避免回绕。
    let mut in_range = vec![false; entries.len()];
    let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let Some(first_unit) = entries[start].2 else {
            start += 1;
            continue;
        };
        let mut end = start;
        while end + 1 < entries.len() {
            let Some(next_unit) = entries[end + 1].2 else {
                break;
            };
            let offset = (end - start) as u32 + 1;
            if u32::from(entries[end].0) + 1 != u32::from(entries[end + 1].0)
                || u32::from(first_unit) + offset != u32::from(next_unit)
            {
                break;
            }
            end += 1;
        }
        if end > start {
            ranges.push((entries[start].0, entries[end].0, first_unit));
            in_range[start..=end].fill(true);
        }
        start = end + 1;
    }

    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n\
         <0000> <FFFF>\n\
         endcodespacerange\n",
    );
    for chunk in ranges.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} beginbfrange\n", chunk.len()));
        for (start_cid, end_cid, unit) in chunk {
            cmap.push_str(&format!("<{start_cid:04X}> <{end_cid:04X}> <{unit:04X}>\n"));
        }
        cmap.push_str("endbfrange\n");
    }

    let chars: Vec<(u16, &str)> = entries
        .iter()
        .zip(&in_range)
        .filter(|(_, covered)| !**covered)
        .map(|((cid, text, _), _)| (*cid, *text))
        .collect();
    for chunk in chars.chunks(SECTION_LIMIT) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (cid, text) in chunk {
            cmap.push_str(&format!("<{cid:04X}> <"));
            for unit in text.encode_utf16() {
                cmap.push_str(&format!("{unit:04X}"));
            }
            cmap.push_str(">\n");
        }
        cmap.push_str("endbfchar\n");
    }

    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap.into_bytes()
}
