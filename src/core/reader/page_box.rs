//! 页面边界与框颜色信息（ISO 32000-1 14.11.2、Table 30 页面条目与
//! Table 360/361 框颜色信息/框样式字典）。
//!
//! 页面字典声明四种页面边界：`/MediaBox`（必填、可继承）、`/CropBox`
//! （可选、可继承，缺省等于 MediaBox）、`/BleedBox` `/TrimBox` `/ArtBox`
//! （PDF 1.3，可选，**不可继承**，缺省等于 CropBox）。本模块只报告页面
//! 实际声明的值，不物化缺省值；缺省链与 14.11.2.1 的「有效框 = 声明框
//! 与 MediaBox 的交集」由 [`PageBounds::effective_crop`] 等有效值方法
//! 给出。
//!
//! `/BoxColorInfo`（PDF 1.4，Table 360）为每个框声明显示样式（Table 361
//! 框样式字典）。按 Table 360 注，页面对象未定义对应框时该条目被忽略；
//! 读端不做对应性强制，只按字典结构解析。`/S` 的扩展样式名原样保留。

use std::collections::HashSet;
use std::fmt;

use crate::core::reader::navigation::{MAX_TREE_DEPTH, resolve};
use crate::core::reader::page_content::PageContentError;
use crate::core::types::{Dict, Document, Value};

/// 页面矩形（`[llx lly urx ury]`）。
///
/// 规范 7.9.5 允许矩形数组以任意对角线顺序给出坐标，构造时统一按
/// min/max 归一化为左下/右上角。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectangle {
    /// 左下角 x。
    pub llx: f64,
    /// 左下角 y。
    pub lly: f64,
    /// 右上角 x。
    pub urx: f64,
    /// 右上角 y。
    pub ury: f64,
}

impl Rectangle {
    /// 按任意对角顺序构造矩形（坐标自动归一化）。
    pub fn new(llx: f64, lly: f64, urx: f64, ury: f64) -> Self {
        Self {
            llx: llx.min(urx),
            lly: lly.min(ury),
            urx: llx.max(urx),
            ury: lly.max(ury),
        }
    }

    /// 宽度（非负）。
    pub fn width(&self) -> f64 {
        self.urx - self.llx
    }

    /// 高度（非负）。
    pub fn height(&self) -> f64 {
        self.ury - self.lly
    }

    /// 两矩形交集。
    ///
    /// 无重叠时返回退化的零宽/零高矩形：下界钳制在交集上界（最近边界）
    /// 上，保证结果仍在两矩形范围内且 `width()` / `height()` 非负。
    pub fn intersect(self, other: Rectangle) -> Rectangle {
        let urx = self.urx.min(other.urx);
        let ury = self.ury.min(other.ury);
        let llx = self.llx.max(other.llx).min(urx);
        let lly = self.lly.max(other.lly).min(ury);
        Rectangle { llx, lly, urx, ury }
    }
}

/// 框样式 `/S`（Table 361）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxStyleName {
    /// `S`：实线。
    Solid,
    /// `D`：虚线。
    Dashed,
    /// 扩展定义样式名（Table 361 允许未来定义），原样保留。
    Other(String),
}

impl BoxStyleName {
    /// 按 `/S` 名称解析；未知名称保留为 [`BoxStyleName::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "S" => Self::Solid,
            "D" => Self::Dashed,
            other => Self::Other(other.to_owned()),
        }
    }

    /// `/S` 名称（标准名称或扩展原文）。
    pub fn name(&self) -> &str {
        match self {
            Self::Solid => "S",
            Self::Dashed => "D",
            Self::Other(name) => name,
        }
    }
}

/// 框样式字典（Table 361）。
///
/// 各字段对应表内声明条目；缺省值不物化（`/C` 缺省 `[0 0 0]`、`/W`
/// 缺省 1、`/S` 缺省 `S`、`/D` 缺省 `[3]`）。`dict` 保留未建模条目。
#[derive(Debug, Clone, PartialEq)]
pub struct BoxStyle {
    /// `/C`：3 个 0.0–1.0 的 DeviceRGB 分量。
    pub color: Option<[f64; 3]>,
    /// `/W`：导线宽度（默认用户空间单位）。
    pub width: Option<f64>,
    /// `/S`：样式名称。
    pub style: Option<BoxStyleName>,
    /// `/D`：虚线数组（默认用户空间单位，同 8.4.3.6 线 dash 格式，相位 0）。
    pub dash: Option<Vec<f64>>,
    /// 原始样式字典（保留未建模条目）。
    pub dict: Dict,
}

impl BoxStyle {
    /// 空样式（全部条目取表内缺省）。
    pub fn new() -> Self {
        Self {
            color: None,
            width: None,
            style: None,
            dash: None,
            dict: Dict::new(),
        }
    }

    /// 设置 `/C`。
    pub fn with_color(mut self, color: [f64; 3]) -> Self {
        self.color = Some(color);
        self
    }

    /// 设置 `/W`。
    pub fn with_width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    /// 设置 `/S`。
    pub fn with_style(mut self, style: BoxStyleName) -> Self {
        self.style = Some(style);
        self
    }

    /// 设置 `/D`。
    pub fn with_dash(mut self, dash: Vec<f64>) -> Self {
        self.dash = Some(dash);
        self
    }
}

impl Default for BoxStyle {
    fn default() -> Self {
        Self::new()
    }
}

/// 框颜色信息字典 `/BoxColorInfo`（Table 360）。
///
/// 四个框样式条目均可选；页面对象未定义对应框时该条目被忽略（Table 360
/// 注），读端不强制。
#[derive(Debug, Clone, PartialEq)]
pub struct BoxColorInfo {
    /// `/CropBox` 框样式。
    pub crop: Option<BoxStyle>,
    /// `/BleedBox` 框样式。
    pub bleed: Option<BoxStyle>,
    /// `/TrimBox` 框样式。
    pub trim: Option<BoxStyle>,
    /// `/ArtBox` 框样式。
    pub art: Option<BoxStyle>,
    /// 原始条目字典（保留未建模条目）。
    pub dict: Dict,
}

impl BoxColorInfo {
    /// 空框颜色信息（无任何框样式）。
    pub fn new() -> Self {
        Self {
            crop: None,
            bleed: None,
            trim: None,
            art: None,
            dict: Dict::new(),
        }
    }

    /// 设置 `/CropBox` 框样式。
    pub fn with_crop(mut self, style: BoxStyle) -> Self {
        self.crop = Some(style);
        self
    }

    /// 设置 `/BleedBox` 框样式。
    pub fn with_bleed(mut self, style: BoxStyle) -> Self {
        self.bleed = Some(style);
        self
    }

    /// 设置 `/TrimBox` 框样式。
    pub fn with_trim(mut self, style: BoxStyle) -> Self {
        self.trim = Some(style);
        self
    }

    /// 设置 `/ArtBox` 框样式。
    pub fn with_art(mut self, style: BoxStyle) -> Self {
        self.art = Some(style);
        self
    }
}

impl Default for BoxColorInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// 一页的边界声明（14.11.2 Table 30）。
///
/// 只含页面实际声明的值；仅 `/MediaBox` / `/CropBox` 沿 `/Parent` 链
/// 继承，`/BleedBox` `/TrimBox` `/ArtBox` 只取页面字典自身。
#[derive(Debug, Clone, PartialEq)]
pub struct PageBounds {
    /// `/MediaBox`：介质框（必填、可继承）。
    pub media: Rectangle,
    /// `/CropBox`：裁剪框（可继承，缺省等于 MediaBox）。
    pub crop: Option<Rectangle>,
    /// `/BleedBox`：出血框（PDF 1.3，不可继承，缺省等于 CropBox）。
    pub bleed: Option<Rectangle>,
    /// `/TrimBox`：裁切框（PDF 1.3，不可继承，缺省等于 CropBox）。
    pub trim: Option<Rectangle>,
    /// `/ArtBox`：作品框（PDF 1.3，不可继承，缺省等于 CropBox）。
    pub art: Option<Rectangle>,
    /// `/BoxColorInfo`：框颜色信息（PDF 1.4）。
    pub box_color_info: Option<BoxColorInfo>,
    /// 原始页面字典（保留未建模条目）。
    pub dict: Dict,
}

impl PageBounds {
    /// 有效 CropBox：声明值或 MediaBox，再与 MediaBox 求交（14.11.2.1）。
    pub fn effective_crop(&self) -> Rectangle {
        self.crop.unwrap_or(self.media).intersect(self.media)
    }

    /// 有效 BleedBox：声明值或 CropBox，再与 MediaBox 求交。
    pub fn effective_bleed(&self) -> Rectangle {
        self.bleed
            .unwrap_or_else(|| self.effective_crop())
            .intersect(self.media)
    }

    /// 有效 TrimBox：声明值或 CropBox，再与 MediaBox 求交。
    pub fn effective_trim(&self) -> Rectangle {
        self.trim
            .unwrap_or_else(|| self.effective_crop())
            .intersect(self.media)
    }

    /// 有效 ArtBox：声明值或 CropBox，再与 MediaBox 求交。
    pub fn effective_art(&self) -> Rectangle {
        self.art
            .unwrap_or_else(|| self.effective_crop())
            .intersect(self.media)
    }
}

/// 页面边界读取的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum PageBoxError {
    /// 页面节点或页面树无法解析（含页码越界）。
    Page(PageContentError),
    /// 条目缺失、类型错误或取值非法。
    Malformed(String),
    /// 超出 `/Parent` 链继承深度上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for PageBoxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(error) => write!(f, "invalid page box: {error}"),
            Self::Malformed(message) => write!(f, "invalid page box: {message}"),
            Self::LimitExceeded(what) => write!(f, "invalid page box: limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for PageBoxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Page(error) => Some(error),
            Self::Malformed(_) | Self::LimitExceeded(_) => None,
        }
    }
}

impl From<PageContentError> for PageBoxError {
    fn from(error: PageContentError) -> Self {
        Self::Page(error)
    }
}

fn malformed(message: impl Into<String>) -> PageBoxError {
    PageBoxError::Malformed(message.into())
}

impl Document {
    /// 读取 `page_index`（0 基）页面的边界声明（14.11.2 Table 30）。
    ///
    /// `/MediaBox` 沿 `/Parent` 链继承（深度上限 64，引用成环报错），
    /// 整条链都缺失时为 [`PageBoxError::Malformed`]；`/CropBox` 取最近
    /// 声明者；`/BleedBox` `/TrimBox` `/ArtBox` 只取页面字典自身。
    /// 各框须为 4 个有限数字的数组（7.9.5，任意对角顺序均归一化）。
    /// `/BoxColorInfo` 须为字典，其条目按 Table 360/361 解析。
    pub fn page_bounds(&self, page_index: usize) -> Result<PageBounds, PageBoxError> {
        let page_value = self.locate_page_node(page_index)?;
        let page = page_value
            .as_dict()
            .ok_or_else(|| malformed("page node is not a dictionary"))?;

        let media = inherited_box(self, page, "MediaBox")?
            .ok_or_else(|| malformed("page has no inheritable /MediaBox"))?;
        let crop = inherited_box(self, page, "CropBox")?;
        let bleed = page_box(self, page, "BleedBox")?;
        let trim = page_box(self, page, "TrimBox")?;
        let art = page_box(self, page, "ArtBox")?;
        let box_color_info = match page.get("BoxColorInfo") {
            Some(value) => Some(parse_box_color_info(self, value)?),
            None => None,
        };

        Ok(PageBounds {
            media,
            crop,
            bleed,
            trim,
            art,
            box_color_info,
            dict: page.clone(),
        })
    }
}

/// 沿页面字典与 `/Parent` 链查找可继承的框（MediaBox/CropBox）。
fn inherited_box(
    document: &Document,
    page: &Dict,
    key: &str,
) -> Result<Option<Rectangle>, PageBoxError> {
    let mut current = page.clone();
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    let mut depth = 0usize;
    loop {
        if let Some(value) = current.get(key) {
            return parse_rectangle(document, value, &format!("page /{key}")).map(Some);
        }
        if depth > MAX_TREE_DEPTH {
            return Err(PageBoxError::LimitExceeded("page boundary inheritance"));
        }
        let Some(parent) = current.get("Parent") else {
            return Ok(None);
        };
        if let Some(reference) = parent.as_ref()
            && !visited.insert(reference)
        {
            return Err(malformed(format!(
                "page /Parent chain contains a cycle while resolving /{key}"
            )));
        }
        let parent = resolve(document, parent)
            .map_err(|error| malformed(format!("page /Parent: {error}")))?;
        current = parent
            .as_dict()
            .cloned()
            .ok_or_else(|| malformed("page /Parent is not a dictionary"))?;
        depth += 1;
    }
}

/// 读取页面字典自身（不继承）的框。
fn page_box(
    document: &Document,
    page: &Dict,
    key: &str,
) -> Result<Option<Rectangle>, PageBoxError> {
    match page.get(key) {
        Some(value) => parse_rectangle(document, value, &format!("page /{key}")).map(Some),
        None => Ok(None),
    }
}

/// 解析矩形数组（解引用后须为恰好 4 个有限数字，任意对角顺序）。
fn parse_rectangle(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Rectangle, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("{context} is not an array")))?;
    if array.len() != 4 {
        return Err(malformed(format!(
            "{context} must have exactly 4 numbers, got {}",
            array.len()
        )));
    }
    let mut coordinates = [0.0f64; 4];
    for (index, item) in array.iter().enumerate() {
        coordinates[index] = match item {
            Value::Integer(number) => *number as f64,
            Value::Float(number) if number.is_finite() => *number,
            Value::Float(_) => {
                return Err(malformed(format!(
                    "{context} coordinate {index} is not finite"
                )));
            }
            other => {
                return Err(malformed(format!(
                    "{context} coordinate {index} is not a number ({})",
                    other.type_name()
                )));
            }
        };
    }
    Ok(Rectangle::new(
        coordinates[0],
        coordinates[1],
        coordinates[2],
        coordinates[3],
    ))
}

/// 解析 `/BoxColorInfo`（Table 360），条目本身须为字典。
fn parse_box_color_info(document: &Document, value: &Value) -> Result<BoxColorInfo, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("/BoxColorInfo: {error}")))?;
    let dict = value
        .as_dict()
        .ok_or_else(|| malformed("/BoxColorInfo is not a dictionary"))?
        .clone();
    let crop = optional_style(document, &dict, "CropBox")?;
    let bleed = optional_style(document, &dict, "BleedBox")?;
    let trim = optional_style(document, &dict, "TrimBox")?;
    let art = optional_style(document, &dict, "ArtBox")?;
    Ok(BoxColorInfo {
        crop,
        bleed,
        trim,
        art,
        dict,
    })
}

/// 解析 `/BoxColorInfo` 的一个框样式条目。
fn optional_style(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<BoxStyle>, PageBoxError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let context = format!("/BoxColorInfo /{key}");
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    let style = value
        .as_dict()
        .ok_or_else(|| malformed(format!("{context} is not a dictionary")))?
        .clone();
    parse_box_style(document, style, &context).map(Some)
}

/// 解析框样式字典（Table 361）。
///
/// `/C` 须为 3 个 0.0–1.0 的有限数字；`/W` 须有限；`/S` 须为名称（未知
/// 名称保留）；`/D` 须为非空、非负有限数字且不全为零的数组。Table 361
/// 限定 `/D` 只与 `/S /D` 同用，本读端不强制（宽松读取，写端强制）。
fn parse_box_style(
    document: &Document,
    dict: Dict,
    context: &str,
) -> Result<BoxStyle, PageBoxError> {
    let color = match dict.get("C") {
        Some(value) => Some(parse_color(document, value, context)?),
        None => None,
    };
    let width = match dict.get("W") {
        Some(value) => {
            let number = number_value(document, value, &format!("{context} /W"))?;
            if !number.is_finite() {
                return Err(malformed(format!("{context} /W is not finite")));
            }
            Some(number)
        }
        None => None,
    };
    let style = match dict.get("S") {
        Some(value) => {
            let value = resolve(document, value)
                .map_err(|error| malformed(format!("{context} /S: {error}")))?;
            let name = value
                .as_name()
                .ok_or_else(|| malformed(format!("{context} /S is not a name")))?;
            Some(BoxStyleName::from_name(name))
        }
        None => None,
    };
    let dash = match dict.get("D") {
        Some(value) => Some(parse_dash(document, value, context)?),
        None => None,
    };
    Ok(BoxStyle {
        color,
        width,
        style,
        dash,
        dict,
    })
}

/// 解析 `/C`：恰好 3 个 0.0–1.0 的有限数字。
fn parse_color(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[f64; 3], PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context} /C: {error}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("{context} /C is not an array")))?;
    if array.len() != 3 {
        return Err(malformed(format!(
            "{context} /C must have exactly 3 numbers, got {}",
            array.len()
        )));
    }
    let mut color = [0.0f64; 3];
    for (index, item) in array.iter().enumerate() {
        let component = number_value(document, item, &format!("{context} /C"))?;
        if !component.is_finite() {
            return Err(malformed(format!(
                "{context} /C component {index} is not finite"
            )));
        }
        if !(0.0..=1.0).contains(&component) {
            return Err(malformed(format!(
                "{context} /C component {index} must be between 0 and 1, got {component}"
            )));
        }
        color[index] = component;
    }
    Ok(color)
}

/// 解析 `/D`：非空、非负有限数字且不全为零（8.4.3.6）。
fn parse_dash(document: &Document, value: &Value, context: &str) -> Result<Vec<f64>, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context} /D: {error}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("{context} /D is not an array")))?;
    if array.is_empty() {
        return Err(malformed(format!("{context} /D must not be empty")));
    }
    let mut dash = Vec::with_capacity(array.len());
    for (index, item) in array.iter().enumerate() {
        let number = number_value(document, item, &format!("{context} /D"))?;
        if !number.is_finite() || number < 0.0 {
            return Err(malformed(format!(
                "{context} /D element {index} must be a non-negative finite number"
            )));
        }
        dash.push(number);
    }
    if dash.iter().all(|number| *number == 0.0) {
        return Err(malformed(format!("{context} /D must not be all zero")));
    }
    Ok(dash)
}

/// 取数字值（整数或浮点）。
fn number_value(document: &Document, value: &Value, context: &str) -> Result<f64, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    match value {
        Value::Integer(number) => Ok(number as f64),
        Value::Float(number) => Ok(number),
        other => Err(malformed(format!(
            "{context} is not a number ({})",
            other.type_name()
        ))),
    }
}

#[cfg(test)]
mod tests;
