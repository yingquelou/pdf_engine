//! 注解读取（12.5.1–12.5.6）。
//!
//! - 页面 `/Annots` 数组遍历（直接字典与间接引用均支持），深度/数量/环防护。
//! - 通用注解字典条目：`/Rect`、`/Subtype`、`/F` 标志（Table 165）、
//!   `/Contents`、`/T`、`/NM`、`/M`、`/C`、`/CA`、`/AP`/`/AS` 外观流、
//!   `/BS`/`/BM`/`/Border` 边框（12.5.4）。
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
//! - 3D 标记注解的外部数据字典 `/ExData`（13.6.6 与 Table 170/313，
//!   PDF 1.7）：`/Subtype /Markup3D` 的 `/3DA` `/3DV` `/MD5` 类型化为
//!   [`Markup3D`]，解析在 [`exdata`]。
//! - 未建模条目保留在 [`Annotation::dict`] 中（FileSpec、Widget 字段链、
//!   Line `/DS` 等）。
//!
//! 通用条目解析在 `entries`，子类型专有条目解析在 `subtype`，
//! `/F` 标志位在 [`flags`]。

use std::collections::HashSet;
use std::fmt;

use crate::core::reader::action::{Action, ActionError};
use crate::core::reader::movie::{Movie, MoviePlayback};
use crate::core::reader::navigation::{NavigationError, OutlineDestination};
use crate::core::reader::sound::Sound;
use crate::core::reader::three_d::{ThreeDActivation, ThreeDArtwork, ThreeDViewSelection};
use crate::core::types::{Dict, Document, Value};

#[cfg(test)]
mod tests;

mod entries;
mod exdata;
mod flags;
mod kind;
mod link;
mod measure;
mod mk;
mod movie;
mod parse;
mod printer_mark;
mod redact;
mod sound;
pub mod subtype;
mod watermark;

pub use exdata::{ExDataSubtype, ExternalData, Markup3D, Markup3DAnnotation};
pub use flags::AnnotationFlags;
pub use kind::{AnnotationSubtype, Row, TABLE_169};
pub use link::LinkHighlightMode;
pub(crate) use measure::parse_measure_value;
pub use measure::{Measure, NumberFormat, NumberFraction, NumberLabelOrder};
pub use mk::{AppearanceCharacteristics, IconFit, IconScaleWhen, IconScaling};
use parse::parse_annotation;
pub use printer_mark::{PrinterMark, PrinterMarkForm};
pub use subtype::{
    BorderEffect, BorderEffectStyle, CalloutLine, CaptionPosition, FreeTextIntent, LineEndStyle,
    LineIntent, PolygonIntent,
};
pub use watermark::FixedPrint;

/// 页面树遍历最大深度。
const MAX_PAGE_DEPTH: usize = 64;
/// 单页注解数量上限。
pub const MAX_ANNOTATIONS_PER_PAGE: usize = 4096;

/// 注解读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// 注解条目缺失、类型错误或不可解析（阻塞级字段校验失败）。
    Malformed(String),
    /// 页码越界或页面节点无法解析为字典。
    PageNotFound,
    /// 超出遍历深度或注解数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for AnnotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::PageNotFound => write!(f, "page not found"),
            Self::LimitExceeded(what) => write!(f, "annotation limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for AnnotationError {}

impl From<NavigationError> for AnnotationError {
    fn from(error: NavigationError) -> Self {
        match error {
            NavigationError::CatalogMissing => Self::CatalogMissing,
            NavigationError::Malformed(message) => Self::Malformed(message),
            NavigationError::PageNotFound => Self::PageNotFound,
            NavigationError::LimitExceeded(what) => Self::LimitExceeded(what),
        }
    }
}

impl From<ActionError> for AnnotationError {
    fn from(error: ActionError) -> Self {
        match error {
            ActionError::Malformed(message) => Self::Malformed(message),
            ActionError::PageNotFound => Self::PageNotFound,
        }
    }
}

type Result<T> = std::result::Result<T, AnnotationError>;

fn malformed(message: impl Into<String>) -> AnnotationError {
    AnnotationError::Malformed(message.into())
}

/// 外观流字典 `/AP`（12.5.5）：正常/按下/悬停三种状态。
///
/// 每个状态的值既可能是外观流引用，也可能是「状态名 → 外观流引用」
/// 的子字典（如 Widget 的多状态外观），此处原样保留供调用方解析。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppearanceStreams {
    pub normal: Option<Value>,
    pub down: Option<Value>,
    pub rollover: Option<Value>,
}

impl AppearanceStreams {
    /// 按状态键（"N"/"D"/"R"）取外观流值。
    pub fn state(&self, key: &str) -> Option<&Value> {
        match key {
            "N" => self.normal.as_ref(),
            "D" => self.down.as_ref(),
            "R" => self.rollover.as_ref(),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.normal.is_none() && self.down.is_none() && self.rollover.is_none()
    }
}

/// 边框样式（12.5.4）：优先 `/BS` 字典，回退旧式 `/Border` 数组。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnnotationBorder {
    /// `/BS /W` 或 `/Border` 第三项（宽度）。
    pub width: Option<f64>,
    /// `/BS /S`：Solid/Beveled/Dashed/Inset/Underline。
    pub style: Option<String>,
    /// `/BS /D` 或 `/Border` 第四项的虚线数组。
    pub dash: Vec<f64>,
    /// `/BM` 混合模式名。
    pub blend_mode: Option<String>,
    /// 旧式 `/Border` 前两项（水平、垂直圆角半径），`/BS` 路径下为 `None`。
    pub corner_radii: Option<[f64; 2]>,
}

/// 单条注解：通用条目类型化，未建模条目保留在 [`Self::dict`]。
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// 所在页索引（0 基）。
    pub page_index: usize,
    /// 注解对象的间接引用；直接嵌入 `/Annots` 时为 `None`。
    pub reference: Option<(i64, i64)>,
    pub subtype: AnnotationSubtype,
    /// `/Rect`（页面坐标系，未归一化）。
    pub rect: [f64; 4],
    pub flags: AnnotationFlags,
    /// `/Contents` 文本串。
    pub contents: Option<Value>,
    /// `/RC` 富内容（PDF 1.7 文本注释）。
    pub rich_contents: Option<Value>,
    /// `/T` 弹出窗口标题。
    pub title: Option<Value>,
    /// `/NM` 注解名（跨更新标识）。
    pub name: Option<Value>,
    /// `/M` 修改日期文本串。
    pub modified_date: Option<Value>,
    /// `/CreationDate` 创建日期文本串。
    pub creation_date: Option<Value>,
    /// `/C` 标注颜色。
    pub color: Option<Vec<f64>>,
    /// `/IC` 背景色。
    pub interior_color: Option<Vec<f64>>,
    /// `/CA` 不透明度（0.0–1.0）。
    pub opacity: Option<f64>,
    pub border: Option<AnnotationBorder>,
    pub appearance: Option<AppearanceStreams>,
    /// `/AS` 当前外观状态名。
    pub appearance_state: Option<String>,
    /// Printer's mark 注解的 `/MN` 与外观表单条目（14.11.3 Table 362/363）。
    pub printer_mark: Option<PrinterMark>,
    /// `/QuadPoints`（标记类注解，长度为 8 的倍数）。
    pub quad_points: Option<Vec<f64>>,
    /// `/Name` 图标名（Text 注解专有条目；名字或文本串，取值由查看器定义）。
    pub icon_name: Option<String>,
    /// `/Sound` 声音流对象引用（Sound 注解必填，Table 185）；类型化解析见
    /// [`Document::annotation_sound`]。
    pub sound: Option<Value>,
    /// `/Open` Text/Popup 注解气泡是否展开（Table 172/183；Popup 缺省 false）。
    pub is_open: Option<bool>,
    /// `/State` Text 注解标记状态（文本串；标准值随 `/StateModel` 而变）。
    pub state: Option<String>,
    /// `/StateModel` Text 注解状态模型（文本串，缺省视为 `Marked`）。
    pub state_model: Option<String>,
    /// `/Symbol` Caret 注解符号（名字；标准值 `NewLine`、`Cross`）。
    pub symbol: Option<String>,
    /// `/InkList` Ink 注解路径：每条是 x/y 交替的偶数长度有限数值数组。
    pub ink_paths: Vec<Vec<f64>>,
    /// Line 注解端点 `/L`：`[x1, y1, x2, y2]`（Table 175 必填条目）。
    pub line_endpoints: Option<[f64; 4]>,
    /// 端点样式 `/LE`（Table 176）：Line 与 Polygon/PolyLine 的数组形态，
    /// 缺省视为 `[/None /None]`。
    pub line_endings: Option<[LineEndStyle; 2]>,
    /// Line 注解引线长度 `/LL`（PDF 1.6，缺省 0）。
    pub leader_length: Option<f64>,
    /// Line 注解引线超出端点的延伸长度 `/LLE`（PDF 1.6，需 `/LL`）。
    pub leader_extension: Option<f64>,
    /// Line 注解是否显示题注 `/Cap`（PDF 1.6，缺省 false）。
    pub caption: Option<bool>,
    /// Line 注解意图 `/IT`（PDF 1.6，缺省 `LineArrow`）。
    pub line_intent: Option<LineIntent>,
    /// 题注相对线中点的偏移 `/LLO`（PDF 1.7，仅 `/Cap` 为真时有意义）。
    pub leader_line_offset: Option<f64>,
    /// 题注位置 `/CP`（PDF 1.7，仅 `/Cap` 为真时有意义）。
    pub caption_position: Option<CaptionPosition>,
    /// 题注相对 `/LLO` 位置的偏移 `/CO`（PDF 1.7，缺省 `[0 0]`）。
    pub caption_offset: Option<[f64; 2]>,
    /// FreeText 默认外观串 `/DA`（Table 174 必填；有 `/AP` 时外观流优先），
    /// Redact 叠文字符串 `/DA`（Table 192）同用此字段。
    pub default_appearance: Option<String>,
    /// 文本对齐方式 `/Q`（0 左、1 居中、2 右；FreeText PDF 1.4、
    /// Redact Table 192 共用此键，缺省 0）。
    pub quadding: Option<u8>,
    /// FreeText 富文本默认样式串 `/DS`（PDF 1.5）。
    pub default_style: Option<String>,
    /// FreeText 引线 `/CL`（PDF 1.6，四或六个数值；仅意图为标注框时有意义）。
    pub callout_line: Option<CalloutLine>,
    /// FreeText 意图 `/IT`（PDF 1.6，缺省 `FreeText`）。
    pub free_text_intent: Option<FreeTextIntent>,
    /// 边框效果字典 `/BE`（Table 167；FreeText PDF 1.6，
    /// Square/Circle/Polygon PDF 1.5）。
    pub border_effect: Option<BorderEffect>,
    /// 注解矩形与实际边界的四边差值 `/RD`（FreeText PDF 1.6、
    /// Square/Circle PDF 1.5，均 ≥0）。
    pub text_rectangle: Option<[f64; 4]>,
    /// FreeText 引线端点样式 `/LE`（单个名字，PDF 1.6，需 `/CL`）。
    pub callout_ending: Option<LineEndStyle>,
    /// Polygon/PolyLine 顶点 `/Vertices`（Table 178 必填）：x/y 交替的偶数个数值。
    pub vertices: Option<Vec<f64>>,
    /// Polygon/PolyLine 意图 `/IT`（PDF 1.6 云状、1.7 标注）。
    pub polygon_intent: Option<PolygonIntent>,
    /// Link 高亮模式 `/H`（Table 173，PDF 1.2，缺省 `Invert`）。
    pub link_highlight: Option<LinkHighlightMode>,
    /// Link 按下 URI 动作 `/PA` 原始值（PDF 1.3）；类型化解析见
    /// [`Document::annotation_pressed_action`]。
    pub pressed_action: Option<Value>,
    /// Redact 涂消后叠加文本 `/OverlayText`（Table 192，PDF 1.7）。
    pub overlay_text: Option<String>,
    /// Redact `/Repeat`：叠加文本是否平铺填满涂消区（缺省 false）。
    pub repeat_overlay_text: Option<bool>,
    /// Redact 叠加外观 `/RO` 原值（Form XObject 流或引用）；
    /// 声明它时 `/IC` `/OverlayText` `/DA` `/Q` 按 Table 192 被忽略。
    pub overlay_appearance: Option<Value>,
    /// 外观特性字典 `/MK`（Table 189；规范只声明给 Widget 与 Screen 注解）。
    pub appearance_characteristics: Option<AppearanceCharacteristics>,
    /// Watermark 注解的固定打印字典 `/FixedPrint`（Table 190/191，PDF 1.6）。
    pub fixed_print: Option<FixedPrint>,
    /// Movie 注解的电影字典 `/Movie`（Table 186，PDF 1.2）。
    pub movie: Option<Movie>,
    /// Movie 注解的播放设置 `/A`（布尔或电影激活字典，Table 186）。
    pub movie_playback: Option<MoviePlayback>,
    /// 3D 注解的 artwork `/3DD`（3D 流或 3D 引用字典，Table 298）。
    pub three_d: Option<ThreeDArtwork>,
    /// 3D 注解的初始视图 `/3DV`（Table 298）。
    pub three_d_view: Option<ThreeDViewSelection>,
    /// 3D 注解的激活字典 `/3DA`（Table 299）。
    pub three_d_activation: Option<ThreeDActivation>,
    /// 3D 注解的交互标志 `/3DI`（缺省 true 由呈现端应用）。
    pub three_d_interactive: Option<bool>,
    /// 3D 注解的视图框 `/3DB`（缺省由 `/Rect` 计算）。
    pub three_d_view_box: Option<[f64; 4]>,
    /// `/A` 动作（语义归 E3，原样保留）。
    pub action: Option<Value>,
    /// `/Dest` 原始值（名称/数组/字典）；类型化解析见 `Document::annotation_destination`。
    pub destination: Option<Value>,
    /// `/Popup` 弹出窗口引用。
    pub popup: Option<(i64, i64)>,
    /// `/Parent` 父注解引用（Table 183：Popup 关联的父注解；
    /// Widget 上指向字段字典）。
    pub parent: Option<(i64, i64)>,
    /// `/Measure` 测量字典（12.9；Line 与 Polygon/PolyLine，PDF 1.7）。
    pub measure: Option<Measure>,
    /// 3D 标记注解的外部数据字典 `/ExData`（13.6.6 Table 313，PDF 1.7）。
    pub ex_data: Option<ExternalData>,
    /// 完整注解字典，未建模条目（`/FS`、Widget 字段条目等）经此访问。
    pub dict: Dict,
}

impl Annotation {
    /// 是否应在正常查看时呈现：`/F` 的 Invisible/Hidden/NoView 均未置位。
    pub fn visible(&self) -> bool {
        !(self.flags.invisible() || self.flags.hidden() || self.flags.no_view())
    }
}

impl Document {
    /// 读取指定页面的注解列表（12.5.1 `/Annots`）。
    ///
    /// 页码越界、页面或注解字典不可解析时返回类型化错误。
    pub fn page_annotations(&self, page_index: usize) -> Result<Vec<Annotation>> {
        let page = self.page_dict_at(page_index)?;
        let Some(annots) = page.get("Annots") else {
            return Ok(Vec::new());
        };
        let resolved = crate::core::reader::navigation::resolve(self, annots)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let array = resolved
            .as_array()
            .ok_or_else(|| malformed("page /Annots is not an array"))?;
        if array.len() > MAX_ANNOTATIONS_PER_PAGE {
            return Err(AnnotationError::LimitExceeded("annotations per page"));
        }
        let mut out = Vec::with_capacity(array.len());
        for entry in array.iter() {
            let reference = entry.as_ref();
            let resolved = crate::core::reader::navigation::resolve(self, entry)
                .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
            let dict = resolved
                .as_dict()
                .ok_or_else(|| malformed("annotation entry is not a dictionary"))?;
            out.push(parse_annotation(self, page_index, reference, dict)?);
        }
        Ok(out)
    }

    /// 解析注解 `/Dest` 为目标模型（12.5.6.20 与 12.3.2）。
    ///
    /// 支持目的地数组、名字/文本串（命名目标）与 PDF 1.1 的 `/D` 字典形式；
    /// 无 `/Dest` 返回 `None`。返回 `Named` 时可继续用
    /// [`Document::named_destination`] 在目录 `/Dests` 名称树中查找实际目标。
    pub fn annotation_destination(
        &self,
        annotation: &Annotation,
    ) -> Result<Option<OutlineDestination>> {
        let Some(value) = annotation.destination.as_ref() else {
            return Ok(None);
        };
        let resolved = crate::core::reader::navigation::resolve(self, value)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let value = match &resolved {
            Value::Dict(dict) => dict
                .get("D")
                .ok_or_else(|| malformed("annotation /Dest dictionary is missing /D"))?,
            other => other,
        };
        let pages = crate::core::reader::navigation::page_tree_index(self)?;
        let target = crate::core::reader::navigation::destination::parse_outline_destination(
            self, value, &pages,
        )?;
        Ok(Some(target))
    }

    /// 解析 Link 注解 `/PA` 按下动作（Table 173，PDF 1.3）。
    ///
    /// `/PA` 是 Web Capture 把 URI 动作改写为跳转动作时保留的原 URI 数据；
    /// 无 `/PA` 返回 `None`，动作字典不可解析时返回类型化错误。
    pub fn annotation_pressed_action(&self, annotation: &Annotation) -> Result<Option<Action>> {
        let Some(value) = annotation.pressed_action.as_ref() else {
            return Ok(None);
        };
        Ok(Some(self.action(value)?))
    }

    /// 解析 Sound 注解的 `/Sound` 声音对象（12.5.6.16 Table 185）。
    ///
    /// 无 `/Sound` 返回 `None`；引用的流对象非法或解码失败时返回
    /// [`AnnotationError::Malformed`]。
    pub fn annotation_sound(&self, annotation: &Annotation) -> Result<Option<Sound>> {
        let Some(value) = annotation.sound.as_ref() else {
            return Ok(None);
        };
        self.sound(value)
            .map(Some)
            .map_err(|error| malformed(format!("Sound annotation: {error}")))
    }

    /// 按 0 基页号遍历页面树，返回页面字典（含直接嵌入页）。
    fn page_dict_at(&self, page_index: usize) -> Result<Dict> {
        let catalog = crate::core::reader::navigation::catalog(self)
            .map_err(|_| AnnotationError::CatalogMissing)?;
        let pages = catalog.get("Pages").ok_or(AnnotationError::PageNotFound)?;
        let root_key = pages.as_ref();
        let root = crate::core::reader::navigation::resolve(self, pages)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let root = root.as_dict().ok_or(AnnotationError::PageNotFound)?;

        let mut visited: HashSet<(i64, i64)> = HashSet::new();
        if let Some(key) = root_key {
            visited.insert(key);
        }

        let mut count = 0usize;
        let mut stack = vec![(Value::Dict(root.clone()), 0usize)];
        while let Some((value, depth)) = stack.pop() {
            if depth > MAX_PAGE_DEPTH {
                return Err(AnnotationError::LimitExceeded("page tree depth"));
            }
            let resolved = crate::core::reader::navigation::resolve(self, &value)
                .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
            let node = resolved.as_dict().ok_or(AnnotationError::PageNotFound)?;
            match node.get("Type").and_then(Value::as_name) {
                Some("Page") => {
                    if count == page_index {
                        return Ok(node.clone());
                    }
                    count += 1;
                }
                Some("Pages") => {
                    let mut refs_out: Vec<Value> = Vec::new();
                    if let Some(kids) = node.get("Kids") {
                        let kids = crate::core::reader::navigation::resolve(self, kids)
                            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
                        let kids = kids
                            .as_array()
                            .ok_or_else(|| malformed("page tree /Kids is not an array"))?;
                        refs_out.extend(kids.iter().rev().cloned());
                    }
                    for kid in refs_out {
                        if let Some(key) = kid.as_ref()
                            && !visited.insert(key)
                        {
                            return Err(AnnotationError::Malformed(
                                "page tree contains a reference cycle".to_string(),
                            ));
                        }
                        stack.push((kid, depth + 1));
                    }
                }
                _ => {
                    return Err(AnnotationError::PageNotFound);
                }
            }
        }
        Err(AnnotationError::PageNotFound)
    }
}
