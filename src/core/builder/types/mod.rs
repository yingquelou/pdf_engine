//! 逻辑层公共数据类型：页面尺寸、字体与编码配置、图片数据。
//!
//! 按领域拆分为子模块，并由本模块统一 `pub use`：
//! - `action`：动作字典（12.6 的 Hide/Set-OCG-State/Transition 与 12.7.5 表单动作）；
//! - `annotation`：注解字典（12.5 通用条目与常用子类型）；
//! - `descriptor`：字体描述符度量；
//! - `embedded_file`：嵌入文件与文件规格（7.11）；
//! - `encoding`：简单字体编码配置；
//! - `font`：逻辑字体描述；
//! - `type3`：Type 3 字体与字形；
//! - `cid`：CIDFont、CID 编码与宽度；
//! - `program`：嵌入字体程序；
//! - `image`：图片数据与颜色空间；
//! - `form`：Form XObject 与组属性字典；
//! - `form_field`：交互表单（AcroForm 字段树与 Widget 注解）；
//! - `function`：PDF 函数（Type 0/2/3/4）；
//! - `graphics_state`：图形状态参数（ExtGState）；
//! - `halftone`：半色调字典（Type 1/2 与包装字典）；
//! - `optional_content`：可选内容组（OCG）；
//! - `opi`：OPI 代理字典（14.11.7 Table 368–370）；
//! - `output_intent`：输出意图（14.11.5 Table 365）；
//! - `separation`：分色字典（14.11.4 Table 364）；
//! - `page_box`：页面边界与框颜色信息（14.11.2 Table 30/360/361）；
//! - `shading`：渐变（Shading Type 1–5）与着色图案（Shading Pattern）；
//! - `pattern`：平铺图案（Tiling Pattern）与统一图案池；
//! - `rendition`：多媒体 rendition 与 media clip 对象（13.2）；
//! - `sound`：声音对象（13.3 Table 294）；
//! - `span`：Span 标记内容属性表（14.9.2.1、14.9.3–14.9.5）；
//! - `three_d`：3D artwork、视图、投影、背景与激活字典（13.6）；
//! - `transition`：过渡字典（12.4.4 Table 162 与 Transition 动作 12.6.4.14）；
//! - `xmp`：类型化 XMP 包（14.3.2）的构造与 RDF/XML 序列化。

mod action;
mod annotation;
mod cid;
mod descriptor;
mod embedded_file;
mod encoding;
mod font;
mod form;
mod form_field;
mod function;
mod graphics_state;
mod halftone;
mod image;
mod movie;
mod opi;
mod optional_content;
mod output_intent;
mod page_box;
mod pattern;
mod program;
mod rendition;
mod separation;
mod shading;
mod sound;
mod span;
mod structure;
mod three_d;
mod transition;
mod type3;
mod viewport;
mod xmp;

pub(crate) use action::ActionRefs;
pub use action::{
    Action, ActionFileSpec, ActionNamedAction, ActionRemoteDestination, GoTo3DViewAction,
    GoTo3DViewSelection, GoToEAction, GoToETarget, GoToETargetAnnotation, GoToETargetPage,
    GoToETargetRelation, HideAction, HideTarget, MovieAction, MovieActionTarget, OcgStateOperation,
    OcgStateSequence, RenditionAction, RenditionOperation, SetOcgStateAction, SoundAction,
    ThreadAction,
};
pub(crate) use annotation::validate_name_value;
pub use annotation::{
    AnnotationBorderStyle, AnnotationDestination, AnnotationKind, AppearanceCharacteristics,
    BorderArray, BorderEffect, BorderEffectStyle, CalloutLine, CaptionPosition, FixedPrint,
    FreeTextIntent, IconFit, IconScaleWhen, IconScaling, LineEndStyle, LineIntent,
    LinkHighlightMode, Markup3D, Measure, NumberFormat, NumberFraction, NumberLabelOrder,
    PdfAnnotation, PolygonIntent,
};
pub use cid::{CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, CidVerticalMetrics, CidWidth};
pub use descriptor::FontDescriptor;
pub use embedded_file::{
    CollectionItem, CollectionItemValue, CollectionSubitem, CollectionSubitemValue, EmbeddedFile,
};
pub use encoding::FontEncoding;
pub use font::{Font, FontWidths};
pub use form::{FormResources, FormXObject, GroupXObject};
pub use form_field::{
    AcroForm, FieldActionEvent, FieldValue, FormField, FormFieldType, FormOption,
};
pub use function::{
    CalculatorFunction, ExponentialFunction, Function, SampledFunction, StitchingFunction,
};
pub(crate) use graphics_state::GraphicsStateFunctions;
pub use graphics_state::{
    DashPattern, GraphicsState, GraphicsStateFont, SoftMask, SoftMaskSubtype,
};
pub use halftone::{Halftone, HalftoneDictionary, Type1Halftone, Type2Halftone};
pub use image::{
    CalGray, CalRGB, ColorSpace, DeviceN, ICCBased, ImageData, ImageIntent, ImageMask, Indexed,
    Lab, Separation,
};
pub use movie::{Movie, MovieActivation, MoviePlayback};
pub use opi::{
    Opi13Dictionary, Opi20Dictionary, OpiColor, OpiColorType, OpiInks, OpiProxy, OpiTagText,
    OpiVersion,
};
pub use optional_content::OptionalContentGroup;
pub(crate) use optional_content::text_string_value;
pub use output_intent::{OutputIntent, OutputIntentProfile};
pub use page_box::PageBoundaries;
pub(crate) use pattern::Pattern;
pub use pattern::{PaintType, TilingPattern, TilingType};
pub use program::FontProgram;
pub(crate) use program::OpenTypeFlavor;
pub(crate) use rendition::ClipRefs;
pub use rendition::{
    MediaClip, MediaClipData, MediaClipSection, MediaClipSource, MediaDuration, MediaFit,
    MediaFloatingWindow, MediaPermissions, MediaPlayParameters, MediaPlaySettings, MediaPlayerInfo,
    MediaPlayers, MediaRendition, MediaScreenParameters, MediaScreenSettings, MediaTimespan,
    MediaWindow, Rendition, SelectorRendition, SoftwareIdentifier, TemporaryFilePermission,
};
pub(crate) use separation::SeparationGroup;
pub use separation::{DeviceColorant, SeparationInfo};
pub(crate) use shading::mesh_component_ranges;
pub use shading::{
    AxialShading, CoonsPatch, CoonsPatchShading, FreeFormMeshShading, FunctionShading,
    LatticeMeshShading, MeshFunction, MeshVertex, RadialShading, Shading, ShadingOptions,
    ShadingPattern, TensorProductPatch, TensorProductPatchShading,
};
pub use sound::{Sound, SoundEncoding};
pub use span::SpanProperties;
pub use structure::{
    BlockAlign, BorderColor, BorderStyle, BorderStyles, BorderThickness, GlyphOrientationVertical,
    InlineAlign, LayoutAttributes, LineHeight, ListAttributes, ListNumbering, NumberOrAuto,
    NumberOrFour, NumberOrNumbers, Placement, PrintFieldAttributes, PrintFieldChecked,
    PrintFieldRole, RubyAlign, RubyPosition, StructureAttribute, StructureElement, StructureKid,
    StructureMarkInfo, StructureTree, TableAttributes, TableScope, TextAlign, TextDecorationType,
    UserProperty, WritingMode,
};
pub use three_d::{
    ThreeDActivation, ThreeDActivationPolicy, ThreeDAnimationStyle, ThreeDAnimationStyleType,
    ThreeDArtwork, ThreeDArtworkState, ThreeDBackground, ThreeDBackgroundType, ThreeDClipping,
    ThreeDColor, ThreeDCrossSection, ThreeDDeactivationPolicy, ThreeDFaceColor,
    ThreeDLightingScheme, ThreeDLightingSchemeType, ThreeDMatrixSource, ThreeDNode,
    ThreeDProjection, ThreeDProjectionBinding, ThreeDProjectionScale, ThreeDProjectionType,
    ThreeDRenderMode, ThreeDRenderModeType, ThreeDStreamFormat, ThreeDView, ThreeDViewSelection,
};
pub use transition::{
    Transition, TransitionDimension, TransitionDirection, TransitionMotion, TransitionStyle,
};
pub use type3::{Type3Font, Type3Glyph};
pub use viewport::Viewport;
pub use xmp::{XmpListItem, XmpListKind, XmpPacket, XmpPacketError, XmpValue};

/// 页面尺寸预设。
///
/// 提供 A4、Letter 两种常用规格（单位 pt），以及可选的自定义宽高。
/// 用于 [`crate::core::builder::PdfBuilder::page`] 声明新页的 MediaBox。
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
    pub(super) fn dimensions(&self) -> (f64, f64) {
        match self {
            PageSize::A4 => (595.0, 842.0),
            PageSize::Letter => (612.0, 792.0),
            PageSize::Custom(w, h) => (*w, *h),
        }
    }
}
