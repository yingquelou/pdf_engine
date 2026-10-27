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
mod navigation;
mod patterns;
#[cfg(test)]
mod tests;
mod text_codes;
mod types;

pub use navigation::OutlineEntry;

pub(crate) use types::GraphicsStateFunctions;
pub(crate) use types::OpenTypeFlavor;
pub(crate) use types::Pattern;
pub(crate) use types::mesh_component_ranges;
pub use types::{
    AxialShading, CalGray, CalRGB, CalculatorFunction, CidEncoding, CidFont, CidFontSubtype,
    CidSystemInfo, CidVerticalMetrics, CidWidth, ColorSpace, CoonsPatch, CoonsPatchShading,
    DashPattern, DeviceN, ExponentialFunction, Font, FontDescriptor, FontEncoding, FontProgram,
    FontWidths, FormResources, FormXObject, FreeFormMeshShading, Function, FunctionShading,
    GraphicsState, GraphicsStateFont, GroupXObject, Halftone, HalftoneDictionary, ICCBased,
    ImageData, ImageIntent, ImageMask, Indexed, Lab, LatticeMeshShading, MeshFunction, MeshVertex,
    OptionalContentGroup, PageSize, PaintType, RadialShading, SampledFunction, Separation, Shading,
    ShadingOptions, ShadingPattern, SoftMask, SoftMaskSubtype, StitchingFunction,
    TensorProductPatch, TensorProductPatchShading, TilingPattern, TilingType, Type1Halftone,
    Type2Halftone, Type3Font, Type3Glyph,
};

use crate::core::content::{ContentStream, MarkedContentProperties};
use crate::core::types::PdfVersion;
use crate::core::writer::{DocumentWriter, StreamCompression, WriteError};
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
    /// `text_code_bytes*` 首次遇到的非法字符码长度；`build` 时据此报错。
    invalid_code_length: Option<usize>,
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
    /// D4 写端流压缩策略；`None`（默认）时所有流保持明文写出。
    compression: Option<StreamCompression>,
    /// 文档级导航结构（页面标签、命名目标/页面、大纲）。
    navigation: navigation::NavigationState,
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
            compression: None,
            navigation: navigation::NavigationState::default(),
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
            compression: None,
            navigation: navigation::NavigationState::default(),
        }
    }

    /// 启用写端流压缩（D4）：`build` 时所有未声明 `/Filter` 的明文流
    /// （页面内容流、图像、字体程序与 writer 生成的结构流）按
    /// [`StreamCompression`] 策略以 `FlateDecode` 压缩写出；已声明过滤器的
    /// 流（如 `DCTDecode`/`JPXDecode` 透传）保持原样。不调用本方法则输出
    /// 与既往一致为明文。`FlateDecode` 要求目标版本 ≥1.2，图像 PNG 预测器
    /// 要求 ≥1.3，不满足时 [`build`](Self::build) 报
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节；后端失败报
    /// [`WriteError::Compression`]，同样零字节、不静默回退明文。
    pub fn compress_streams(&mut self, compression: StreamCompression) -> &mut Self {
        self.compression = Some(compression);
        self
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
            invalid_code_length: None,
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

    /// 序列化为 PDF 字节流。
    ///
    /// 内部按固定顺序分配对象号、构造底层 `Document`，再交给
    /// [`DocumentWriter`] 输出。xref/trailer 由 writer 自动生成。
    pub fn build<W: Write>(&mut self, w: &mut W) -> Result<(), WriteError> {
        if let Some(length) = self.pages.iter().find_map(|page| page.invalid_code_length) {
            return Err(WriteError::InvalidObject(format!(
                "invalid CID character code length {length} in text_code_bytes (expected 1 to 4 bytes)"
            )));
        }
        let mut doc = self.assemble_document()?;
        navigation::apply(&mut doc, self.version, self.pages.len(), &self.navigation)?;
        let mut writer = DocumentWriter::new().with_version(self.version);
        if let Some(compression) = self.compression.clone() {
            writer = writer.with_compression(compression);
        }
        writer.build(&doc, w)
    }
}
