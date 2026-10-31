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
mod metadata;
mod navigation;
mod patterns;
#[cfg(test)]
mod tests;
mod text_codes;
mod types;

pub use metadata::{DocumentInfo, FileIdentifier};
pub use navigation::{
    ArticleBead, ArticleThread, Collection, CollectionField, CollectionFieldType, CollectionSort,
    CollectionView, OutlineEntry, Slideshow,
};

pub(crate) use types::GraphicsStateFunctions;
pub(crate) use types::OpenTypeFlavor;
pub(crate) use types::Pattern;
pub(crate) use types::SeparationGroup;
pub(crate) use types::mesh_component_ranges;
pub use types::{
    AcroForm, Action, ActionFileSpec, ActionNamedAction, ActionRemoteDestination,
    AnnotationBorderStyle, AnnotationDestination, AnnotationKind, AppearanceCharacteristics,
    AxialShading, BorderArray, BorderEffect, BorderEffectStyle, CalGray, CalRGB,
    CalculatorFunction, CalloutLine, CaptionPosition, CidEncoding, CidFont, CidFontSubtype,
    CidSystemInfo, CidVerticalMetrics, CidWidth, CollectionItem, CollectionItemValue,
    CollectionSubitem, CollectionSubitemValue, ColorSpace, CoonsPatch, CoonsPatchShading,
    DashPattern, DeviceColorant, DeviceN, EmbeddedFile, ExponentialFunction, FieldActionEvent,
    FieldValue, FixedPrint, Font, FontDescriptor, FontEncoding, FontProgram, FontWidths, FormField,
    FormFieldType, FormOption, FormResources, FormXObject, FreeFormMeshShading, FreeTextIntent,
    Function, FunctionShading, GoTo3DViewAction, GoTo3DViewSelection, GoToEAction, GoToETarget,
    GoToETargetAnnotation, GoToETargetPage, GoToETargetRelation, GraphicsState, GraphicsStateFont,
    GroupXObject, Halftone, HalftoneDictionary, HideAction, HideTarget, ICCBased, IconFit,
    IconScaleWhen, IconScaling, ImageData, ImageIntent, ImageMask, Indexed, Lab,
    LatticeMeshShading, LineEndStyle, LineIntent, LinkHighlightMode, Markup3D, Measure, MediaClip,
    MediaClipData, MediaClipSection, MediaClipSource, MediaDuration, MediaFit, MediaFloatingWindow,
    MediaPermissions, MediaPlayParameters, MediaPlaySettings, MediaPlayerInfo, MediaPlayers,
    MediaRendition, MediaScreenParameters, MediaScreenSettings, MediaTimespan, MediaWindow,
    MeshFunction, MeshVertex, Movie, MovieAction, MovieActionTarget, MovieActivation,
    MoviePlayback, NumberFormat, NumberFraction, NumberLabelOrder, OcgStateOperation,
    OcgStateSequence, Opi13Dictionary, Opi20Dictionary, OpiColor, OpiColorType, OpiInks, OpiProxy,
    OpiTagText, OpiVersion, OptionalContentGroup, OutputIntent, OutputIntentProfile,
    PageBoundaries, PageSize, PaintType, PdfAnnotation, PolygonIntent, RadialShading, Rendition,
    RenditionAction, RenditionOperation, SampledFunction, SelectorRendition, Separation,
    SeparationInfo, SetOcgStateAction, Shading, ShadingOptions, ShadingPattern, SoftMask,
    SoftMaskSubtype, SoftwareIdentifier, Sound, SoundAction, SoundEncoding, StitchingFunction,
    TemporaryFilePermission, TensorProductPatch, TensorProductPatchShading, ThreadAction,
    ThreeDActivation, ThreeDActivationPolicy, ThreeDAnimationStyle, ThreeDAnimationStyleType,
    ThreeDArtwork, ThreeDArtworkState, ThreeDBackground, ThreeDBackgroundType, ThreeDClipping,
    ThreeDColor, ThreeDCrossSection, ThreeDDeactivationPolicy, ThreeDFaceColor,
    ThreeDLightingScheme, ThreeDLightingSchemeType, ThreeDMatrixSource, ThreeDNode,
    ThreeDProjection, ThreeDProjectionBinding, ThreeDProjectionScale, ThreeDProjectionType,
    ThreeDRenderMode, ThreeDRenderModeType, ThreeDStreamFormat, ThreeDView, ThreeDViewSelection,
    TilingPattern, TilingType, Transition, TransitionDimension, TransitionDirection,
    TransitionMotion, TransitionStyle, Type1Halftone, Type2Halftone, Type3Font, Type3Glyph,
    Viewport, XmpListItem, XmpListKind, XmpPacket, XmpPacketError, XmpValue,
};

use crate::core::content::{ContentStream, MarkedContentProperties};
use crate::core::types::PdfVersion;
use crate::core::writer::{DocumentWriter, StreamCompression, WriteError};
use std::cell::Cell;
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
    /// 页面视口字典（`/VP` 数组，12.9.2）；按绘制顺序写出。
    viewports: Vec<Viewport>,
    /// 页面过渡效果（页面 `/Trans`，12.4.4 Table 30/162）。
    transition: Option<Transition>,
    /// 页面停留时长（页面 `/Dur`，12.4.4 Table 30，单位秒）。
    duration: Option<f64>,
    /// 页面缩略图（页面 `/Thumb`，12.3.4 Table 30）。
    thumbnail: Option<ImageData>,
    /// 页面边界（14.11.2 Table 30）：`/MediaBox` 及可选 Crop/Bleed/Trim/Art
    /// 框与 `/BoxColorInfo`；`None` 时按 `size` 写 MediaBox。
    boundaries: Option<PageBoundaries>,
    /// `text_code_bytes*` 首次遇到的非法字符码长度；`build` 时据此报错。
    invalid_code_length: Option<usize>,
}

/// 注解的构建态：绑定的页号与尚未分配对象号的注解字典。
#[derive(Debug)]
struct AnnotationEntry {
    page: usize,
    annot: PdfAnnotation,
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
    /// 目录 `/OutputIntents` 输出意图数组（14.11.5 Table 365）。
    output_intents: Vec<OutputIntent>,
    /// 页面 `/SeparationInfo` 分色页组（14.11.4 Table 364）。
    separation_groups: Vec<SeparationGroup>,
    /// 文档级函数池；`PdfBuilder::function` 返回其索引。
    functions: Vec<Function>,
    /// 文档级渐变池；`PdfBuilder::shading` 返回其索引。
    shadings: Vec<Shading>,
    /// 文档级图案池；`PdfBuilder::shading_pattern` / `tiling_pattern`
    /// 返回其索引。
    patterns: Vec<Pattern>,
    /// 文档级颜色空间池；`PdfBuilder::color_space` 返回其索引。
    color_spaces: Vec<ColorSpace>,
    /// 文档级注解池；`PdfBuilder::add_annotation` 返回其索引。
    annotations: Vec<AnnotationEntry>,
    /// 最近一次 assemble 分配的注解对象号（下标即注解句柄）；供大纲
    /// `/A` 解析 Hide 动作的注解目标。
    annotation_nums: Vec<i64>,
    /// 最近一次 assemble 分配的可选内容组对象号（下标即组句柄）；供大纲
    /// `/A` 解析 Set-OCG-State 动作的组目标。
    ocg_nums: Vec<i64>,
    /// 文档级声音对象池（13.3）；`PdfBuilder::add_sound` 返回其索引。
    sounds: Vec<Sound>,
    /// 最近一次 assemble 分配的声音流对象号（下标即声音句柄）；供 Sound
    /// 动作 `/Sound` 与 Sound 注解 `/Sound` 解析。
    sound_nums: Vec<i64>,
    /// 文档级 3D artwork 池（13.6）；`PdfBuilder::add_three_d_artwork`
    /// 返回其索引。
    three_d_artworks: Vec<ThreeDArtwork>,
    /// 最近一次 assemble 分配的 Form XObject 对象号（下标即表单句柄）；
    /// 供大纲 `/A` 中 Rendition 动作的 media clip `/D` 解析。
    form_nums: Vec<i64>,
    /// 最近一次 assemble 分配的嵌入文件规格对象号（下标即嵌入文件句柄）；
    /// 供 Rendition 动作的 media clip `/D` 解析。
    embedded_nums: Vec<i64>,
    /// 最近一次 assemble 分配的内联 media clip section 首级 `/D` 目标号
    /// （按动作写出顺序）；供注解与大纲 `/C` 内联 section 消费。
    rendition_nums: Vec<i64>,
    /// 最近一次 assemble 分配的 section 链全部 `/D` 目标号（按分配顺序）；
    /// 供 `assemble::rendition::write_objects` 写出。
    rendition_targets: Vec<i64>,
    /// 内联 section 的消费游标；一次构建内注解与大纲共享。
    rendition_cursor: Cell<usize>,
    /// 文档级嵌入文件池；`PdfBuilder::embed_file` 返回其索引。
    embedded: Vec<EmbeddedFile>,
    /// 交互表单（AcroForm）：表单级条目与按页绑定的字段树。
    acro_form: Option<AcroForm>,
    /// D4 写端流压缩策略；`None`（默认）时所有流保持明文写出。
    compression: Option<StreamCompression>,
    /// 文档级导航结构（页面标签、命名目标/页面、大纲）。
    navigation: navigation::NavigationState,
    /// trailer `/Info` 文档信息字典（14.3.3）；空时不写出。
    document_info: DocumentInfo,
    /// trailer `/ID` 文件标识符（14.4）；`None`（默认）时不写出。
    file_identifier: Option<FileIdentifier>,
    /// XMP 元数据流原文（14.3.2）；`None`（默认）时不写出。
    xmp_metadata: Option<Vec<u8>>,
    /// 类型化 XMP 包（14.3.2）；与 `xmp_metadata` 互斥。
    xmp_packet: Option<XmpPacket>,
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
            output_intents: Vec::new(),
            separation_groups: Vec::new(),
            functions: Vec::new(),
            shadings: Vec::new(),
            patterns: Vec::new(),
            color_spaces: Vec::new(),
            annotations: Vec::new(),
            annotation_nums: Vec::new(),
            ocg_nums: Vec::new(),
            sounds: Vec::new(),
            sound_nums: Vec::new(),
            three_d_artworks: Vec::new(),
            form_nums: Vec::new(),
            embedded_nums: Vec::new(),
            rendition_nums: Vec::new(),
            rendition_targets: Vec::new(),
            rendition_cursor: Cell::new(0),
            embedded: Vec::new(),
            acro_form: None,
            compression: None,
            navigation: navigation::NavigationState::default(),
            document_info: DocumentInfo::default(),
            file_identifier: None,
            xmp_metadata: None,
            xmp_packet: None,
        }
    }

    /// 指定目标版本。
    pub fn with_version(v: PdfVersion) -> Self {
        Self {
            version: v,
            pages: Vec::new(),
            forms: Vec::new(),
            optional_content_groups: Vec::new(),
            output_intents: Vec::new(),
            separation_groups: Vec::new(),
            functions: Vec::new(),
            shadings: Vec::new(),
            patterns: Vec::new(),
            color_spaces: Vec::new(),
            annotations: Vec::new(),
            annotation_nums: Vec::new(),
            ocg_nums: Vec::new(),
            sounds: Vec::new(),
            sound_nums: Vec::new(),
            three_d_artworks: Vec::new(),
            form_nums: Vec::new(),
            embedded_nums: Vec::new(),
            rendition_nums: Vec::new(),
            rendition_targets: Vec::new(),
            rendition_cursor: Cell::new(0),
            embedded: Vec::new(),
            acro_form: None,
            compression: None,
            navigation: navigation::NavigationState::default(),
            document_info: DocumentInfo::default(),
            file_identifier: None,
            xmp_metadata: None,
            xmp_packet: None,
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

    /// 设置 trailer `/Info` 文档信息字典（14.3.3）；`build` 时分配独立
    /// 对象写出，trailer 以 `/Info` 间接引用挂接。空字典（全部字段未
    /// 设置）不写出，trailer 与既往一致。
    pub fn set_document_info(&mut self, info: DocumentInfo) -> &mut Self {
        self.document_info = info;
        self
    }

    /// 设置 trailer `/ID` 文件标识符（14.4）。不调用则不写 `/ID`。
    /// 增量更新场景（E7）经 [`FileIdentifier::generate`] 生成后复用
    /// 永久 ID、更新 `update` 字段。
    pub fn set_file_identifier(&mut self, id: FileIdentifier) -> &mut Self {
        self.file_identifier = Some(id);
        self
    }

    /// 设置 XMP 元数据包（14.3.2）：`build` 时写成独立
    /// `/Type /Metadata /Subtype /XML` 流对象并在目录挂 `/Metadata`
    /// 引用。调用方负责 XML良构性；本层原样透传字节。要求目标版本
    /// ≥1.4，否则 [`build`](Self::build) 返回
    /// [`WriteError::UnsupportedVersion`] 且零字节输出。
    ///
    /// 与类型化入口 [`set_xmp_packet`](Self::set_xmp_packet) 互斥：
    /// 两者同时设置时 `build` 返回 [`WriteError::InvalidObject`]，
    /// 不静默择一。
    pub fn set_xmp_metadata(&mut self, xml: impl Into<Vec<u8>>) -> &mut Self {
        self.xmp_metadata = Some(xml.into());
        self
    }

    /// 设置类型化 XMP 包（14.3.2）：`build` 时由
    /// [`XmpPacket::to_xml`] 序列化为 RDF/XML，写成独立
    /// `/Type /Metadata /Subtype /XML` 流对象并在目录挂 `/Metadata`。
    ///
    /// 序列化失败（非法 Clark 键、XML 1.0 非法字符、非法
    /// `rdf:datatype` URI 等）返回 [`WriteError::InvalidObject`] 且零
    /// 字节输出。要求目标版本 ≥1.4，否则返回
    /// [`WriteError::UnsupportedVersion`] 且零字节输出。
    ///
    /// 与原文透传入口 [`set_xmp_metadata`](Self::set_xmp_metadata)
    /// 互斥：两者同时设置时 `build` 返回
    /// [`WriteError::InvalidObject`]，不静默择一。
    pub fn set_xmp_packet(&mut self, packet: XmpPacket) -> &mut Self {
        self.xmp_packet = Some(packet);
        self
    }

    /// 追加一个输出意图（14.11.5 Table 365），按注册顺序写入目录
    /// `/OutputIntents` 数组（PDF 1.4）。
    ///
    /// `/DestOutputProfile` 写成独立 ICC 配置文件流，`/N` 须为 1–4；
    /// `/OutputConditionIdentifier` 须非空；标识为 `Custom` 时须同时声明
    /// `/Info` 与 `/DestOutputProfile`（Table 365 的条件性要求）。违反者
    /// [`build`](Self::build) 返回 [`WriteError::InvalidObject`]；目标版本
    /// 低于 1.4 返回 [`WriteError::UnsupportedVersion`]，均零字节输出。
    pub fn add_output_intent(&mut self, intent: OutputIntent) -> &mut Self {
        self.output_intents.push(intent);
        self
    }

    /// 为一组分色页设置共享的分色字典（14.11.4 Table 364）。
    ///
    /// `pages` 为 0 基页号；同组各页写出完全相同的 `/SeparationInfo`
    /// （相同 `/Pages` 数组），`/DeviceColorant` 为名字或文本串色料，
    /// 可选 `/ColorSpace` 只接受 Separation 与 DeviceN 颜色空间。
    ///
    /// 本方法只收集，不做校验；[`build`](Self::build) 在写出任何字节前
    /// 要求：组非空、页号在范围内、组内页号不重复、组间不重叠、
    /// `/DeviceColorant` 为合法 PDF 名字或非空文本串、`/ColorSpace`
    /// 为合法 Separation/DeviceN。违反者返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.3（PDF 1.3 引入）
    /// 返回 [`WriteError::UnsupportedVersion`]，两者均零字节输出。
    pub fn add_separation_group(&mut self, pages: &[usize], info: SeparationInfo) -> &mut Self {
        self.separation_groups.push(SeparationGroup {
            pages: pages.to_vec(),
            info,
        });
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
            viewports: Vec::new(),
            transition: None,
            duration: None,
            thumbnail: None,
            boundaries: None,
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

    /// 为指定页设置视口字典数组（页面字典 `/VP`，12.9.2 Table 260，PDF 1.6）。
    ///
    /// 数组按绘制顺序排列，各视口的 `/BBox` 与 `/Measure` 决定页面不同区域
    /// 的测量比例。参数在 [`PdfBuilder::build`] 阶段校验：非有限或未按
    /// 左下/右上规范化的 `/BBox`、空 `/Name`、非法测量字典返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.6 时返回
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置替换先前数组。
    pub fn set_page_viewports(&mut self, page: usize, viewports: &[Viewport]) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.viewports = viewports.to_vec();
        }
        self
    }

    /// 为指定页设置页面过渡效果（页面 `/Trans`，12.4.4 Table 30/162，PDF 1.1）。
    ///
    /// 过渡字典与 Transition 动作共用 [`Transition`] 模型；取值与样式适用性
    /// 在 [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]。目标版本低于 1.1，或使用 PDF 1.5 引入的
    /// 样式（Fly/Push/Cover/Uncover/Fade）与 `/SS` `/B` 条目，返回
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_transition(&mut self, page: usize, transition: &Transition) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.transition = Some(transition.clone());
        }
        self
    }

    /// 为指定页设置演示时的停留时长（页面 `/Dur`，12.4.4 Table 30，单位秒）。
    ///
    /// 缺省（未设置）表示查看器不自动换页。取值须为非负有限数，在
    /// [`PdfBuilder::build`] 阶段校验，非法值返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.1 时返回
    /// [`WriteError::UnsupportedVersion`] 且不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_duration(&mut self, page: usize, seconds: f64) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.duration = Some(seconds);
        }
        self
    }

    /// 为指定页设置缩略图（页面 `/Thumb`，12.3.4 Table 30，PDF 1.0）。
    ///
    /// 缩略图是页面字典直接引用的图像 XObject，不进入页面内容资源；
    /// 与页面绘制的图像共用去重池。按 12.3.4 仅接受 DeviceGray、
    /// DeviceRGB 或基于二者的 Indexed 颜色空间，且不得是图像蒙版，非法
    /// 输入在 [`PdfBuilder::build`] 阶段返回 [`WriteError::InvalidObject`]
    /// 且不写出任何字节。未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_thumbnail(&mut self, page: usize, thumbnail: &ImageData) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.thumbnail = Some(thumbnail.clone());
        }
        self
    }

    /// 为指定页设置页面边界（14.11.2 Table 30）。
    ///
    /// [`PageBoundaries`] 取代该页的 `/MediaBox`（`page(PageSize)` 声明的
    /// 尺寸不再使用），并写出显式声明的 `/CropBox` `/BleedBox` `/TrimBox`
    /// `/ArtBox` 与 `/BoxColorInfo`；缺省框不物化，由读者按 Table 30/361
    /// 推导。
    ///
    /// 取值契约（本库选择：零面积框无实际意义，一并拒绝）与版本门禁在
    /// [`PdfBuilder::build`] 阶段校验：坐标须有限且宽高为正，`/C` 分量
    /// 0.0–1.0，`/W` ≥ 0，`/D` 非空非负且不全为零并只与 `/S /D` 同用，
    /// 扩展 `/S` 名须为合法 PDF 名称；违反者返回
    /// [`WriteError::InvalidObject`]。`/BleedBox` `/TrimBox` `/ArtBox`
    /// 需 PDF 1.3，`/BoxColorInfo` 需 PDF 1.4，否则返回
    /// [`WriteError::UnsupportedVersion`]。两种失败均不写出任何字节。
    /// 未知页索引被忽略；重复设置以后一次为准。
    pub fn set_page_boundaries(&mut self, page: usize, boundaries: PageBoundaries) -> &mut Self {
        if let Some(p) = self.pages.get_mut(page) {
            p.boundaries = Some(boundaries);
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

    /// 把一个注解（12.5）绑定到 0 基页号，返回文档级句柄。
    ///
    /// 句柄供另一注解的 [`PdfAnnotation::with_popup`] 引用。注册的全部
    /// 注解在 [`PdfBuilder::build`] 时写成独立间接对象并登记进页面
    /// `/Annots`；页号越界或字典非法（[`PdfAnnotation::validate`] 语义）
    /// 返回 [`WriteError::InvalidObject`]，子类型版本不足返回
    /// [`WriteError::UnsupportedVersion`]。
    pub fn add_annotation(&mut self, page: usize, annot: PdfAnnotation) -> usize {
        let idx = self.annotations.len();
        self.annotations.push(AnnotationEntry { page, annot });
        idx
    }

    /// 注册一个声音对象（13.3 Table 294），返回文档级句柄。
    ///
    /// 句柄供 [`PdfAnnotation::with_sound`]（Sound 注解，12.5.6.16）与
    /// [`SoundAction`]（Sound 动作，12.6.4.8）引用。每个注册的声音在
    /// [`PdfBuilder::build`] 时写成独立间接流对象；采样率、声道数、
    /// 每样本位数与数据在构建阶段校验，非法输入返回
    /// [`WriteError::InvalidObject`]；目标版本低于 1.2 返回
    /// [`WriteError::UnsupportedVersion`]，均不写出任何字节。
    pub fn add_sound(&mut self, sound: Sound) -> usize {
        let handle = self.sounds.len();
        self.sounds.push(sound);
        handle
    }

    /// 注册一个 3D artwork 流（13.6 Table 300），返回文档级句柄。
    ///
    /// 句柄供 [`PdfAnnotation::with_three_d`]（3D 注解 `/3DD`）引用。
    /// 每个注册项在 [`PdfBuilder::build`] 时写成独立间接流对象；启用
    /// [`ThreeDArtwork::with_reference_dictionary`] 时另写 Table 303 引用
    /// 字典，引用同一字典的多个注解共享运行实例；声明
    /// [`ThreeDArtwork::with_on_instantiate`] 时脚本写成独立间接流并由
    /// `/OnInstantiate` 引用。数据格式、视图与默认视图在构建阶段校验，
    /// 非法输入返回 [`WriteError::InvalidObject`]；目标版本低于 1.6 返回
    /// [`WriteError::UnsupportedVersion`]，均不写出任何字节。
    pub fn add_three_d_artwork(&mut self, artwork: ThreeDArtwork) -> usize {
        let handle = self.three_d_artworks.len();
        self.three_d_artworks.push(artwork);
        handle
    }

    /// 设置交互表单（AcroForm，12.7.2–12.7.4）。
    ///
    /// 字段树在 [`PdfBuilder::build`] 时写成独立间接对象：目录
    /// `/AcroForm /Fields` 引用顶层字段，页面 `/Annots` 引用各 Widget，
    /// 字段间 `/Kids`/`/Parent` 互引。Widget 外观流 `/AP` 复用
    /// [`PdfBuilder::form_xobject`] 池。页号越界、字段树结构非法或
    /// `/AP` 句柄未知返回 [`WriteError::InvalidObject`]；目标版本低于
    /// 1.2 返回 [`WriteError::UnsupportedVersion`]，均不写出任何字节。
    pub fn set_acro_form(&mut self, form: AcroForm) -> &mut Self {
        self.acro_form = Some(form);
        self
    }

    /// 注册一个嵌入文件（7.11.3–7.11.4），返回文档级句柄。
    ///
    /// `build` 时每个注册项展开为文件规格字典 + 嵌入文件流一对间接对象，
    /// 并登记进目录 `/Names /EmbeddedFiles` 名称树；句柄供
    /// [`PdfAnnotation::with_filespec`]（FileAttachment 注解 `/FS`）引用。
    /// 名称为空/含路径分隔符/重复或名称树键非法返回
    /// [`WriteError::InvalidObject`]；嵌入文件要求目标版本 ≥1.3，`/Desc`
    /// 要求 ≥1.6，非 ASCII 展示名（`/UF`）要求 ≥1.7（ISO 32000-1 Table 44），
    /// 不满足返回 [`WriteError::UnsupportedVersion`]，均零字节输出。
    pub fn embed_file(&mut self, file: EmbeddedFile) -> usize {
        let handle = self.embedded.len();
        self.embedded.push(file);
        handle
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
        navigation::apply(
            &mut doc,
            self.version,
            self.pages.len(),
            &self.navigation,
            types::ActionRefs {
                annotation_nums: &self.annotation_nums,
                ocg_nums: &self.ocg_nums,
                sound_nums: &self.sound_nums,
                form_nums: &self.form_nums,
                embedded_nums: &self.embedded_nums,
                rendition_nums: &self.rendition_nums,
                rendition_cursor: Some(&self.rendition_cursor),
            },
        )?;
        // 内联 section 的消费游标必须覆盖全部首级目标：遗漏说明注解/大纲
        // 的写出顺序与分配顺序不一致，此时在任何字节写出前拒绝。
        if self.rendition_cursor.get() != self.rendition_nums.len() {
            return Err(WriteError::InvalidObject(format!(
                "media clip section inline /D wrote {} of {} allocated roots",
                self.rendition_cursor.get(),
                self.rendition_nums.len()
            )));
        }
        let mut writer = DocumentWriter::new().with_version(self.version);
        if let Some(compression) = self.compression.clone() {
            writer = writer.with_compression(compression);
        }
        writer.build(&doc, w)
    }
}
