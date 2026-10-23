//! PDF 逻辑抽象层：以「页 / 文字 / 图片」等逻辑概念组织内容，
//! 内部翻译为底层 [`crate::pdf::types`] 对象，再由
//! [`crate::pdf::writer::DocumentWriter`] 序列化。
//!
//! 这是构建 PDF 的第二层 API：调用方不需要直接拼 `Dict`/`Object`，
//! 只需声明「在第几页什么位置写什么字、放什么图」，由 [`PdfBuilder`]
//! 负责对象号分配、资源字典组装、内容流操作符生成。
//!
//! # 对象号布局
//!
//! 顺序固定，便于测试断言：
//! 1. Catalog（对象 1）
//! 2. Pages（对象 2）
//! 3. 每页：Page 字典对象、内容流对象
//! 4. 字体对象（每个用到的标准字体一个，含可配置的 `/Encoding`）
//! 5. 图片 XObject 流对象（每张一个）
//!
//! trailer 的 `/Root` 指向 Catalog(1)，`/Size` 由 writer 自动取
//! `最大对象号 + 1`。

use crate::pdf::content::ContentStream;
use crate::pdf::reader::font::{
    PredefinedEncoding, code_for_char, default_encoding, predefined_encoding,
};
use crate::pdf::types::{Array, Dict, Document, Object, PdfVersion, StreamData, Trailer, Value};
use crate::pdf::writer::{DocumentWriter, WriteError};
use std::collections::BTreeMap;
use std::io::Write;

/// 页面尺寸预设。
///
/// 提供 A4、Letter 两种常用规格（单位 pt），以及可选的自定义宽高。
/// 用于 [`PdfBuilder::page`] 声明新页的 MediaBox。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    /// A4 纵向：595 × 842 pt。
    A4,
    /// Letter 纵向：612 × 792 pt。
    Letter,
    /// 自定义宽高（pt）。
    Custom(f64, f64),
}

impl PageSize {
    /// 返回 `(width, height)`，单位 pt。
    fn dimensions(&self) -> (f64, f64) {
        match self {
            PageSize::A4 => (595.0, 842.0),
            PageSize::Letter => (612.0, 792.0),
            PageSize::Custom(w, h) => (*w, *h),
        }
    }
}

/// 简单字体 `/Encoding` 的写端配置。
///
/// 默认取 [`FontEncoding::Named`] 的 `WinAnsiEncoding`，与既有输出一致。
/// [`FontEncoding::BuiltIn`] 省略 `/Encoding`，让字体程序的内建编码生效；
/// [`FontEncoding::Custom`] 写入 `/BaseEncoding` 与 `/Differences` 覆盖。
///
/// `text` 会按此配置把文本转成单字节字符码：五个预定义编码名可识别，
/// 其余名称无法转码时退回原始 UTF-8 字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontEncoding {
    /// 省略 `/Encoding`（使用字体内建编码，如 Standard 14 的 Standard）。
    BuiltIn,
    /// 预定义编码名（不含前导 `/`），如 `MacRomanEncoding`。
    Named(String),
    /// 编码字典。
    Custom {
        /// `/BaseEncoding` 名（不含前导 `/`）；`None` 表示省略。
        base: Option<String>,
        /// 字符码 → glyph 名覆盖项，写出时按字符码升序排列。
        differences: BTreeMap<u8, String>,
    },
}

impl Default for FontEncoding {
    fn default() -> Self {
        Self::Named("WinAnsiEncoding".to_string())
    }
}

impl FontEncoding {
    /// 指定预定义编码名。
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// 带 `/Differences` 的编码字典；`base` 为 `None` 时省略 `/BaseEncoding`。
    ///
    /// 同一字符码重复出现时，后定义者覆盖先定义者。
    pub fn with_differences(base: Option<&str>, differences: Vec<(u8, String)>) -> Self {
        Self::Custom {
            base: base.map(str::to_string),
            differences: differences.into_iter().collect(),
        }
    }

    /// 解析基础预定义编码；编码名无法识别或 `/BaseEncoding` 无效时返回 `None`。
    fn base_encoding(&self, base_font: &str) -> Option<PredefinedEncoding> {
        match self {
            Self::BuiltIn => Some(default_encoding(base_font)),
            Self::Named(name) => predefined_encoding(name).ok(),
            Self::Custom {
                base: Some(name), ..
            } => predefined_encoding(name).ok(),
            Self::Custom { base: None, .. } => Some(default_encoding(base_font)),
        }
    }

    /// 把文本按编码转成单字节字符码。
    ///
    /// 无法表示的字符替换为 `?`（0x3F）；基础编码无法识别时返回 `None`，
    /// 调用方回退为原始 UTF-8 字节。
    fn encode_text(&self, base_font: &str, text: &str) -> Option<Vec<u8>> {
        let base = self.base_encoding(base_font)?;
        let empty = BTreeMap::new();
        let differences = match self {
            Self::Custom { differences, .. } => differences,
            _ => &empty,
        };
        Some(
            text.chars()
                .map(|ch| code_for_char(base, differences, ch).unwrap_or(b'?'))
                .collect(),
        )
    }

    /// 转换为字体字典的 `/Encoding` 值；`BuiltIn` 无对应值。
    fn to_value(&self) -> Option<Value> {
        match self {
            Self::BuiltIn => None,
            Self::Named(name) => Some(Value::Name(name.clone())),
            Self::Custom { base, differences } => {
                let mut dict = Dict::new();
                if let Some(base) = base {
                    dict = dict.insert("BaseEncoding", Value::Name(base.clone()));
                }
                if !differences.is_empty() {
                    let mut arr = Array::new();
                    for (code, name) in differences {
                        arr = arr
                            .push(Value::Integer(i64::from(*code)))
                            .push(Value::Name(name.clone()));
                    }
                    dict = dict.insert("Differences", Value::Array(arr));
                }
                Some(Value::Dict(dict))
            }
        }
    }
}

/// 逻辑字体描述。
///
/// 本层仅支持 PDF 标准内置 Type1 字体（如 Helvetica），无需子集嵌入。
/// 字段 [`Font::name`] 指定 BaseFont 名，[`Font::size`] 指定字号 pt，
/// [`Font::encoding`] 控制字体字典的 `/Encoding`（默认 WinAnsiEncoding），
/// 可通过 [`Font::helvetica`] 便捷构造常用 Helvetica、用
/// [`Font::with_encoding`] 替换编码配置。
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// BaseFont 名（不含前导 `/`），如 `Helvetica`。
    pub name: String,
    /// 字号（pt）。
    pub size: f64,
    /// `/Encoding` 写端配置。
    pub encoding: FontEncoding,
}

impl Font {
    /// Helvetica 标准字体（默认 WinAnsiEncoding）。
    pub fn helvetica(size: f64) -> Self {
        Self {
            name: "Helvetica".to_string(),
            size,
            encoding: FontEncoding::default(),
        }
    }

    /// 替换编码配置（链式构造）。
    pub fn with_encoding(mut self, encoding: FontEncoding) -> Self {
        self.encoding = encoding;
        self
    }
}

/// 字体去重键：BaseFont 名、字号位模式与编码配置都相同才算同一字体对象。
fn same_font(a: &Font, b: &Font) -> bool {
    a.name == b.name && a.size.to_bits() == b.size.to_bits() && a.encoding == b.encoding
}

/// 图片色彩空间。
///
/// 目前仅支持 DeviceGray（灰度）与 DeviceRGB（真彩），作为
/// [`ImageData::color_space`] 的取值，决定每个像素的通道数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// 设备灰度，每个分量 1 个样本。
    DeviceGray,
    /// 设备 RGB，每个分量 3 个样本。
    DeviceRGB,
}

impl ColorSpace {
    fn as_name(&self) -> &'static str {
        match self {
            ColorSpace::DeviceGray => "DeviceGray",
            ColorSpace::DeviceRGB => "DeviceRGB",
        }
    }
}

/// 原始像素图片数据。
///
/// 流字节即未压缩的逐行像素样本，由 [`PdfBuilder::image`] 消费并
/// 翻译为 XObject Image 流。关键字段见 [`ImageData::width`]、
/// [`ImageData::color_space`]、[`ImageData::data`]。
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    /// 图像宽度（像素）。
    pub width: u32,
    /// 图像高度（像素）。
    pub height: u32,
    /// 色彩空间（DeviceGray / DeviceRGB）。
    pub color_space: ColorSpace,
    /// 每个颜色分量的位数（通常 8）。
    pub bits: u8,
    /// 原始像素字节，长度应为
    /// `width * height * color_space.bytes_per_pixel() * (bits/8)`。
    pub data: Vec<u8>,
}

/// 单页的构建态：尚未分配对象号的内容。
#[derive(Debug)]
struct PageBuild {
    size: PageSize,
    /// 累积的内容流操作符序列（类型化引擎）。`text`/`image` 调用追加于此。
    content: ContentStream,
    /// 该页用到的字体（按首次出现顺序）。
    fonts: Vec<Font>,
    /// 该页用到的图片（按添加顺序）。
    images: Vec<ImageData>,
}

/// PDF 逻辑层构建器。
///
/// 以「页 / 文字 / 图片」为核心抽象：通过 [`PdfBuilder::page`] 增页、
/// [`PdfBuilder::text`] 写文字、[`PdfBuilder::image`] 放图片，
/// 最终由 [`PdfBuilder::build`] 序列化为 PDF 字节流。
///
/// # 用法示例
///
/// ```ignore
/// use pdf::builder::{PdfBuilder, PageSize, Font};
///
/// let mut b = PdfBuilder::new();
/// let p0 = b.page(PageSize::A4);
/// b.text(p0, &Font::helvetica(24.0), 72.0, 700.0, "Hello");
/// let mut out = Vec::new();
/// b.build(&mut out).unwrap();
/// ```
#[derive(Debug)]
pub struct PdfBuilder {
    version: PdfVersion,
    pages: Vec<PageBuild>,
}

impl Default for PdfBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfBuilder {
    /// 新建构建器，默认版本取 [`PdfVersion::default`]（当前为 1.7）。
    pub fn new() -> Self {
        Self {
            version: PdfVersion::default(),
            pages: Vec::new(),
        }
    }

    /// 指定目标版本。
    pub fn with_version(v: PdfVersion) -> Self {
        Self {
            version: v,
            pages: Vec::new(),
        }
    }

    /// 新增一页，返回该页的 0-based 索引，供后续 `text`/`image` 使用。
    pub fn page(&mut self, size: PageSize) -> usize {
        let idx = self.pages.len();
        self.pages.push(PageBuild {
            size,
            content: ContentStream::new(),
            fonts: Vec::new(),
            images: Vec::new(),
        });
        idx
    }

    /// 在指定页上写一段文字。
    ///
    /// 文本先按字体的 [`FontEncoding`] 转成单字节字符码：可打印 ASCII
    /// 仍输出字面量 `BT /F<n> <size> Tf x y Td (text) Tj ET`（`(` `)` `\`
    /// 由 [`ContentStream`] 转义），其他字节输出十六进制字符串
    /// `BT ... <hex> Tj ET`。无法映射的字符写成 `?`；编码名无法识别时
    /// 退回原始 UTF-8 字节。坐标原点在页面左下角，单位 pt。
    pub fn text(&mut self, page: usize, font: &Font, x: f64, y: f64, s: &str) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        // 字体去重：同 BaseFont 名 + 同字号 + 同编码 视为同一字体对象。
        let fidx = match p.fonts.iter().position(|f| same_font(f, font)) {
            Some(i) => i,
            None => {
                p.fonts.push(font.clone());
                p.fonts.len() - 1
            }
        };

        // 委托内容流引擎：资源名 /F<fidx+1>（1-based）由 builder 分配，
        // 操作符组装（BT/ET/Tf/Td/Tj）和字面量转义由 ContentStream 负责。
        let font_name = format!("F{}", fidx + 1);
        match font.encoding.encode_text(&font.name, s) {
            Some(bytes) if bytes.iter().all(|&b| (0x20..=0x7E).contains(&b)) => {
                // 纯可打印 ASCII 保持字面量输出，便于阅读。
                let literal: String = bytes.iter().map(|&b| char::from(b)).collect();
                p.content.show_text_at(font_name, font.size, x, y, literal);
            }
            Some(bytes) => {
                // 单字节码用十六进制字符串写出，避免 UTF-8 误写。
                p.content
                    .begin_text()
                    .set_font(font_name, font.size)
                    .move_next_line(x, y)
                    .show_text_hex(bytes)
                    .end_text();
            }
            None => {
                // 编码名无法识别：维持原始 UTF-8 直写。
                p.content.show_text_at(font_name, font.size, x, y, s);
            }
        }
        self
    }

    /// 在指定页上放置一张图片。
    ///
    /// 生成内容操作符：`q w 0 0 h x y cm /Im<n> Do Q`，cm 矩阵为
    /// `[w 0 0 h x y]`，即把单位图片方块缩放到 `(w,h)` 并平移到
    /// `(x,y)`（左下角）。图片像素字节作为 XObject Image 流的原始数据。
    pub fn image(
        &mut self,
        page: usize,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        data: &ImageData,
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let iidx = p.images.len();
        p.images.push(data.clone());

        // 委托内容流引擎：cm 矩阵 [w 0 0 h x y] 把单位方块缩放到 (w,h)
        // 并平移到 (x,y)（左下角）。资源名 /Im<iidx+1> 由 builder 分配。
        p.content
            .save_state()
            .concat_matrix([w, 0.0, 0.0, h, x, y])
            .draw_xobject(format!("Im{}", iidx + 1))
            .restore_state();
        self
    }

    /// 序列化为 PDF 字节流。
    ///
    /// 内部按固定顺序分配对象号、构造底层 `Document`，再交给
    /// [`DocumentWriter`] 输出。xref/trailer 由 writer 自动生成。
    pub fn build<W: Write>(&mut self, w: &mut W) -> Result<(), WriteError> {
        let doc = self.assemble_document();
        DocumentWriter::new()
            .with_version(self.version)
            .build(&doc, w)
    }

    /// 把逻辑态翻译为底层 `Document`。
    fn assemble_document(&mut self) -> Document {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn render(b: &mut PdfBuilder) -> String {
        let mut out = Vec::new();
        b.build(&mut out).expect("build should succeed");
        // PDF 是二进制；用 lossy 转成字符串做子串断言。
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn builds_minimal_text_pdf() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &Font::helvetica(24.0), 72.0, 700.0, "Hello");
        let s = render(&mut b);

        assert!(
            s.contains("%PDF-1.7"),
            "应输出 1.7 版本头: {}",
            &s[..s.len().min(200)]
        );
        assert!(s.contains("/Type /Page"), "应含 Page 类型");
        assert!(s.contains("/Type /Catalog"), "应含 Catalog 类型");
        assert!(s.contains("/Root 1 0 R"), "trailer 应指向 Catalog");
        assert!(s.contains("BT"), "内容流应含 BT");
        assert!(s.contains("Tf"), "内容流应含 Tf");
        assert!(s.contains("Td"), "内容流应含 Td");
        assert!(s.contains("(Hello) Tj"), "内容流应含 (Hello) Tj");
        assert!(s.contains("ET"), "内容流应含 ET");
        // 字体对象
        assert!(s.contains("/Type /Font"));
        assert!(s.contains("/BaseFont /Helvetica"));
        assert!(s.contains("/Encoding /WinAnsiEncoding"));
    }

    #[test]
    fn text_escapes_parens_and_backslash() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &Font::helvetica(12.0), 10.0, 10.0, r"a(b)c\d");
        let s = render(&mut b);
        assert!(s.contains(r"(a\(b\)c\\d) Tj"), "应转义 ( ) \\");
    }

    #[test]
    fn image_xobject_emitted() {
        // 2x2 RGB 全红。
        let img = ImageData {
            width: 2,
            height: 2,
            color_space: ColorSpace::DeviceRGB,
            bits: 8,
            data: vec![255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0],
        };
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::Letter);
        b.image(p, 100.0, 100.0, 50.0, 50.0, &img);
        let s = render(&mut b);

        assert!(s.contains("/Type /XObject"));
        assert!(s.contains("/Subtype /Image"));
        assert!(s.contains("/Width 2"));
        assert!(s.contains("/Height 2"));
        assert!(s.contains("/ColorSpace /DeviceRGB"));
        assert!(s.contains("/BitsPerComponent 8"));
        // 内容流应含 Do
        assert!(s.contains("/Im1 Do"));
        assert!(s.contains("cm"));
    }

    #[test]
    fn multi_page_count() {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        let p1 = b.page(PageSize::A4);
        b.text(p0, &Font::helvetica(12.0), 10.0, 700.0, "First");
        b.text(p1, &Font::helvetica(12.0), 10.0, 700.0, "Second");
        let s = render(&mut b);

        assert!(s.contains("/Count 2"), "Pages Count 应为 2");
        // Kids 数组应含 2 个引用（对象 3 与 5）。
        assert!(s.contains("/Kids [3 0 R 5 0 R]"), "Kids 应引用两个 Page");
    }

    #[test]
    fn custom_page_size_mediabox() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::Custom(200.0, 300.0));
        b.text(p, &Font::helvetica(10.0), 5.0, 5.0, "x");
        let s = render(&mut b);
        assert!(s.contains("/MediaBox [0 0 200.0 300.0]"));
    }

    #[test]
    fn version_override_in_header() {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
        let p = b.page(PageSize::A4);
        b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "hi");
        let s = render(&mut b);
        assert!(s.contains("%PDF-1.4"));
    }

    #[test]
    fn named_encoding_is_emitted() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
        b.text(p, &font, 10.0, 10.0, "hi");
        let s = render(&mut b);
        assert!(s.contains("/Encoding /MacRomanEncoding"));
        assert!(!s.contains("WinAnsiEncoding"));
    }

    #[test]
    fn builtin_encoding_omits_encoding_entry() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::BuiltIn);
        b.text(p, &font, 10.0, 10.0, "hi");
        let s = render(&mut b);
        assert!(!s.contains("/Encoding"), "BuiltIn 不应写出 /Encoding");
    }

    #[test]
    fn custom_encoding_writes_base_and_sorted_differences() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::with_differences(
            Some("WinAnsiEncoding"),
            vec![
                (66, "adieresis".to_string()),
                (65, "eacute".to_string()),
                (66, "egrave".to_string()),
                (200, "Euro".to_string()),
            ],
        ));
        b.text(p, &font, 10.0, 10.0, "AB");
        let s = render(&mut b);
        assert!(s.contains("/BaseEncoding /WinAnsiEncoding"));
        // 升序输出，且重复的 66 以后定义者为准。
        assert!(
            s.contains("/Differences [65 /eacute 66 /egrave 200 /Euro]"),
            "Differences 应按字符码升序: {s}"
        );
    }

    #[test]
    fn custom_encoding_without_base_omits_base_encoding() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0)
            .with_encoding(FontEncoding::with_differences(None, vec![(65, "A".into())]));
        b.text(p, &font, 10.0, 10.0, "A");
        let s = render(&mut b);
        assert!(!s.contains("/BaseEncoding"));
        assert!(s.contains("/Differences [65 /A]"));
    }

    #[test]
    fn font_dedup_includes_encoding() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "a");
        let mac = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
        b.text(p, &mac, 10.0, 20.0, "b");
        b.text(p, &mac, 10.0, 30.0, "c");
        let s = render(&mut b);

        assert_eq!(
            s.matches("/Type /Font").count(),
            2,
            "不同编码应生成两个字体对象"
        );
        assert!(s.contains("/Encoding /WinAnsiEncoding"));
        assert!(s.contains("/Encoding /MacRomanEncoding"));
    }

    #[test]
    fn text_encodes_winansi_non_ascii_as_hex() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "Café €");
        let s = render(&mut b);
        assert!(
            s.contains("<436166E92080> Tj"),
            "WinAnsi 应按 CP1252 转码: {s}"
        );
    }

    #[test]
    fn text_encodes_macroman_codes() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
        b.text(p, &font, 10.0, 10.0, "é");
        let s = render(&mut b);
        assert!(s.contains("<8E> Tj"), "MacRoman 中 é 应为 0x8E: {s}");
    }

    #[test]
    fn text_replaces_unencodable_characters() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "a中b");
        let s = render(&mut b);
        // 替换后的 ? 仍是可打印 ASCII，因此走字面量输出。
        assert!(s.contains("(a?b) Tj"), "无法映射的字符应替换为 ?: {s}");
    }

    #[test]
    fn text_uses_differences_and_skips_overridden_base_codes() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::with_differences(
            Some("WinAnsiEncoding"),
            vec![(65, "eacute".to_string())],
        ));
        b.text(p, &font, 10.0, 10.0, "éA");
        let s = render(&mut b);
        // é 使用 /Differences 的 65；A 的原 65 被覆盖，退化为 ?。
        assert!(
            s.contains("(A?) Tj"),
            "Differences 应优先且屏蔽被覆盖的基码: {s}"
        );
    }

    #[test]
    fn text_builtin_encoding_uses_standard_default() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::BuiltIn);
        b.text(p, &font, 10.0, 10.0, "Aé");
        let s = render(&mut b);
        // A 在 StandardEncoding 为 65（可打印 ASCII），é 无映射。
        assert!(s.contains("(A?) Tj"), "BuiltIn 应按 Standard 编码转码: {s}");
    }

    #[test]
    fn text_with_unknown_encoding_name_falls_back_to_utf8() {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::named("UnknownEncoding"));
        b.text(p, &font, 10.0, 10.0, "é");
        let s = render(&mut b);
        assert!(s.contains("(é) Tj"), "未知编码名应保留 UTF-8 直写: {s}");
    }
}
