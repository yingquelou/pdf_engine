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

pub(crate) use types::GraphicsStateFunctions;
pub(crate) use types::OpenTypeFlavor;
pub(crate) use types::Pattern;
pub(crate) use types::mesh_component_ranges;
pub use types::{
    AxialShading, CalGray, CalRGB, CalculatorFunction, CidEncoding, CidFont, CidFontSubtype,
    CidSystemInfo, CidVerticalMetrics, CidWidth, ColorSpace, CoonsPatch, CoonsPatchShading,
    DashPattern, DeviceN, ExponentialFunction, Font, FontDescriptor, FontEncoding, FontProgram,
    FontWidths, FormXObject, FreeFormMeshShading, Function, FunctionShading, GraphicsState,
    GraphicsStateFont, GroupXObject, ICCBased, ImageData, ImageMask, Indexed, Lab,
    LatticeMeshShading, MeshFunction, MeshVertex, OptionalContentGroup, PageSize, PaintType,
    RadialShading, SampledFunction, Separation, Shading, ShadingOptions, ShadingPattern, SoftMask,
    SoftMaskSubtype, StitchingFunction, TensorProductPatch, TensorProductPatchShading,
    TilingPattern, TilingType, Type3Font, Type3Glyph,
};

use crate::core::content::{ContentStream, MarkedContentProperties};
use crate::core::reader::font::predefined_cmaps::resolve_predefined_cmap;
use crate::core::types::PdfVersion;
use crate::core::writer::{DocumentWriter, WriteError};
use std::io::Write;

/// 单页的构建态：尚未分配对象号的内容。
#[derive(Debug)]
struct PageBuild {
    size: PageSize,
    /// 累积的内容流操作符序列（类型化引擎）。`text`/`image` 调用追加于此。
    content: ContentStream,
    /// 该页用到的简单字体（按首次出现顺序）。
    fonts: Vec<Font>,
    /// 该页用到的 CIDFont（按首次出现顺序）。
    cid_fonts: Vec<CidFont>,
    /// 该页用到的图片（按添加顺序）。
    images: Vec<ImageData>,
    /// 该页用到的图形状态参数（按首次出现顺序，资源名 `/GS<n>`）。
    graphics_states: Vec<GraphicsState>,
    /// 该页绘制的 Form XObject 句柄（按首次绘制顺序，资源名 `/Fm<n>`）。
    forms: Vec<usize>,
    /// 该页使用的可选内容组句柄（按首次标记顺序，资源名 `/OC<n>`）。
    optional_content: Vec<usize>,
    /// 该页绘制的渐变句柄（按首次绘制顺序，资源名 `/Sh<n>`）。
    shadings: Vec<usize>,
    /// 该页填充的着色图案句柄（按首次使用顺序，资源名 `/P<n>`）。
    patterns: Vec<usize>,
    /// 该页使用的命名颜色空间句柄（按首次引用顺序，资源名 `/CS<n>`）。
    color_spaces: Vec<usize>,
    /// 页组属性字典（`/Group`）；提供时整页作为透明组合成。
    group: Option<GroupXObject>,
}

/// 把 16-bit 字符码（或 Identity 语义下的 CID）按 2 字节大端序
/// 追加到内容流字节缓冲。
fn push_two_byte_codes(bytes: &mut Vec<u8>, codes: impl IntoIterator<Item = u16>) {
    for code in codes {
        bytes.push((code >> 8) as u8);
        bytes.push((code & 0xFF) as u8);
    }
}

/// 字体去重键：BaseFont 名、字号位模式、编码配置、嵌入程序、子集标记、
/// 三种子集字形列表与 `/Widths` 都相同才算同一字体对象；Type 3 字体还比较
/// 字形定义。
fn same_font(a: &Font, b: &Font) -> bool {
    a.name == b.name
        && a.size.to_bits() == b.size.to_bits()
        && a.encoding == b.encoding
        && a.font_program == b.font_program
        && a.subset == b.subset
        && a.subset_glyphs == b.subset_glyphs
        && a.subset_type1_glyphs == b.subset_type1_glyphs
        && a.subset_cff_glyphs == b.subset_cff_glyphs
        && a.widths == b.widths
        && a.type3 == b.type3
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
    /// 文档级 Form XObject 池；`PdfBuilder::form_xobject` 返回其索引。
    forms: Vec<FormXObject>,
    /// 文档级可选内容组池；`PdfBuilder::optional_content_group` 返回其索引。
    optional_content_groups: Vec<OptionalContentGroup>,
    /// 文档级函数池；`PdfBuilder::function` 返回其索引。
    functions: Vec<Function>,
    /// 文档级渐变池；`PdfBuilder::shading` 返回其索引。
    shadings: Vec<Shading>,
    /// 文档级图案池；`PdfBuilder::shading_pattern` / `tiling_pattern`
    /// 返回其索引。
    patterns: Vec<Pattern>,
    /// 文档级颜色空间池；`PdfBuilder::color_space` 返回其索引。
    color_spaces: Vec<ColorSpace>,
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
            forms: Vec::new(),
            optional_content_groups: Vec::new(),
            functions: Vec::new(),
            shadings: Vec::new(),
            patterns: Vec::new(),
            color_spaces: Vec::new(),
        }
    }

    /// 指定目标版本。
    pub fn with_version(v: PdfVersion) -> Self {
        Self {
            version: v,
            pages: Vec::new(),
            forms: Vec::new(),
            optional_content_groups: Vec::new(),
            functions: Vec::new(),
            shadings: Vec::new(),
            patterns: Vec::new(),
            color_spaces: Vec::new(),
        }
    }

    /// 新增一页，返回该页的 0-based 索引，供后续 `text`/`image` 使用。
    pub fn page(&mut self, size: PageSize) -> usize {
        let idx = self.pages.len();
        self.pages.push(PageBuild {
            size,
            content: ContentStream::new(),
            fonts: Vec::new(),
            cid_fonts: Vec::new(),
            images: Vec::new(),
            graphics_states: Vec::new(),
            forms: Vec::new(),
            optional_content: Vec::new(),
            shadings: Vec::new(),
            patterns: Vec::new(),
            color_spaces: Vec::new(),
            group: None,
        });
        idx
    }

    /// 在指定页上写一段文字。
    ///
    /// 文本先按字体的 [`FontEncoding`] 转成单字节字符码：可打印 ASCII
    /// 仍输出字面量 `BT /F<n> <size> Tf x y Td (text) Tj ET`（`(` `)` `\`
    /// 由 [`ContentStream`] 转义），其他字节输出十六进制字符串
    /// `BT ... <hex> Tj ET`。无法映射的字符写成 `?`；编码名无法识别时
    /// 此调用退回原始 UTF-8 字节，但该字体在 [`PdfBuilder::build`] 阶段会
    /// 因未知 `/Encoding` 名报 [`WriteError::InvalidObject`]。
    /// 坐标原点在页面左下角，单位 pt。
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

    /// 注册一个可复用的 Form XObject，返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::draw_form`] 使用。表单内容流不附加
    /// `/Resources`，按规范使用绘制页的资源字典；内容只含路径等
    /// 无资源操作符时完全自包含。
    pub fn form_xobject(&mut self, form: &FormXObject) -> usize {
        let idx = self.forms.len();
        self.forms.push(form.clone());
        idx
    }

    /// 在指定页绘制已注册的 Form XObject。
    ///
    /// `matrix` 为绘制时的 CTM `[a b c d e f]`，生成
    /// `q <matrix> cm /Fm<n> Do Q`。同一表单在一页内只登记一次资源名，
    /// 多次绘制复用同一 `/Fm<n>` 与对象号；未知句柄或页索引被忽略。
    pub fn draw_form(&mut self, page: usize, form: usize, matrix: [f64; 6]) -> &mut Self {
        if form >= self.forms.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.forms.iter().position(|&f| f == form) {
            Some(i) => i,
            None => {
                p.forms.push(form);
                p.forms.len() - 1
            }
        };
        p.content
            .save_state()
            .concat_matrix(matrix)
            .draw_xobject(format!("Fm{}", idx + 1))
            .restore_state();
        self
    }

    /// 为指定页设置页组属性字典（页字典 `/Group`，PDF 1.4）。
    ///
    /// 页组把整页内容作为一个透明组合成，颜色空间、隔离（`/I`）与
    /// 挖空（`/K`）语义同 [`GroupXObject`]。参数在 [`PdfBuilder::build`]
    /// 阶段校验：非法取值返回 [`WriteError::InvalidObject`]；目标版本低于
    /// 1.4 时返回 [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_group(&mut self, page: usize, group: &GroupXObject) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.group = Some(group.clone());
        }
        self
    }

    /// 注册一个可选内容组（`/OCG`，PDF 1.5），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::begin_optional_content`] 使用。只有页面实际
    /// 标记过的组才写出 `/OCG` 对象与目录 `/OCProperties` 配置；组名称与
    /// `/Intent` 在 [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]。
    pub fn optional_content_group(&mut self, group: &OptionalContentGroup) -> usize {
        let idx = self.optional_content_groups.len();
        self.optional_content_groups.push(group.clone());
        idx
    }

    /// 注册一个 PDF 函数（§7.10 Type 0/2/3/4），返回文档级句柄。
    ///
    /// 句柄供 [`GraphicsState::with_transfer_function`] 等引用函数的配置
    /// 使用。只有被实际引用的函数才写出对象：Type 0/4 写成流对象，
    /// Type 2/3 写成直接字典；Type 3 的子函数在组装阶段递归生成。
    /// 函数参数在 [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]。
    pub fn function(&mut self, function: &Function) -> usize {
        let idx = self.functions.len();
        self.functions.push(function.clone());
        idx
    }

    /// 注册一个渐变（§8.7.4.2，Type 1/2/3），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::paint_shading`] 使用。只有页面实际绘制的渐变
    /// 才写出对象并登记到该页 `/Shading` 资源字典；参数在构建阶段校验，
    /// 非法值返回 [`WriteError::InvalidObject`]。
    pub fn shading(&mut self, shading: &Shading) -> usize {
        let idx = self.shadings.len();
        self.shadings.push(shading.clone());
        idx
    }

    /// 注册一个着色图案（PatternType 2，§8.7.2），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::fill_rect_with_pattern`] 使用。图案字典内联
    /// 引用其渐变；只有页面实际使用的图案才写出对象并登记到该页
    /// `/Pattern` 资源字典。
    pub fn shading_pattern(&mut self, pattern: &ShadingPattern) -> usize {
        let idx = self.patterns.len();
        self.patterns
            .push(Pattern::Shading(Box::new(pattern.clone())));
        idx
    }

    /// 注册一个平铺图案（PatternType 1，§8.7.3），返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::fill_rect_with_pattern`] 使用。当前仅支持
    /// 彩色图案（`/PaintType 1`）。图案以流对象写出：图块内容独立于
    /// 页面内容，`/Resources` 未提供时写空字典，因此图案内目前无法
    /// 引用构建器管理的字体或图片。只有页面实际使用的图案才写出对象
    /// 并登记到该页 `/Pattern` 资源字典。平铺图案自 PDF 1.2 引入，
    /// 低于该版本的目标版本在构建时返回
    /// [`WriteError::UnsupportedVersion`]。
    pub fn tiling_pattern(&mut self, pattern: &TilingPattern) -> usize {
        let idx = self.patterns.len();
        self.patterns
            .push(Pattern::Tiling(Box::new(pattern.clone())));
        idx
    }

    /// 在指定页用已注册的渐变填充当前裁剪区域（`/Sh<n> sh`）。
    ///
    /// 同一渐变在一页内只登记一次资源名，重复调用复用同一 `/Sh<n>`
    /// 与对象号；未知句柄或页索引被忽略。渐变属于 PDF 1.3 特性，低于
    /// 1.3 的目标版本在构建时返回 [`WriteError::UnsupportedVersion`]。
    pub fn paint_shading(&mut self, page: usize, shading: usize) -> &mut Self {
        if shading >= self.shadings.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.shadings.iter().position(|&h| h == shading) {
            Some(i) => i,
            None => {
                p.shadings.push(shading);
                p.shadings.len() - 1
            }
        };
        p.content.paint_shading(format!("Sh{}", idx + 1));
        self
    }

    /// 注册一个命名颜色空间，返回文档级句柄。
    ///
    /// 句柄供 [`PdfBuilder::set_stroke_color_space`] 与
    /// [`PdfBuilder::set_fill_color_space`] 使用。仅页面实际引用的颜色
    /// 空间会写入该页 `/Resources /ColorSpace`，并按首次引用顺序命名为
    /// `/CS1`、`/CS2`……；同一句柄跨页复用时，ICCBased 配置文件与
    /// tint 变换函数等流对象只写出一次。
    pub fn color_space(&mut self, color_space: &ColorSpace) -> usize {
        let idx = self.color_spaces.len();
        self.color_spaces.push(color_space.clone());
        idx
    }

    /// 在指定页把描边颜色空间切换为已注册的命名颜色空间（`/CS<n> CS`）。
    ///
    /// 未知句柄或页索引被忽略。
    pub fn set_stroke_color_space(&mut self, page: usize, color_space: usize) -> &mut Self {
        if let Some(name) = self.page_color_space_name(page, color_space) {
            self.pages[page].content.set_stroke_color_space(name);
        }
        self
    }

    /// 在指定页把填充颜色空间切换为已注册的命名颜色空间（`/CS<n> cs`）。
    ///
    /// 未知句柄或页索引被忽略。
    pub fn set_fill_color_space(&mut self, page: usize, color_space: usize) -> &mut Self {
        if let Some(name) = self.page_color_space_name(page, color_space) {
            self.pages[page].content.set_fill_color_space(name);
        }
        self
    }

    /// 在指定页以当前颜色空间的分量设置描边颜色（`c1 … cn SC`）。
    ///
    /// 分量个数由当前颜色空间决定；构建器不校验个数。
    pub fn set_stroke_color_components(&mut self, page: usize, components: &[f64]) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.set_stroke_color_components(components);
        }
        self
    }

    /// 在指定页以当前颜色空间的分量设置填充颜色（`c1 … cn sc`）。
    ///
    /// 分量个数由当前颜色空间决定；构建器不校验个数。
    pub fn set_fill_color_components(&mut self, page: usize, components: &[f64]) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.set_fill_color_components(components);
        }
        self
    }

    /// 返回该页的资源名（`/CS<n>`），首次引用时登记；未知页或句柄返回 `None`。
    fn page_color_space_name(&mut self, page: usize, color_space: usize) -> Option<String> {
        if color_space >= self.color_spaces.len() {
            return None;
        }
        let p = self.pages.get_mut(page)?;
        let idx = match p.color_spaces.iter().position(|&h| h == color_space) {
            Some(i) => i,
            None => {
                p.color_spaces.push(color_space);
                p.color_spaces.len() - 1
            }
        };
        Some(format!("CS{}", idx + 1))
    }

    /// 在指定页用图案填充矩形：`/Pattern cs /P<n> scn x y w h re f`。
    ///
    /// 图案句柄可来自 [`PdfBuilder::shading_pattern`] 或
    /// [`PdfBuilder::tiling_pattern`]。矩形的定位由图案矩阵与填充区域
    /// 共同决定；同一图案在一页内复用同一 `/P<n>` 资源名。未知句柄或页
    /// 索引被忽略。着色图案属于 PDF 1.3 特性，平铺图案属于 PDF 1.2
    /// 特性；目标版本不足时构建返回 [`WriteError::UnsupportedVersion`]。
    pub fn fill_rect_with_pattern(
        &mut self,
        page: usize,
        pattern: usize,
        rect: [f64; 4],
    ) -> &mut Self {
        if pattern >= self.patterns.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.patterns.iter().position(|&h| h == pattern) {
            Some(i) => i,
            None => {
                p.patterns.push(pattern);
                p.patterns.len() - 1
            }
        };
        p.content
            .set_fill_color_space("Pattern")
            .set_fill_pattern(format!("P{}", idx + 1))
            .rect(rect[0], rect[1], rect[2], rect[3])
            .fill();
        self
    }

    /// 在指定页用无色平铺图案填充矩形：`/Pattern cs c1 … cn /P<n> scn … re f`。
    ///
    /// 图案句柄必须来自 [`PdfBuilder::tiling_pattern`]，且图案须由
    /// [`TilingPattern::uncolored`] 创建。`components` 按图案字典的
    /// `/ColorSpace` 解释，数量必须与颜色空间分量数一致且有限；否则
    /// 该调用被忽略。资源名复用与版本门控同
    /// [`PdfBuilder::fill_rect_with_pattern`]。
    pub fn fill_rect_with_uncolored_pattern(
        &mut self,
        page: usize,
        pattern: usize,
        components: &[f64],
        rect: [f64; 4],
    ) -> &mut Self {
        let expected = match self.patterns.get(pattern) {
            Some(Pattern::Tiling(pattern)) if pattern.paint_type == PaintType::Uncolored => {
                pattern.color_space.as_ref().map(ColorSpace::channels)
            }
            _ => None,
        };
        let Some(expected) = expected else {
            return self;
        };
        if components.len() != expected || components.iter().any(|value| !value.is_finite()) {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.patterns.iter().position(|&h| h == pattern) {
            Some(i) => i,
            None => {
                p.patterns.push(pattern);
                p.patterns.len() - 1
            }
        };
        p.content
            .set_fill_color_space("Pattern")
            .set_fill_pattern_with_components(components.to_vec(), format!("P{}", idx + 1))
            .rect(rect[0], rect[1], rect[2], rect[3])
            .fill();
        self
    }

    /// 在指定页开始一段可选内容：`/OC /OC<n> BDC`。
    ///
    /// 组句柄必须是 [`PdfBuilder::optional_content_group`] 的返回值；同页
    /// 首次使用分配 `/OC<n>` 资源名，后续复用。必须以
    /// [`PdfBuilder::end_optional_content`] 结束。未知组句柄或页索引被忽略。
    /// 目标版本低于 1.5 时构建返回 [`WriteError::UnsupportedVersion`]。
    pub fn begin_optional_content(&mut self, page: usize, group: usize) -> &mut Self {
        if group >= self.optional_content_groups.len() {
            return self;
        }
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let idx = match p.optional_content.iter().position(|&g| g == group) {
            Some(i) => i,
            None => {
                p.optional_content.push(group);
                p.optional_content.len() - 1
            }
        };
        p.content.begin_marked_content_properties(
            "OC",
            MarkedContentProperties::Name(format!("OC{}", idx + 1)),
        );
        self
    }

    /// 结束当前可选内容段（`EMC`）。未知页索引被忽略。
    pub fn end_optional_content(&mut self, page: usize) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.content.end_marked_content();
        }
        self
    }

    /// 在指定页应用一组图形状态参数。
    ///
    /// 在页面资源字典中登记 `/ExtGState` 条目（同页相同配置去重为同一
    /// `/GS<n>`），并在当前位置追加 `/GS<n> gs`。`/Font` 条目引用的字体
    /// 会一并登记到该页字体资源，构建阶段解析为字体字典对象引用。
    ///
    /// 参数在 [`PdfBuilder::build`] 阶段校验：非法取值返回
    /// [`WriteError::InvalidObject`]；包含透明度参数且目标版本低于 1.4
    /// 时返回 [`WriteError::UnsupportedVersion`]。
    pub fn set_graphics_state(&mut self, page: usize, state: &GraphicsState) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        if let Some(gs_font) = &state.font
            && !p.fonts.iter().any(|font| same_font(font, &gs_font.font))
        {
            p.fonts.push(gs_font.font.clone());
        }
        let idx = match p.graphics_states.iter().position(|s| s == state) {
            Some(i) => i,
            None => {
                p.graphics_states.push(state.clone());
                p.graphics_states.len() - 1
            }
        };
        p.content.set_ext_g_state(format!("GS{}", idx + 1));
        self
    }

    /// 在指定页上注册一个 CIDFont 并返回该页内的字体索引（0-based）。
    ///
    /// 索引供 [`PdfBuilder::text_cid`] 与 [`PdfBuilder::text_cid_unicode`]
    /// 使用。同一 CIDFont（整体结构相等判定）在同一页内去重。
    pub fn cid_font(&mut self, page: usize, cid_font: &CidFont) -> usize {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return 0,
        };
        match p.cid_fonts.iter().position(|f| f == cid_font) {
            Some(i) => i,
            None => {
                p.cid_fonts.push(cid_font.clone());
                p.cid_fonts.len() - 1
            }
        }
    }

    /// 在指定页上用 CIDFont 写一段文字（Identity 语义）。
    ///
    /// `cids` 为 CID 序列（16-bit），直接按 `/Identity-H` 语义（字符码
    /// 即 CID）写成 2 字节大端序十六进制字符串。若字体配置了自定义
    /// [`CidEncoding`]，请改用 [`PdfBuilder::text_codes`] 传入源字符码，
    /// 否则内容流中的字节会被嵌入 CMap 当作字符码重新映射。
    /// 坐标原点在页面左下角，单位 pt。
    pub fn text_cid(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        cids: &[u16],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get(cid_font_idx) {
            Some(f) => f.size,
            None => return self,
        };
        // 简单字体用 /F1..，CIDFont 用 /CF1.. 避免命名冲突。
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(cids.len() * 2);
        push_two_byte_codes(&mut bytes, cids.iter().copied());
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用 CIDFont 写一段文字，并登记 CID → Unicode 映射
    /// （Identity 语义）。
    ///
    /// `glyphs` 为 `(CID, Unicode 文本)` 序列：CID 按 Identity-H 写成
    /// 2 字节大端序十六进制串，同时写入该 CIDFont 的 `/ToUnicode` CMap
    /// （`bfchar`/`bfrange` 条目，目标为 UTF-16BE）。同一 CID 的映射以
    /// 最后一次登记为准；CIDFont 未注册时忽略本次调用。配置自定义
    /// [`CidEncoding`] 时应改用 [`PdfBuilder::text_codes_unicode`]。
    pub fn text_cid_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        glyphs: &[(u16, &str)],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => {
                for &(cid, text) in glyphs {
                    f.to_unicode.insert(cid, text.to_owned());
                }
                f.size
            }
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(glyphs.len() * 2);
        push_two_byte_codes(&mut bytes, glyphs.iter().map(|&(cid, _)| cid));
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用 CIDFont 按 2 字节字符码写一段文字。
    ///
    /// `codes` 是内容流中的**源字符码**，逐个按 2 字节大端序写成十六进制
    /// 字符串。CIDFont 配置了 [`CidEncoding`] 时由嵌入 CMap 把字符码映射
    /// 为 CID；未配置时等价于 `/Identity-H`（字符码即 CID）。字符码可以不
    /// 在编码映射中，此时读端按 CMap 语义得到 CID 0。
    pub fn text_codes(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        codes: &[u16],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get(cid_font_idx) {
            Some(f) => f.size,
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(codes.len() * 2);
        push_two_byte_codes(&mut bytes, codes.iter().copied());
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用 CIDFont 按 2 字节字符码写文字，并登记
    /// 字符码 → Unicode 映射。
    ///
    /// `glyphs` 为 `(字符码, Unicode 文本)` 序列：字符码按 2 字节大端序
    /// 写入内容流，同时以**字符码**为键登记到该 CIDFont 的 `/ToUnicode`
    /// CMap（读端按源字符码查询，即使嵌入 CMap 把它映射到不同 CID）。
    /// 同一字符码的映射以最后一次登记为准；CIDFont 未注册时忽略本次调用。
    pub fn text_codes_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        glyphs: &[(u16, &str)],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => {
                for &(code, text) in glyphs {
                    f.to_unicode.insert(code, text.to_owned());
                }
                f.size
            }
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::with_capacity(glyphs.len() * 2);
        push_two_byte_codes(&mut bytes, glyphs.iter().map(|&(code, _)| code));
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用 CIDFont 按显式字节字符码写一段文字。
    ///
    /// `codes` 中每一项是一个完整的源字符码（大端字节序列），长度须为
    /// 1..=4，与 [`CidEncoding::mapping_bytes`] 登记的码长对应；长度非法
    /// 的条目被跳过（本 API 无法逐项报错）。CIDFont 未配置编码时等价于
    /// `/Identity-H`（2 字节码即 CID）。字符码可不在编码映射中，此时读端
    /// 按 CMap 语义得到 CID 0。
    ///
    /// [`CidEncoding::mapping_bytes`]: crate::core::builder::CidEncoding::mapping_bytes
    pub fn text_code_bytes(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        codes: &[&[u8]],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get(cid_font_idx) {
            Some(f) => f.size,
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::new();
        for code in codes {
            if (1..=4).contains(&code.len()) {
                bytes.extend_from_slice(code);
            }
        }
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上按显式字节字符码写文字，并登记字符码 → Unicode 映射。
    ///
    /// `glyphs` 为 `(字符码字节序列, Unicode 文本)` 序列：字符码原样写入
    /// 内容流，同时以**完整字节序列**为键登记到该 CIDFont 的
    /// `/ToUnicode` CMap（键长度与字符码一致）。长度非法的条目被跳过；
    /// 同一字符码以最后一次登记为准。
    pub fn text_code_bytes_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        glyphs: &[(&[u8], &str)],
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let size = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => {
                for &(code, text) in glyphs {
                    if (1..=4).contains(&code.len()) {
                        f.to_unicode_bytes.insert(code.to_vec(), text.to_owned());
                    }
                }
                f.size
            }
            None => return self,
        };
        let font_name = format!("CF{}", cid_font_idx + 1);
        let mut bytes = Vec::new();
        for &(code, _) in glyphs {
            if (1..=4).contains(&code.len()) {
                bytes.extend_from_slice(code);
            }
        }
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 在指定页上用配置了预定义 CMap 的 CIDFont 写一段 Unicode 文本。
    ///
    /// 每个字符取其 Unicode 码位作为字符码，按 2 字节大端序写入内容流，
    /// 并以字符码为键登记到该 CIDFont 的 `/ToUnicode` CMap，保持
    /// 「写入→解析→提取」可还原。字符码必须是目标 CMap **2 字节码空间**
    /// 内的值（如 `UniGB-UCS2-H` 覆盖全部 BMP；`Adobe-GB1` 等混合码空间
    /// CMap 只接受其 2 字节区段内的码位）。
    ///
    /// 取舍说明（本 API 返回 `&mut Self`，无法逐字报错，参照现有
    /// 「无法表示」约定跳过而非替换）：
    /// - 补充平面字符（码位 > U+FFFF）无法用 2 字节码表示，跳过；
    /// - 码位不在目标 CMap 2 字节码空间内时，写出后读端会把字节拆成
    ///   较短码或拒绝解码，ToUnicode 无法对应，故同样跳过；
    ///   被跳过的字符既不写内容流也不登记 ToUnicode。
    ///
    /// CIDFont 未注册、未配置 [`CidFont::predefined_encoding`] 或名称不在
    /// 读端名册内时忽略本次调用（未知名称仍会在 [`PdfBuilder::build`]
    /// 阶段报错）。
    pub fn text_predefined_unicode(
        &mut self,
        page: usize,
        cid_font_idx: usize,
        x: f64,
        y: f64,
        text: &str,
    ) -> &mut Self {
        let p = match self.pages.get_mut(page) {
            Some(p) => p,
            None => return self,
        };
        let font = match p.cid_fonts.get_mut(cid_font_idx) {
            Some(f) => f,
            None => return self,
        };
        let Some(name) = font.predefined_encoding.as_deref() else {
            return self;
        };
        // 名称合法性由 build 统一校验；此处解析失败直接忽略本次文本。
        let Ok(cmap) = resolve_predefined_cmap(name) else {
            return self;
        };
        let size = font.size;
        let mut bytes = Vec::with_capacity(text.len() * 2);
        for ch in text.chars() {
            let code = u32::from(ch);
            // 补充平面无法表示为 2 字节码，跳过。
            if code > u32::from(u16::MAX) {
                continue;
            }
            // 仅接受落在 2 字节码空间内的码位，保证读端按 2 字节解码。
            let in_codespace = cmap
                .codespace_ranges
                .iter()
                .any(|&(low, high, length)| length == 2 && (low..=high).contains(&code));
            if !in_codespace {
                continue;
            }
            let code = code as u16;
            bytes.push((code >> 8) as u8);
            bytes.push((code & 0xFF) as u8);
            font.to_unicode.insert(code, ch.to_string());
        }
        let font_name = format!("CF{}", cid_font_idx + 1);
        p.content
            .begin_text()
            .set_font(font_name, size)
            .move_next_line(x, y)
            .show_text_hex(bytes)
            .end_text();
        self
    }

    /// 序列化为 PDF 字节流。
    ///
    /// 内部按固定顺序分配对象号、构造底层 `Document`，再交给
    /// [`DocumentWriter`] 输出。xref/trailer 由 writer 自动生成。
    pub fn build<W: Write>(&mut self, w: &mut W) -> Result<(), WriteError> {
        let doc = self.assemble_document()?;
        DocumentWriter::new()
            .with_version(self.version)
            .build(&doc, w)
    }
}
