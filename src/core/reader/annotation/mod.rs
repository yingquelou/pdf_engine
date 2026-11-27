//! 注解读取（12.5.1–12.5.6）。
//!
//! - 页面 `/Annots` 数组遍历（直接字典与间接引用均支持），深度/数量/环防护。
//! - 通用注解字典条目：`/Rect`、`/Subtype`、`/F` 标志（Table 165）、
//!   `/Contents`、`/T`、`/NM`、`/M`、`/C`、`/CA`、`/AP`/`/AS` 外观流、
//!   `/BS`/`/BM`/`/Border` 边框（12.5.4）；`/T`、`/Subj`、`/NM` 与 FreeText
//!   `/DS` 的 7.9.2.2 语言转义视图在
//!   [`Annotation::title_semantic_text`]、[`Annotation::subject_semantic_text`]、
//!   [`Annotation::name_semantic_text`] 与
//!   [`Annotation::default_style_semantic_text`]（`annotation/language_escape.rs`）；
//!   `/Contents` 逐段视图与未建模的 `/TM` 在 `annotation/language_escape_contents.rs`
//!   与 `annotation/language_escape_modified_text.rs`。
//! - 标记类注解附加条目（12.5.6.2 Table 170）：`/Subj` 主题、`/IRT` 所回复
//!   注解的间接引用、`/RT` 回复类型，解析在 [`reply`]。
//! - 标记类注解公共条目 `/QuadPoints`（12.5.6.4）；`/A` 原样保留、动作语义由
//!   E3 的 [`crate::core::reader::action`] 解析，`/Dest` 经
//!   `Document::annotation_destination` 解析为 12.3.2 目标模型。
//! - 子类型表（12.5.3 Table 169：全 26 种名称、引入版本与标记类归属）在 [`kind`]，
//!   写端子集经 `AnnotationKind::subtype` 复用同一张表。
//! - 子类型专有条目类型化（12.5.6，解析在 [`subtype`]）：Text `/Name`、`/Open`、
//!   `/State`、`/StateModel`；Caret `/Symbol`；Ink `/InkList` 路径；
//!   Line `/L`、`/LE`、`/LL`、`/LLE`、`/Cap`、`/IT`、`/LLO`、`/CP`、`/CO`
//!   （Table 175/176，端点样式与意图、题注位置类型化为枚举）；
//!   FreeText `/DA`、`/Q`、`/DS`、`/CL`、`/IT`、`/BE`、`/RD`、`/LE`
//!   （Table 174 + Table 167，`/RC` 属标记类通用条目）；
//!   Square/Circle `/BE`、`/RD`（Table 177，与 FreeText 共用键）；
//!   Polygon/PolyLine `/Vertices`、`/LE`、`/BE`、`/IT`（Table 178）；
//!   Highlight/Underline/Squiggly/StrikeOut 经
//!   [`Annotation::text_markup_quad_groups`] 按 8 数一组类型化 `/QuadPoints`
//!   （Table 179，四型共用；分组访问统一走 [`Annotation::quad_groups`]）；
//!   Stamp `/Name` 图章图标名类型化为 [`StampIcon`]（Table 181，14 个标准名，
//!   未知名原样保留于 `StampIcon::Other` 且可判别，缺省 `Draft`）；
//!   Link `/H` 高亮模式与 `/PA` 按下 URI 动作（Table 173，`/PA` 经
//!   `Document::annotation_pressed_action` 解析为 12.6 动作模型）。
//! - Sound（Table 185）：`/Sound` 声音流引用经 `Document::annotation_sound`
//!   解析为 13.3 声音模型；`/Name` 图标名复用通用 `icon_name`
//!   （标准值 `Speaker`、`Mic`，缺省 `Speaker`）。
//! - Measure 字典（12.9，Table 261–263）：Line 与 Polygon/PolyLine 的
//!   `/Measure` 类型化为 [`Measure`]（直角坐标条目 + 数字格式数组），
//!   解析在 [`measure`]。
//! - 3D（13.6，Table 298–306）：`/3DD`（3D 流或 3D 引用字典）、`/3DV`
//!   视图选择、`/3DA` 激活字典、`/3DI` 交互标志与 `/3DB` 视图框类型化于
//!   [`crate::core::reader::three_d`]；视图、投影与背景字典随之解析。
//! - 外观特性字典 `/MK`（Table 189，含图标适配字典 `/IF`，Table 247）类型化为
//!   [`AppearanceCharacteristics`]，解析在 [`mk`]。
//! - Watermark 注解的固定打印字典 `/FixedPrint`（Table 190/191）类型化为
//!   [`FixedPrint`]，解析在 [`watermark`]。
//! - Printer's mark 注解（14.11.3，Table 362/363）：`/MN` 名称与按
//!   `/AP`/`/AS` 选定的外观表单（`/MarkStyle`、`/Colorants`）类型化为
//!   [`PrinterMark`]，解析在 [`printer_mark`]。
//! - Trap 网络注解（14.11.6，Table 366/367）：`/LastModified` 或
//!   `/Version` + `/AnnotStates` 跟踪条目与选定的外观表单（`/PCM` 等）
//!   类型化为 [`TrapNetwork`]，解析在 [`trap_net`]。
//! - 3D 标记注解的外部数据字典 `/ExData`（13.6.6 与 Table 170/313，
//!   PDF 1.7）：`/Subtype /Markup3D` 的 `/3DA` `/3DV` `/MD5` 类型化为
//!   [`Markup3D`]，解析在 [`exdata`]。
//! - 未建模条目保留在 [`Annotation::dict`] 中（FileSpec、Widget 字段链、
//!   Line `/DS` 等）。
//!
//! 通用条目解析在 `entries`，子类型专有条目解析在 `subtype`，
//! `/F` 标志位在 [`flags`]。

#[cfg(test)]
mod tests;

mod entries;
mod error;
mod exdata;
mod flags;
mod kind;
mod language_escape;
mod language_escape_contents;
mod language_escape_modified_text;
mod link;
mod measure;
mod mk;
mod model;
mod movie;
mod parse;
mod printer_mark;
mod queries;
mod redact;
mod reply;
mod sound;
pub mod subtype;
mod trap_net;
mod watermark;

// 拆分后本文件不再调用这两个名字，但 `#[cfg(test)] mod tests` 经
// `use crate::core::reader::annotation::*` 全局导入用到 `parse_annotation`，
// `tests/support.rs` 经 `crate::core::reader::annotation::Document` 路径用到 `Document`；
// 用 `cfg(test)` 保留绑定，避免非测试构建下出现未使用导入告警。
#[cfg(test)]
use crate::core::types::Document;
#[cfg(test)]
use parse::parse_annotation;

pub use error::AnnotationError;
pub use exdata::{ExDataSubtype, ExternalData, Markup3D, Markup3DAnnotation};
pub use flags::AnnotationFlags;
pub use kind::{AnnotationSubtype, Row, TABLE_169};
pub use link::LinkHighlightMode;
pub(crate) use measure::parse_measure_value;
pub use measure::{Measure, NumberFormat, NumberFraction, NumberLabelOrder};
pub use mk::{AppearanceCharacteristics, IconFit, IconScaleWhen, IconScaling};
pub use model::{
    Annotation, AnnotationBorder, AppearanceStreams, StandardAnnotationState, StandardStateModel,
};
pub use printer_mark::{PrinterMark, PrinterMarkForm};
pub use reply::ReplyType;
pub use subtype::{
    BorderEffect, BorderEffectStyle, CalloutLine, CaptionPosition, FreeTextIntent, LineEndStyle,
    LineIntent, PolygonIntent, StampIcon,
};
pub use trap_net::{ProcessColorModel, TrapNetwork, TrapNetworkForm};
pub use watermark::FixedPrint;

/// 单页注解数量上限。
pub const MAX_ANNOTATIONS_PER_PAGE: usize = 4096;

type Result<T> = std::result::Result<T, AnnotationError>;

fn malformed(message: impl Into<String>) -> AnnotationError {
    AnnotationError::Malformed(message.into())
}
