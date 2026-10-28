//! 注解读取（12.5.1–12.5.6）。
//!
//! - 页面 `/Annots` 数组遍历（直接字典与间接引用均支持），深度/数量/环防护。
//! - 通用注解字典条目：`/Rect`、`/Subtype`、`/F` 标志（Table 165）、
//!   `/Contents`、`/T`、`/NM`、`/M`、`/C`、`/CA`、`/AP`/`/AS` 外观流、
//!   `/BS`/`/BM`/`/Border` 边框（12.5.4）。
//! - 标记类注解公共条目 `/QuadPoints`（12.5.6.4）；`/A` 原样保留、动作语义由
//!   E3 的 [`crate::core::reader::action`] 解析，`/Dest` 经
//!   `Document::annotation_destination` 解析为 12.3.2 目标模型。
//! - 子类型专有条目类型化（12.5.6，解析在 [`subtype`]）：Text `/Name`、`/Open`、
//!   `/State`、`/StateModel`；Caret `/Symbol`；Ink `/InkList` 路径；
//!   Line `/L`、`/LE`、`/LL`、`/LLE`、`/Cap`、`/IT`、`/LLO`、`/CP`、`/CO`
//!   （Table 175/176，端点样式与意图、题注位置类型化为枚举）。
//! - 未建模条目保留在 [`Annotation::dict`] 中（FileSpec、Widget 字段链、
//!   Polygon `/Vertices`、FreeText `/DA` 等）。
//!
//! 通用条目解析在 `entries`，子类型专有条目解析在 `subtype`。

use std::collections::HashSet;
use std::fmt;

use crate::core::reader::navigation::{NavigationError, OutlineDestination};
use crate::core::types::{Dict, Document, Value};

#[cfg(test)]
mod tests;

mod entries;
pub mod subtype;

use entries::{
    parse_appearance, parse_border, parse_number_array, parse_opacity, parse_rect,
    parse_text_string,
};
pub use subtype::{CaptionPosition, LineEndStyle, LineIntent};

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

type Result<T> = std::result::Result<T, AnnotationError>;

fn malformed(message: impl Into<String>) -> AnnotationError {
    AnnotationError::Malformed(message.into())
}

/// 注解子类型（12.5.3 Table 169；PDF 1.7 全集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationSubtype {
    Text,
    Link,
    FreeText,
    Line,
    Square,
    Circle,
    Polygon,
    PolyLine,
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
    Caret,
    Ink,
    Popup,
    FileAttachment,
    Sound,
    Movie,
    Widget,
    Screen,
    Stamp,
    Redaction,
    /// 未识别的子类型名，原样保留。
    Unknown(String),
}

impl AnnotationSubtype {
    fn from_name(name: &str) -> Self {
        match name {
            "Text" => Self::Text,
            "Link" => Self::Link,
            "FreeText" => Self::FreeText,
            "Line" => Self::Line,
            "Square" => Self::Square,
            "Circle" => Self::Circle,
            "Polygon" => Self::Polygon,
            "PolyLine" => Self::PolyLine,
            "Highlight" => Self::Highlight,
            "Underline" => Self::Underline,
            "Squiggly" => Self::Squiggly,
            "StrikeOut" => Self::StrikeOut,
            "Caret" => Self::Caret,
            "Ink" => Self::Ink,
            "Popup" => Self::Popup,
            "FileAttachment" => Self::FileAttachment,
            "Sound" => Self::Sound,
            "Movie" => Self::Movie,
            "Widget" => Self::Widget,
            "Screen" => Self::Screen,
            "Stamp" => Self::Stamp,
            "Redaction" => Self::Redaction,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 标准子类型名（Unknown 返回其原始名）。
    pub fn name(&self) -> &str {
        match self {
            Self::Text => "Text",
            Self::Link => "Link",
            Self::FreeText => "FreeText",
            Self::Line => "Line",
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Polygon => "Polygon",
            Self::PolyLine => "PolyLine",
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::Squiggly => "Squiggly",
            Self::StrikeOut => "StrikeOut",
            Self::Caret => "Caret",
            Self::Ink => "Ink",
            Self::Popup => "Popup",
            Self::FileAttachment => "FileAttachment",
            Self::Sound => "Sound",
            Self::Movie => "Movie",
            Self::Widget => "Widget",
            Self::Screen => "Screen",
            Self::Stamp => "Stamp",
            Self::Redaction => "Redaction",
            Self::Unknown(other) => other,
        }
    }

    /// 是否为标记类注解（Table 172，携带 `/QuadPoints`）。
    pub fn is_markup(&self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Highlight
                | Self::Underline
                | Self::Squiggly
                | Self::StrikeOut
                | Self::Caret
                | Self::FreeText
                | Self::Square
                | Self::Circle
                | Self::Polygon
                | Self::PolyLine
                | Self::Line
                | Self::Ink
                | Self::Stamp
                | Self::Redaction
        )
    }
}

/// 注解标志位（12.5.2 Table 165）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnnotationFlags(u32);

impl AnnotationFlags {
    pub const INVISIBLE: u32 = 1 << 0;
    pub const HIDDEN: u32 = 1 << 1;
    pub const PRINT: u32 = 1 << 2;
    pub const NO_ZOOM: u32 = 1 << 3;
    pub const NO_ROTATE: u32 = 1 << 4;
    pub const NO_VIEW: u32 = 1 << 5;
    pub const READ_ONLY: u32 = 1 << 6;
    pub const LOCKED: u32 = 1 << 7;
    pub const TOGGLE_NO_VIEW: u32 = 1 << 8;
    pub const LOCKED_CONTENTS: u32 = 1 << 9;

    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    pub fn invisible(self) -> bool {
        self.has(Self::INVISIBLE)
    }

    pub fn hidden(self) -> bool {
        self.has(Self::HIDDEN)
    }

    pub fn print(self) -> bool {
        self.has(Self::PRINT)
    }

    pub fn no_zoom(self) -> bool {
        self.has(Self::NO_ZOOM)
    }

    pub fn no_rotate(self) -> bool {
        self.has(Self::NO_ROTATE)
    }

    pub fn no_view(self) -> bool {
        self.has(Self::NO_VIEW)
    }

    pub fn read_only(self) -> bool {
        self.has(Self::READ_ONLY)
    }

    pub fn locked(self) -> bool {
        self.has(Self::LOCKED)
    }

    pub fn toggle_no_view(self) -> bool {
        self.has(Self::TOGGLE_NO_VIEW)
    }

    pub fn locked_contents(self) -> bool {
        self.has(Self::LOCKED_CONTENTS)
    }
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
    /// `/BS /W` 或 `/Border` 第二项（宽度）。
    pub width: Option<f64>,
    /// `/BS /S`：Solid/Beveled/Dashed/Inset/Underline。
    pub style: Option<String>,
    /// `/BS /D` 或 `/Border` 第三项的虚线数组。
    pub dash: Vec<f64>,
    /// `/BM` 混合模式名。
    pub blend_mode: Option<String>,
    /// 旧式 `/Border` 第一项（水平圆角半径），`/BS` 路径下为 `None`。
    pub corner_radius: Option<f64>,
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
    /// `/QuadPoints`（标记类注解，长度为 8 的倍数）。
    pub quad_points: Option<Vec<f64>>,
    /// `/Name` 图标名（Text 注解专有条目；名字或文本串，取值由查看器定义）。
    pub icon_name: Option<String>,
    /// `/Open` Text 注解气泡是否展开。
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
    /// Line 注解两端端点样式 `/LE`（Table 176，PDF 1.4；缺省视为 `[/None /None]`）。
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
    /// `/A` 动作（语义归 E3，原样保留）。
    pub action: Option<Value>,
    /// `/Dest` 原始值（名称/数组/字典）；类型化解析见 `Document::annotation_destination`。
    pub destination: Option<Value>,
    /// `/Popup` 弹出窗口引用。
    pub popup: Option<(i64, i64)>,
    /// 完整注解字典，未建模条目（FreeText `/DA`、Polygon `/Vertices`、`/FS`、`/Parent` 等）经此访问。
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

fn parse_annotation(
    document: &Document,
    page_index: usize,
    reference: Option<(i64, i64)>,
    dict: &Dict,
) -> Result<Annotation> {
    match dict.get("Type").and_then(Value::as_name) {
        Some("Annot") => {}
        Some(other) => {
            return Err(malformed(format!(
                "annotation /Type must be Annot, found {other}"
            )));
        }
        None => return Err(malformed("annotation dictionary has no /Type")),
    }
    let subtype = AnnotationSubtype::from_name(
        dict.get("Subtype")
            .and_then(Value::as_name)
            .ok_or_else(|| malformed("annotation has no /Subtype"))?,
    );
    let rect = parse_rect(dict.get("Rect"))?;
    let flags = match dict.get("F") {
        Some(value) => match value {
            Value::Integer(bits) if *bits >= 0 && *bits <= u32::MAX as i64 => {
                AnnotationFlags::from_bits(*bits as u32)
            }
            _ => return Err(malformed("annotation /F is not a nonnegative integer")),
        },
        None => AnnotationFlags::default(),
    };
    let entries = subtype::parse_subtype_entries(dict)?;

    Ok(Annotation {
        page_index,
        reference,
        subtype,
        rect,
        flags,
        contents: parse_text_string(dict, "Contents")?,
        rich_contents: parse_text_string(dict, "RC")?,
        title: parse_text_string(dict, "T")?,
        name: parse_text_string(dict, "NM")?,
        modified_date: parse_text_string(dict, "M")?,
        creation_date: parse_text_string(dict, "CreationDate")?,
        color: parse_number_array(dict, "C", false)?,
        interior_color: parse_number_array(dict, "IC", false)?,
        opacity: parse_opacity(dict)?,
        border: parse_border(document, dict)?,
        appearance: parse_appearance(document, dict)?,
        appearance_state: dict
            .get("AS")
            .map(|value| match value.as_name() {
                Some(name) => Ok(name.to_string()),
                None => Err(malformed("annotation /AS must be a name")),
            })
            .transpose()?,
        quad_points: parse_number_array(dict, "QuadPoints", true)?,
        icon_name: entries.icon_name,
        is_open: entries.is_open,
        state: entries.state,
        state_model: entries.state_model,
        symbol: entries.symbol,
        ink_paths: entries.ink_paths,
        line_endpoints: entries.line_endpoints,
        line_endings: entries.line_endings,
        leader_length: entries.leader_length,
        leader_extension: entries.leader_extension,
        caption: entries.caption,
        line_intent: entries.line_intent,
        leader_line_offset: entries.leader_line_offset,
        caption_position: entries.caption_position,
        caption_offset: entries.caption_offset,
        action: dict.get("A").cloned(),
        destination: dict.get("Dest").cloned(),
        popup: match dict.get("Popup") {
            None => None,
            Some(value) => Some(
                value
                    .as_ref()
                    .ok_or_else(|| malformed("annotation /Popup must be an indirect reference"))?,
            ),
        },
        dict: dict.clone(),
    })
}
