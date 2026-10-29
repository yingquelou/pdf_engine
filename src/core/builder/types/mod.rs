//! 逻辑层公共数据类型：页面尺寸、字体与编码配置、图片数据。
//!
//! 按领域拆分为子模块，并由本模块统一 `pub use`：
//! - `action`：动作字典（GoTo/GoToR/Named/URI/Launch/JavaScript）；
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
//! - `shading`：渐变（Shading Type 1–5）与着色图案（Shading Pattern）；
//! - `pattern`：平铺图案（Tiling Pattern）与统一图案池。

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
mod optional_content;
mod pattern;
mod program;
mod shading;
mod type3;

pub use action::{Action, ActionFileSpec, ActionNamedAction, ActionRemoteDestination};
pub use annotation::{
    AnnotationBorderStyle, AnnotationDestination, AnnotationKind, BorderEffect, BorderEffectStyle,
    CalloutLine, CaptionPosition, FreeTextIntent, LineEndStyle, LineIntent, LinkHighlightMode,
    Measure, NumberFormat, NumberFraction, NumberLabelOrder, PdfAnnotation, PolygonIntent,
};
pub use cid::{CidEncoding, CidFont, CidFontSubtype, CidSystemInfo, CidVerticalMetrics, CidWidth};
pub use descriptor::FontDescriptor;
pub use embedded_file::EmbeddedFile;
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
pub use optional_content::OptionalContentGroup;
pub(crate) use optional_content::text_string_value;
pub(crate) use pattern::Pattern;
pub use pattern::{PaintType, TilingPattern, TilingType};
pub use program::FontProgram;
pub(crate) use program::OpenTypeFlavor;
pub(crate) use shading::mesh_component_ranges;
pub use shading::{
    AxialShading, CoonsPatch, CoonsPatchShading, FreeFormMeshShading, FunctionShading,
    LatticeMeshShading, MeshFunction, MeshVertex, RadialShading, Shading, ShadingOptions,
    ShadingPattern, TensorProductPatch, TensorProductPatchShading,
};
pub use type3::{Type3Font, Type3Glyph};

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
