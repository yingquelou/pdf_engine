//! 注解读取（12.5.1–12.5.6）。
//!
//! - 页面 `/Annots` 数组遍历（直接字典与间接引用均支持），深度/数量/环防护。
//! - 通用注解字典条目：`/Rect`、`/Subtype`、`/F` 标志（Table 165）、
//!   `/Contents`、`/T`、`/NM`、`/M`、`/C`、`/CA`、`/AP`/`/AS` 外观流、
//!   `/BS`/`/BM`/`/Border` 边框（12.5.4）。
//! - 标记类注解公共条目 `/QuadPoints`（12.5.6.4）；`/A`、`/Dest` 原样保留，
//!   动作语义由 E3 的 [`crate::core::reader::action`] 解析。
//! - 子类型专有条目类型化（12.5.6）：Text `/Name`、`/Open`、`/State`、
//!   `/StateModel`；Caret `/Symbol`；Ink `/InkList` 路径。
//! - 未建模条目保留在 [`Annotation::dict`] 中（FileSpec、Widget 字段链、
//!   Line 端点、FreeText `/DA` 等）。

use std::collections::HashSet;
use std::fmt;

use crate::core::types::{Dict, Document, Value};

#[cfg(test)]
mod tests;

/// 页面树遍历最大深度。
const MAX_PAGE_DEPTH: usize = 64;
/// 单页注解数量上限。
pub const MAX_ANNOTATIONS_PER_PAGE: usize = 4096;
/// `/InkList` 路径数上限（防护不可信文件）。
const MAX_INK_PATHS: usize = 4096;
/// 单条墨迹路径的坐标数值上限（防护不可信文件）。
const MAX_INK_PATH_VALUES: usize = 8192;

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
    /// `/A` 动作（语义归 E3，原样保留）。
    pub action: Option<Value>,
    /// `/Dest` 目标（名称/数组/字典，原样保留）。
    pub destination: Option<Value>,
    /// `/Popup` 弹出窗口引用。
    pub popup: Option<(i64, i64)>,
    /// 完整注解字典，未建模条目（Line `/L`、FreeText `/DA`、`/FS`、`/Parent` 等）经此访问。
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
        icon_name: parse_name_or_text(dict, "Name")?,
        is_open: match dict.get("Open") {
            None => None,
            Some(Value::Bool(flag)) => Some(*flag),
            Some(_) => return Err(malformed("annotation /Open must be a boolean")),
        },
        state: parse_name_or_text(dict, "State")?,
        state_model: parse_name_or_text(dict, "StateModel")?,
        symbol: parse_name_or_text(dict, "Symbol")?,
        ink_paths: parse_ink_list(dict)?,
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

fn parse_rect(value: Option<&Value>) -> Result<[f64; 4]> {
    let value = value.ok_or_else(|| malformed("annotation has no /Rect"))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("annotation /Rect is not an array"))?;
    if array.len() != 4 {
        return Err(malformed("annotation /Rect must have four numbers"));
    }
    let mut out = [0.0f64; 4];
    for (i, item) in array.iter().enumerate() {
        out[i] = number(item, "annotation /Rect")?;
    }
    Ok(out)
}

fn parse_text_string(dict: &Dict, key: &str) -> Result<Option<Value>> {
    match dict.get(key) {
        None => Ok(None),
        Some(value @ (Value::String(_) | Value::HexString(_))) => Ok(Some(value.clone())),
        Some(_) => Err(malformed(format!(
            "annotation /{key} must be a text string"
        ))),
    }
}

fn parse_number_array(dict: &Dict, key: &str, multiple_of_eight: bool) -> Result<Option<Vec<f64>>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("annotation /{key} is not an array")))?;
    if multiple_of_eight && !array.len().is_multiple_of(8) {
        return Err(malformed(format!(
            "annotation /{key} length must be a multiple of 8"
        )));
    }
    let mut out = Vec::with_capacity(array.len());
    for item in array.iter() {
        out.push(number(item, &format!("annotation /{key}"))?);
    }
    Ok(Some(out))
}

/// 名字或文本串条目（Text 注解 `/Name`/`/State`/`/StateModel`、Caret `/Symbol`）
/// 统一解码为 `String`；文本串按 `/DF`/UTF-16BE/PDF 文本串规则解码。
fn parse_name_or_text(dict: &Dict, key: &str) -> Result<Option<String>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    if let Some(name) = value.as_name() {
        return Ok(Some(name.to_string()));
    }
    let decoded = crate::core::reader::content::text_value(value)
        .ok_or_else(|| malformed(format!("annotation /{key} must be a name or text string")))?;
    Ok(Some(decoded))
}

/// `/InkList`：数组的数组，每条路径为 x/y 交替的偶数个有限数值。
fn parse_ink_list(dict: &Dict) -> Result<Vec<Vec<f64>>> {
    let Some(value) = dict.get("InkList") else {
        return Ok(Vec::new());
    };
    let outer = value
        .as_array()
        .ok_or_else(|| malformed("annotation /InkList is not an array"))?;
    if outer.0.len() > MAX_INK_PATHS {
        return Err(malformed("annotation /InkList has too many paths"));
    }
    let mut paths = Vec::with_capacity(outer.0.len());
    for path in outer.0.iter() {
        let inner = path
            .as_array()
            .ok_or_else(|| malformed("annotation /InkList path is not an array"))?;
        if inner.0.len() % 2 != 0 || inner.0.is_empty() {
            return Err(malformed(
                "annotation /InkList path must hold an even, nonzero number of values",
            ));
        }
        if inner.0.len() > MAX_INK_PATH_VALUES {
            return Err(malformed("annotation /InkList path has too many values"));
        }
        let mut points = Vec::with_capacity(inner.0.len());
        for item in inner.0.iter() {
            points.push(number(item, "annotation /InkList")?);
        }
        paths.push(points);
    }
    Ok(paths)
}

fn parse_opacity(dict: &Dict) -> Result<Option<f64>> {
    let Some(value) = dict.get("CA") else {
        return Ok(None);
    };
    let number = number(value, "annotation /CA")?;
    if !(0.0..=1.0).contains(&number) {
        return Err(malformed("annotation /CA must be within 0.0–1.0"));
    }
    Ok(Some(number))
}

fn parse_border(document: &Document, dict: &Dict) -> Result<Option<AnnotationBorder>> {
    let blend_mode = match dict.get("BM") {
        None => None,
        Some(value) => Some(
            value
                .as_name()
                .ok_or_else(|| malformed("annotation /BM must be a name"))?
                .to_string(),
        ),
    };
    let border = if let Some(bs) = dict.get("BS") {
        let resolved = crate::core::reader::navigation::resolve(document, bs)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let bs = resolved
            .as_dict()
            .ok_or_else(|| malformed("annotation /BS is not a dictionary"))?;
        let width = match bs.get("W") {
            None => None,
            Some(value) => {
                let width = number(value, "annotation /BS /W")?;
                if width < 0.0 {
                    return Err(malformed("annotation /BS /W must be nonnegative"));
                }
                Some(width)
            }
        };
        let style = match bs.get("S") {
            None => None,
            Some(value) => Some(
                value
                    .as_name()
                    .ok_or_else(|| malformed("annotation /BS /S must be a name"))?
                    .to_string(),
            ),
        };
        let dash = match bs.get("D") {
            None => Vec::new(),
            Some(value) => {
                let array = value
                    .as_array()
                    .ok_or_else(|| malformed("annotation /BS /D is not an array"))?;
                array
                    .iter()
                    .map(|item| number(item, "annotation /BS /D"))
                    .collect::<Result<Vec<f64>>>()?
            }
        };
        Some(AnnotationBorder {
            width,
            style,
            dash,
            corner_radius: None,
            blend_mode: blend_mode.clone(),
        })
    } else if let Some(legacy) = dict.get("Border") {
        let array = legacy
            .as_array()
            .ok_or_else(|| malformed("annotation /Border is not an array"))?;
        if array.len() > 3 {
            return Err(malformed("annotation /Border array exceeds 3 items"));
        }
        let corner = array.0.first().map(|v| number(v, "annotation /Border"));
        let width = array.0.get(1).map(|v| number(v, "annotation /Border"));
        let dash = match array.0.get(2) {
            // 规范允许 [h w] 或 [h w [dash…]]；真实文档常见第三项写成数字
            // （如 `/Border [0 0 0]`），此处宽容忽略而不报错。
            Some(dash) => match dash.as_array() {
                Some(dash) => dash
                    .iter()
                    .map(|item| number(item, "annotation /Border dash"))
                    .collect::<Result<Vec<f64>>>()?,
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        Some(AnnotationBorder {
            width: width.transpose()?,
            style: None,
            dash,
            corner_radius: corner.transpose()?,
            blend_mode: blend_mode.clone(),
        })
    } else {
        return Ok(blend_mode.map(|mode| AnnotationBorder {
            blend_mode: Some(mode),
            ..Default::default()
        }));
    };
    Ok(border)
}

fn number(value: &Value, context: &str) -> Result<f64> {
    let number = match value {
        Value::Float(number) => *number,
        Value::Integer(number) => *number as f64,
        _ => return Err(malformed(format!("{context} must be a number"))),
    };
    if !number.is_finite() {
        return Err(malformed(format!("{context} has a nonfinite number")));
    }
    Ok(number)
}

fn parse_appearance(document: &Document, dict: &Dict) -> Result<Option<AppearanceStreams>> {
    let Some(ap) = dict.get("AP") else {
        return Ok(None);
    };
    let resolved = crate::core::reader::navigation::resolve(document, ap)
        .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
    let ap = resolved
        .as_dict()
        .ok_or_else(|| malformed("annotation /AP is not a dictionary"))?;
    let streams = AppearanceStreams {
        normal: ap.get("N").cloned(),
        down: ap.get("D").cloned(),
        rollover: ap.get("R").cloned(),
    };
    if streams.is_empty() {
        return Ok(None);
    }
    Ok(Some(streams))
}
