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
mod document;
mod marked_content;
mod metadata;
mod navigation;
mod output;
mod page;
mod patterns;
mod resources;
#[cfg(test)]
mod tests;
mod text_codes;
mod types;

pub use crate::core::reader::accessibility::LanguageIdentifier;
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
pub use types::requirements::{
    DocumentRequirement, DocumentRequirementType, RequirementHandler, RequirementHandlerType,
};
pub use types::{
    AcroForm, Action, ActionFileSpec, ActionNamedAction, ActionRemoteDestination, AlternateImage,
    AnnotationBorderStyle, AnnotationDestination, AnnotationKind, AppearanceCharacteristics,
    ArtifactAttachment, ArtifactProperties, ArtifactSubtype, ArtifactType, AxialShading,
    BlockAlign, BorderArray, BorderColor, BorderEffect, BorderEffectStyle, BorderStyle,
    BorderStyles, BorderThickness, CalGray, CalRGB, CalculatorFunction, CalloutLine,
    CaptionPosition, CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, CidVerticalMetrics,
    CidWidth, CollectionItem, CollectionItemValue, CollectionSubitem, CollectionSubitemValue,
    ColorSpace, CoonsPatch, CoonsPatchShading, DashPattern, DefaultColorSpace, DeviceColorant,
    DeviceN, EmbeddedFile,
    ExponentialFunction, FieldActionEvent, FieldValue, FixedPrint, Font, FontDescriptor,
    FontEncoding, FontProgram, FontWidths, FormField, FormFieldType, FormOption, FormResources,
    FormXObject, FormatSpecificAttributes, FormatSpecificOwner, FreeFormMeshShading,
    FreeTextIntent, Function, FunctionShading, GlyphOrientationVertical, GoTo3DViewAction,
    GoTo3DViewSelection, GoToEAction, GoToETarget, GoToETargetAnnotation, GoToETargetPage,
    GoToETargetRelation, GraphicsState, GraphicsStateFont, GroupXObject, Halftone,
    HalftoneDictionary, HideAction, HideTarget, ICCBased, IconFit, IconScaleWhen, IconScaling,
    ImageData, ImageIntent, ImageMask, Indexed, InlineAlign, Lab, LatticeMeshShading,
    LayoutAttributes, LineEndStyle, LineHeight, LineIntent, LinkHighlightMode, ListAttributes,
    ListNumbering, Markup3D, Measure, MediaClip, MediaClipData, MediaClipSection,
    MediaClipSectionHandler, MediaClipSource, MediaDuration, MediaFit, MediaFloatingWindow,
    MediaPermissions, MediaPlayParameters, MediaPlaySettings, MediaPlayerInfo, MediaPlayers,
    MediaRendition, MediaScreenParameters, MediaScreenSettings, MediaTimespan, MediaWindow,
    MeshFunction, MeshVertex, Movie, MovieAction, MovieActionTarget, MovieActivation,
    MoviePlayback, NumberFormat, NumberFraction, NumberLabelOrder, NumberOrAuto, NumberOrFour,
    NumberOrNumbers, OcgStateOperation, OcgStateSequence, Opi13Dictionary, Opi20Dictionary,
    OpiColor, OpiColorType, OpiInks, OpiProxy, OpiTagText, OpiVersion, OptionalContentGroup,
    OutputIntent, OutputIntentProfile, PageBoundaries, PageSize, PaintType, PdfAnnotation,
    PieceInfo, PieceInfoData, Placement, PolygonIntent, PrintFieldAttributes, PrintFieldChecked,
    PrintFieldRole, RadialShading, Rendition, RenditionAction, RenditionOperation, ReplyType,
    RubyAlign, RubyPosition, SampledFunction, SelectorRendition, Separation, SeparationInfo,
    SetOcgStateAction, Shading, ShadingOptions, ShadingPattern, SoftMask, SoftMaskSubtype,
    SoftwareIdentifier, Sound, SoundAction, SoundEncoding, SpanProperties, StampIcon,
    StitchingFunction, StructureAttribute, StructureElement, StructureKid, StructureMarkInfo,
    StructureObjectReference, StructureTree, TableAttributes, TableScope, TemporaryFilePermission,
    TensorProductPatch, TensorProductPatchShading, TextAlign, TextDecorationType, ThreadAction,
    ThreeDActivation, ThreeDActivationPolicy, ThreeDAnimationStyle, ThreeDAnimationStyleType,
    ThreeDArtwork, ThreeDArtworkState, ThreeDBackground, ThreeDBackgroundType, ThreeDClipping,
    ThreeDColor, ThreeDCrossSection, ThreeDDeactivationPolicy, ThreeDFaceColor,
    ThreeDLightingScheme, ThreeDLightingSchemeType, ThreeDMatrixSource, ThreeDNode,
    ThreeDProjection, ThreeDProjectionBinding, ThreeDProjectionScale, ThreeDProjectionType,
    ThreeDRenderMode, ThreeDRenderModeType, ThreeDStreamFormat, ThreeDView, ThreeDViewSelection,
    TilingPattern, TilingType, Transition, TransitionDimension, TransitionDirection,
    TransitionMotion, TransitionStyle, Type1Halftone, Type2Halftone, Type3Font, Type3Glyph,
    UserProperty, Viewport, WebCaptureCommand, WebCaptureCommandSettings, WebCaptureImageSet,
    WebCapturePageSet, WebCaptureSource, WritingMode, XmpListItem, XmpListKind, XmpPacket,
    XmpPacketError, XmpValue,
};
pub use types::{
    Duplex, NonFullScreenPageMode, PageBoundaryName, PrintScaling, ReadingDirection,
    ViewerPreferences,
};

use crate::core::content::ContentStream;
use crate::core::types::{Dict, PdfVersion};
use crate::core::writer::StreamCompression;
use std::cell::Cell;

/// 单页的构建态：尚未分配对象号的内容。
#[derive(Debug)]
struct PageBuild {
    size: PageSize,
    /// 累积的内容流操作符序列（类型化引擎）。`text`/`image` 调用追加于此。
    content: ContentStream,
    /// 追加的内容流（7.7.3.3 `/Contents` 可为流数组）。按追加顺序写出，
    /// 与主内容流共同组成页面的内容序列；默认空，写出形态与单流一致。
    extra_contents: Vec<ContentStream>,
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
    /// 页面资源 `/DefaultGray` `/DefaultRGB` `/DefaultCMYK`（§8.6.3，PDF 1.3）。
    default_color_spaces: Vec<(DefaultColorSpace, ColorSpace)>,
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
    /// 页面件字典（14.5 `/PieceInfo`，PDF 1.3）。
    piece_info: Option<PieceInfo>,
    /// 页面首选缩放（Table 30 `/PZ`，14.10.6，PDF 1.3）。
    preferred_zoom: Option<f64>,
    /// 页面 Web Capture 数字标识符（Table 30 `/ID`，14.10.6，PDF 1.3）。
    web_capture_id: Option<Vec<u8>>,
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
    /// 最近一次 assemble 记录的注解绑定页索引（下标即注解句柄）；供
    /// 结构树 OBJR 子项解析 `/Pg`。
    annotation_pages: Vec<usize>,
    /// 最近一次 assemble 分配的图像 XObject 对象号（页 → 该页图像资源
    /// 序号）；供结构树 OBJR 子项（14.7.4.3 Table 325）解析 `/Obj`。
    image_nums: Vec<Vec<i64>>,
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
    /// 文档级标记内容属性表池（14.6）；
    /// [`PdfBuilder::add_marked_content_properties`] 返回其索引（句柄）。
    /// 装配期只为内容流实际引用的句柄从共享游标分配对象号，并把页面内容
    /// 里的未定型占位替换成 `<实际号> 0 R` 引用（见
    /// `assemble::marked_properties`）。
    marked_properties: Vec<Dict>,
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
    /// 目录 `/Lang` 文档默认语言（14.9.2.1）；`None`（默认）时不写出。
    document_language: Option<LanguageIdentifier>,
    /// 目录 `/Version`（7.7.2 Table 28，PDF 1.4）：覆盖文件头声明的版本；
    /// `None`（默认）时不写出。
    catalog_version: Option<PdfVersion>,
    /// trailer `/ID` 文件标识符（14.4）；`None`（默认）时不写出。
    file_identifier: Option<FileIdentifier>,
    /// XMP 元数据流原文（14.3.2）；`None`（默认）时不写出。
    xmp_metadata: Option<Vec<u8>>,
    /// 类型化 XMP 包（14.3.2）；与 `xmp_metadata` 互斥。
    xmp_packet: Option<XmpPacket>,
    /// 目录 `/PieceInfo` 页面件字典（14.5，PDF 1.4）；`None` 不写出。
    catalog_piece_info: Option<PieceInfo>,
    /// 目录 `/ViewerPreferences` 查看器首选项（12.2，Table 150）；
    /// `None`（默认）时不写出。
    viewer_preferences: Option<ViewerPreferences>,
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
            annotation_pages: Vec::new(),
            image_nums: Vec::new(),
            ocg_nums: Vec::new(),
            sounds: Vec::new(),
            sound_nums: Vec::new(),
            three_d_artworks: Vec::new(),
            marked_properties: Vec::new(),
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
            document_language: None,
            catalog_version: None,
            file_identifier: None,
            xmp_metadata: None,
            xmp_packet: None,
            catalog_piece_info: None,
            viewer_preferences: None,
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
            annotation_pages: Vec::new(),
            image_nums: Vec::new(),
            ocg_nums: Vec::new(),
            sounds: Vec::new(),
            sound_nums: Vec::new(),
            three_d_artworks: Vec::new(),
            marked_properties: Vec::new(),
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
            document_language: None,
            catalog_version: None,
            file_identifier: None,
            xmp_metadata: None,
            xmp_packet: None,
            catalog_piece_info: None,
            viewer_preferences: None,
        }
    }
}
