//! PDF 逻辑抽象层：以「页 / 文字 / 图片」等逻辑概念组织内容，
//! 内部翻译为底层 [`crate::core::types`] 对象，再由
//! [`crate::core::writer::DocumentWriter`] 序列化。
//!
//! 这是构建 PDF 的第二层 API：调用方不需要直接拼 `Dict`/`Object`，
//! 只需声明「在第几页什么位置写什么字、放什么图」，由 [`PdfBuilder`]
//! 负责对象号分配、资源字典组装、内容流操作符生成。
//!
//! 目录模块：
//! - `types`：页面尺寸、字体与编码配置、图片数据；
//! - `assemble`：逻辑态 → `Document` 的对象号分配与字典组装。
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

mod assemble;
#[cfg(test)]
mod tests;
mod types;

pub use types::{ColorSpace, Font, FontEncoding, ImageData, PageSize};

use crate::core::content::ContentStream;
use crate::core::types::PdfVersion;
use crate::core::writer::{DocumentWriter, WriteError};
use std::io::Write;

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

/// 字体去重键：BaseFont 名、字号位模式与编码配置都相同才算同一字体对象。
fn same_font(a: &Font, b: &Font) -> bool {
    a.name == b.name && a.size.to_bits() == b.size.to_bits() && a.encoding == b.encoding
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
/// use pdf_engine::core::builder::{PdfBuilder, PageSize, Font};
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
}
