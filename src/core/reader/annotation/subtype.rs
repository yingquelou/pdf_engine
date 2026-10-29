//! 注解子类型专有条目解析（12.5.6）。
//!
//! 通用条目由父模块解析；此处只按各子类型的表格提取专有条目，
//! 读端保持宽容：条目存在即解析，不校验声明的 `/Subtype`
//! （归属校验由写端 `PdfAnnotation::validate` 负责），非标准的端点样式、
//! 意图与题注位置名一律落成 `Other` 变体保留原文。
//!
//! - Text（Table 172）：`/Name`、`/Open`、`/State`、`/StateModel`
//! - Caret（Table 180）：`/Symbol`
//! - Ink（Table 182）：`/InkList` 路径
//! - Line（Table 175/176）：`/L`、`/LE`、`/LL`、`/LLE`、`/Cap`、`/IT`、
//!   `/LLO`、`/CP`、`/CO`（`/IC` 属通用条目，见 `Annotation::interior_color`）
//! - FreeText（Table 174 + Table 167）：`/DA`、`/Q`、`/DS`、`/CL`、`/IT`、
//!   `/BE`、`/RD`、`/LE`（`/RC` 属通用条目，`/BS` 属 12.5.4 边框）
//!
//! `/LE` 有两种形态：Line（Table 175）是 1–2 个名字的数组，FreeText
//! （Table 174）是单个名字。解析不校验声明的 `/Subtype`，因此数组形态
//! 只填 [`SubtypeEntries::line_endings`]、裸名字只填
//! [`SubtypeEntries::callout_ending`]，两者互不报错。
//!
//! `/IT` 也归 Line 与 FreeText 两家，名字集合互不相交：命中哪一家就填哪
//! 一家；两家都不认识的名字才按声明的 `/Subtype` 归属决定落进哪个 `Other`
//! （非 FreeText 仍按 Line 的 `Other` 保留原文）。

use super::{
    Result,
    entries::{number, parse_number_array, parse_optional_bool, parse_optional_number},
    malformed,
};
use crate::core::types::{Dict, Value};

/// `/InkList` 路径数上限（防护不可信文件）。
const MAX_INK_PATHS: usize = 4096;
/// 单条墨迹路径的坐标数值上限（防护不可信文件）。
const MAX_INK_PATH_VALUES: usize = 8192;

/// Line 注解端点样式（`/LE` 数组元素，Table 176）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEndStyle {
    /// 方头（`/Square`）。
    Square,
    /// 圆头（`/Circle`）。
    Circle,
    /// 菱形（`/Diamond`）。
    Diamond,
    /// 开口箭头（`/OpenArrow`）。
    OpenArrow,
    /// 闭合箭头（`/ClosedArrow`）。
    ClosedArrow,
    /// 无端点（`/None`，Table 175 缺省）。
    None,
    /// 平头（`/Butt`，PDF 1.5）。
    Butt,
    /// 末端反向开口箭头（`/ROpenArrow`，PDF 1.5）。
    ROpenArrow,
    /// 末端反向闭合箭头（`/RClosedArrow`，PDF 1.5）。
    RClosedArrow,
    /// 斜线（`/Slash`，PDF 1.6）。
    Slash,
    /// 非标准名（原样保留）。
    Other(String),
}

impl LineEndStyle {
    /// 由 `/LE` 名字解析；未知名返回 [`LineEndStyle::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Square" => Self::Square,
            "Circle" => Self::Circle,
            "Diamond" => Self::Diamond,
            "OpenArrow" => Self::OpenArrow,
            "ClosedArrow" => Self::ClosedArrow,
            "None" => Self::None,
            "Butt" => Self::Butt,
            "ROpenArrow" => Self::ROpenArrow,
            "RClosedArrow" => Self::RClosedArrow,
            "Slash" => Self::Slash,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准样式名；[`LineEndStyle::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Diamond => "Diamond",
            Self::OpenArrow => "OpenArrow",
            Self::ClosedArrow => "ClosedArrow",
            Self::None => "None",
            Self::Butt => "Butt",
            Self::ROpenArrow => "ROpenArrow",
            Self::RClosedArrow => "RClosedArrow",
            Self::Slash => "Slash",
            Self::Other(other) => other,
        }
    }
}

/// Line 注解意图（`/IT`，Table 175，PDF 1.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineIntent {
    /// 带箭头装饰的线（`/LineArrow`，缺省）。
    LineArrow,
    /// 线性标注，题注给出测量长度（`/LineDimension`）。
    LineDimension,
    /// 非标准名（原样保留）。
    Other(String),
}

impl LineIntent {
    /// 由 `/IT` 名字解析；未知名返回 [`LineIntent::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "LineArrow" => Self::LineArrow,
            "LineDimension" => Self::LineDimension,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准意图名；[`LineIntent::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::LineArrow => "LineArrow",
            Self::LineDimension => "LineDimension",
            Self::Other(other) => other,
        }
    }
}

/// Line 注解题注位置（`/CP`，仅 `/Cap` 为真时有意义，PDF 1.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptionPosition {
    /// 题注内联于连线延长线（`/Inline`，缺省）。
    Inline,
    /// 题注居中置于连线上方（`/Top`）。
    Top,
    /// 非标准名（原样保留）。
    Other(String),
}

impl CaptionPosition {
    /// 由 `/CP` 名字解析；未知名返回 [`CaptionPosition::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "Inline" => Self::Inline,
            "Top" => Self::Top,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准位置名；[`CaptionPosition::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Inline => "Inline",
            Self::Top => "Top",
            Self::Other(other) => other,
        }
    }
}

/// FreeText 注解意图（`/IT`，Table 174，PDF 1.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreeTextIntent {
    /// 普通文本框（`/FreeText`，缺省）。
    FreeText,
    /// 带引线标注的文本框（`/FreeTextCallout`，`/CL` 有意义）。
    FreeTextCallout,
    /// 点击即输/打字机式，不画引线（`/FreeTextTypeWriter`）。
    FreeTextTypeWriter,
    /// 非标准名（原样保留）。
    Other(String),
}

impl FreeTextIntent {
    /// 由 `/IT` 名字解析；未知名返回 [`FreeTextIntent::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "FreeText" => Self::FreeText,
            "FreeTextCallout" => Self::FreeTextCallout,
            "FreeTextTypeWriter" => Self::FreeTextTypeWriter,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准意图名；[`FreeTextIntent::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::FreeText => "FreeText",
            Self::FreeTextCallout => "FreeTextCallout",
            Self::FreeTextTypeWriter => "FreeTextTypeWriter",
            Self::Other(other) => other,
        }
    }
}

/// FreeText 引线（`/CL`，Table 174，仅意图为 `/FreeTextCallout` 时有意义）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalloutLine {
    /// 四点 `[x1 y1 x2 y2]`：起点与终点。
    Straight([f64; 4]),
    /// 六点 `[x1 y1 x2 y2 x3 y3]`：起点、拐点与终点。
    Knee([f64; 6]),
}

/// 边框效果样式（`/BE /S`，Table 167）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderEffectStyle {
    /// 无效果，边框按 `/BS` 原样绘制（`/S`，缺省）。
    NoEffect,
    /// 云状边框（`/C`），`/I` 仅在此样式下有效。
    Cloudy,
    /// 非标准名（原样保留）。
    Other(String),
}

impl BorderEffectStyle {
    /// 由 `/BE /S` 名字解析；未知名返回 [`BorderEffectStyle::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "S" => Self::NoEffect,
            "C" => Self::Cloudy,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准样式名；[`BorderEffectStyle::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::NoEffect => "S",
            Self::Cloudy => "C",
            Self::Other(other) => other,
        }
    }
}

/// 边框效果字典（`/BE`，Table 167）。
#[derive(Debug, Clone, PartialEq)]
pub struct BorderEffect {
    /// `/S` 样式；缺省为 [`BorderEffectStyle::NoEffect`]。
    pub style: BorderEffectStyle,
    /// `/I` 强度（0–2），仅在 `/S` 为云状时有效；缺省 `0`。
    pub intensity: Option<f64>,
}

/// 一次子类型专有条目解析的结果，字段由父模块并入 [`super::Annotation`]。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct SubtypeEntries {
    pub(super) icon_name: Option<String>,
    pub(super) is_open: Option<bool>,
    pub(super) state: Option<String>,
    pub(super) state_model: Option<String>,
    pub(super) symbol: Option<String>,
    pub(super) ink_paths: Vec<Vec<f64>>,
    pub(super) line_endpoints: Option<[f64; 4]>,
    pub(super) line_endings: Option<[LineEndStyle; 2]>,
    pub(super) leader_length: Option<f64>,
    pub(super) leader_extension: Option<f64>,
    pub(super) caption: Option<bool>,
    pub(super) line_intent: Option<LineIntent>,
    pub(super) leader_line_offset: Option<f64>,
    pub(super) caption_position: Option<CaptionPosition>,
    pub(super) caption_offset: Option<[f64; 2]>,
    pub(super) default_appearance: Option<String>,
    pub(super) quadding: Option<u8>,
    pub(super) default_style: Option<String>,
    pub(super) callout_line: Option<CalloutLine>,
    pub(super) free_text_intent: Option<FreeTextIntent>,
    pub(super) border_effect: Option<BorderEffect>,
    pub(super) text_rectangle: Option<[f64; 4]>,
    pub(super) callout_ending: Option<LineEndStyle>,
}

/// 解析注解字典中的子类型专有条目。
pub(super) fn parse_subtype_entries(dict: &Dict) -> Result<SubtypeEntries> {
    let (line_intent, free_text_intent) = parse_intents(dict)?;
    Ok(SubtypeEntries {
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
        line_endpoints: parse_exact_numbers(dict, "L", 4)?.map(|v| [v[0], v[1], v[2], v[3]]),
        line_endings: parse_line_endings(dict)?,
        leader_length: parse_optional_number(dict, "LL")?,
        leader_extension: parse_optional_number(dict, "LLE")?,
        caption: parse_optional_bool(dict, "Cap")?,
        line_intent,
        leader_line_offset: parse_optional_number(dict, "LLO")?,
        caption_position: parse_name_or_text(dict, "CP")?
            .map(|name| CaptionPosition::from_name(&name)),
        caption_offset: parse_exact_numbers(dict, "CO", 2)?.map(|v| [v[0], v[1]]),
        default_appearance: parse_name_or_text(dict, "DA")?,
        quadding: parse_quadding(dict)?,
        default_style: parse_name_or_text(dict, "DS")?,
        callout_line: parse_callout_line(dict)?,
        free_text_intent,
        border_effect: parse_border_effect(dict)?,
        text_rectangle: parse_text_rectangle(dict)?,
        callout_ending: parse_bare_line_ending(dict)?,
    })
}

/// `/IT` 同时属于 Line（Table 175）与 FreeText（Table 174），两集合不相交：
/// 命中哪一侧就填哪一侧；两侧都不认识的名字按声明的 `/Subtype` 归属路由，
/// 缺省落进 [`LineIntent::Other`] 保留原文。
fn parse_intents(dict: &Dict) -> Result<(Option<LineIntent>, Option<FreeTextIntent>)> {
    let Some(name) = parse_name_or_text(dict, "IT")? else {
        return Ok((None, None));
    };
    let free_text = FreeTextIntent::from_name(&name);
    if !matches!(free_text, FreeTextIntent::Other(_)) {
        return Ok((None, Some(free_text)));
    }
    if matches!(LineIntent::from_name(&name), LineIntent::Other(_))
        && dict.get("Subtype").and_then(Value::as_name) == Some("FreeText")
    {
        return Ok((None, Some(free_text)));
    }
    Ok((Some(LineIntent::from_name(&name)), None))
}

/// `/Q` 对齐方式（0 左、1 居中、2 右，Table 174）。
fn parse_quadding(dict: &Dict) -> Result<Option<u8>> {
    let Some(value) = dict.get("Q") else {
        return Ok(None);
    };
    let number = number(value, "annotation /Q")?;
    if number.fract() != 0.0 || !(0.0..=2.0).contains(&number) {
        return Err(malformed("annotation /Q must be 0, 1 or 2"));
    }
    Ok(Some(number as u8))
}

/// FreeText 引线 `/CL`：四或六个有限数值（Table 174）。
fn parse_callout_line(dict: &Dict) -> Result<Option<CalloutLine>> {
    let Some(values) = parse_number_array(dict, "CL", false)? else {
        return Ok(None);
    };
    match values.len() {
        4 => Ok(Some(CalloutLine::Straight([
            values[0], values[1], values[2], values[3],
        ]))),
        6 => Ok(Some(CalloutLine::Knee([
            values[0], values[1], values[2], values[3], values[4], values[5],
        ]))),
        _ => Err(malformed("annotation /CL must hold four or six numbers")),
    }
}

/// `/RD` 内部文本矩形与 `/Rect` 的四边差值，均须 ≥0（Table 174）。
fn parse_text_rectangle(dict: &Dict) -> Result<Option<[f64; 4]>> {
    let Some(values) = parse_exact_numbers(dict, "RD", 4)? else {
        return Ok(None);
    };
    if values.iter().any(|value| *value < 0.0) {
        return Err(malformed("annotation /RD differences must not be negative"));
    }
    Ok(Some([values[0], values[1], values[2], values[3]]))
}

/// `/BE` 边框效果字典（Table 167）：`/S` 样式名，`/I` 强度 0–2。
fn parse_border_effect(dict: &Dict) -> Result<Option<BorderEffect>> {
    let Some(value) = dict.get("BE") else {
        return Ok(None);
    };
    let effect = match value {
        Value::Dict(effect) => effect,
        _ => return Err(malformed("annotation /BE is not a dictionary")),
    };
    let style = match effect.get("S") {
        None => BorderEffectStyle::NoEffect,
        Some(name) => BorderEffectStyle::from_name(
            name.as_name()
                .ok_or_else(|| malformed("annotation /BE /S must be a name"))?,
        ),
    };
    let intensity = match effect.get("I") {
        None => None,
        Some(value) => {
            let number = number(value, "annotation /BE /I")?;
            if !(0.0..=2.0).contains(&number) {
                return Err(malformed("annotation /BE /I must be within 0.0–2.0"));
            }
            Some(number)
        }
    };
    Ok(Some(BorderEffect { style, intensity }))
}

/// FreeText 的单名字 `/LE`（引线端点样式，Table 174）；数组形态归 Line。
fn parse_bare_line_ending(dict: &Dict) -> Result<Option<LineEndStyle>> {
    let Some(name) = dict.get("LE").and_then(Value::as_name) else {
        return Ok(None);
    };
    Ok(Some(LineEndStyle::from_name(name)))
}

/// 恰好 `len` 个有限数值的数组条目（Line `/L` 四项、`/CO` 两项）。
fn parse_exact_numbers(dict: &Dict, key: &str, len: usize) -> Result<Option<Vec<f64>>> {
    let Some(values) = parse_number_array(dict, key, false)? else {
        return Ok(None);
    };
    if values.len() != len {
        return Err(malformed(format!(
            "annotation /{key} must hold {len} numbers"
        )));
    }
    Ok(Some(values))
}

/// Line `/LE`：一到两个端点样式名，缺省的一端按 `/None` 处理；
/// 单个名字的 FreeText 形态由 [`parse_bare_line_ending`] 承接，此处返回 `None`。
fn parse_line_endings(dict: &Dict) -> Result<Option<[LineEndStyle; 2]>> {
    let Some(value) = dict.get("LE") else {
        return Ok(None);
    };
    if value.as_name().is_some() {
        return Ok(None);
    }
    let array = value
        .as_array()
        .ok_or_else(|| malformed("annotation /LE is not an array"))?;
    if array.0.is_empty() || array.0.len() > 2 {
        return Err(malformed("annotation /LE must hold one or two names"));
    }
    let mut endings = [LineEndStyle::None, LineEndStyle::None];
    for (slot, item) in endings.iter_mut().zip(array.0.iter()) {
        let name = item
            .as_name()
            .ok_or_else(|| malformed("annotation /LE entries must be names"))?;
        *slot = LineEndStyle::from_name(name);
    }
    Ok(Some(endings))
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
