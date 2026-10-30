//! 媒体屏幕参数读取（13.2.6 Table 282–284）。
//!
//! `/SP` 字典分 `/MH` 必须满足与 `/BE` 尽力满足两组屏幕设置（Table 283）；
//! `/W` 为浮动窗口时 `/F` 浮动窗口参数（Table 284）必需。缺省值不物化，
//! 只有显式出现的条目才写入模型。

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Document, Value};

use super::{MAX_DEPTH, RenditionError, check_type, malformed};

/// media screen parameters 字典（Table 282）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaScreenParameters {
    /// `/MH` 必须满足的屏幕设置（Table 283）。
    pub must_honour: Option<MediaScreenSettings>,
    /// `/BE` 尽力满足的屏幕设置（Table 283）。
    pub best_effort: Option<MediaScreenSettings>,
}

/// 屏幕设置（Table 283）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaScreenSettings {
    /// `/W` 窗口类型。
    pub window: Option<MediaWindow>,
    /// `/B` 背景色 `[r g b]`，各分量 0.0..=1.0。
    pub background: Option<[f64; 3]>,
    /// `/O` 不透明度 0.0..=1.0。
    pub opacity: Option<f64>,
    /// `/M` 显示器编号（Table 293，0..=6）。
    pub monitor: Option<i64>,
    /// `/F` 浮动窗口参数（Table 284）；`/W` 为 `Floating` 时必需。
    pub floating: Option<MediaFloatingWindow>,
}

/// `/W` 窗口类型（Table 283）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaWindow {
    /// `0`：浮动窗口。
    Floating,
    /// `1`：全屏。
    FullScreen,
    /// `2`：隐藏。
    Hidden,
    /// `3`：屏幕注解矩形（缺省）。
    AnnotationRectangle,
    /// 未识别的枚举值（必须满足时播放器不可行）。
    Other(i64),
}

impl MediaWindow {
    fn from_code(code: i64) -> Self {
        match code {
            0 => Self::Floating,
            1 => Self::FullScreen,
            2 => Self::Hidden,
            3 => Self::AnnotationRectangle,
            other => Self::Other(other),
        }
    }
}

/// 浮动窗口参数（Table 284）。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaFloatingWindow {
    /// `/D` 播放矩形像素尺寸 `[width height]`，非负整数。
    pub dimensions: [i64; 2],
    /// `/RT` 调整窗口大小的参照目标（0..=3）。
    pub resize_target: Option<i64>,
    /// `/P` 窗口位置（0..=8）。
    pub position: Option<i64>,
    /// `/O` 窗口移出屏幕的方式（0..=2）。
    pub offscreen: Option<i64>,
    /// `/T` 是否显示标题栏。
    pub title_bar: Option<bool>,
    /// `/UC` 是否允许关闭窗口。
    pub closable: Option<bool>,
    /// `/R` 是否允许调整窗口大小（0..=2）。
    pub resizable: Option<i64>,
    /// `/TT` 标题栏文本行（允许为空数组）。
    pub title: Option<Vec<String>>,
}

pub(super) fn parse_screen_parameters(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<MediaScreenParameters, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed(
            "media screen parameters nesting exceeds the depth limit",
        ));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media screen parameters is not a dictionary"))?;
    check_type(dict, "MediaScreenParams", "media screen parameters")?;
    let must_honour = parse_screen_settings(document, dict.get("MH"), "MH", depth + 1)?;
    let best_effort = parse_screen_settings(document, dict.get("BE"), "BE", depth + 1)?;
    Ok(MediaScreenParameters {
        must_honour,
        best_effort,
    })
}

fn parse_screen_settings(
    document: &Document,
    value: Option<&Value>,
    key: &str,
    depth: usize,
) -> Result<Option<MediaScreenSettings>, RenditionError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if depth >= MAX_DEPTH {
        return Err(malformed(
            "media screen parameters nesting exceeds the depth limit",
        ));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved.as_dict().ok_or_else(|| {
        malformed(format!(
            "media screen parameters /{key} is not a dictionary"
        ))
    })?;
    let window = optional_integer(
        document,
        dict.get("W"),
        &format!("media screen parameters /{key} /W"),
    )?
    .map(MediaWindow::from_code);
    let background = match dict.get("B") {
        None => None,
        Some(value) => Some(parse_color(
            document,
            value,
            &format!("media screen parameters /{key} /B"),
        )?),
    };
    let opacity = optional_number(
        document,
        dict.get("O"),
        &format!("media screen parameters /{key} /O"),
    )?;
    if opacity.is_some_and(|opacity| !(0.0..=1.0).contains(&opacity)) {
        return Err(malformed(format!(
            "media screen parameters /{key} /O must be within 0.0..=1.0"
        )));
    }
    let monitor = optional_integer(
        document,
        dict.get("M"),
        &format!("media screen parameters /{key} /M"),
    )?;
    if monitor.is_some_and(|monitor| !(0..=6).contains(&monitor)) {
        return Err(malformed(format!(
            "media screen parameters /{key} /M must be within 0..=6"
        )));
    }
    let floating = match dict.get("F") {
        None => None,
        Some(value) => Some(parse_floating_window(document, value, depth + 1)?),
    };
    if window == Some(MediaWindow::Floating) && floating.is_none() {
        return Err(malformed(format!(
            "media screen parameters /{key} /F is required when /W is 0"
        )));
    }
    Ok(Some(MediaScreenSettings {
        window,
        background,
        opacity,
        monitor,
        floating,
    }))
}

fn parse_floating_window(
    document: &Document,
    value: &Value,
    depth: usize,
) -> Result<MediaFloatingWindow, RenditionError> {
    if depth >= MAX_DEPTH {
        return Err(malformed(
            "media floating window nesting exceeds the depth limit",
        ));
    }
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("media floating window is not a dictionary"))?;
    check_type(dict, "FWParams", "media floating window")?;
    let dimensions = match dict.get("D") {
        None => {
            return Err(malformed("media floating window is missing required /D"));
        }
        Some(value) => parse_dimensions(document, value)?,
    };
    let resize_target = ranged_integer(document, dict.get("RT"), "/RT", 0, 3)?;
    let position = ranged_integer(document, dict.get("P"), "/P", 0, 8)?;
    let offscreen = ranged_integer(document, dict.get("O"), "/O", 0, 2)?;
    let title_bar = optional_boolean(document, dict.get("T"), "media floating window /T")?;
    let closable = optional_boolean(document, dict.get("UC"), "media floating window /UC")?;
    let resizable = ranged_integer(document, dict.get("R"), "/R", 0, 2)?;
    let title = match dict.get("TT") {
        None => None,
        Some(value) => Some(parse_text_array(document, value)?),
    };
    Ok(MediaFloatingWindow {
        dimensions,
        resize_target,
        position,
        offscreen,
        title_bar,
        closable,
        resizable,
        title,
    })
}

fn parse_dimensions(document: &Document, value: &Value) -> Result<[i64; 2], RenditionError> {
    let resolved = resolve(document, value)?;
    let entries = resolved
        .as_array()
        .ok_or_else(|| malformed("media floating window /D is not an array"))?;
    if entries.0.len() != 2 {
        return Err(malformed(
            "media floating window /D must contain exactly 2 integers",
        ));
    }
    let mut dimensions = [0i64; 2];
    for (index, entry) in entries.0.iter().enumerate() {
        let resolved = resolve(document, entry)?;
        match resolved {
            Value::Integer(number) if number >= 0 => dimensions[index] = number,
            Value::Integer(_) => {
                return Err(malformed(
                    "media floating window /D entries must not be negative",
                ));
            }
            _ => {
                return Err(malformed(
                    "media floating window /D entries must be integers",
                ));
            }
        }
    }
    Ok(dimensions)
}

fn parse_color(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[f64; 3], RenditionError> {
    let resolved = resolve(document, value)?;
    let entries = resolved
        .as_array()
        .ok_or_else(|| malformed(format!("{context} is not an array")))?;
    if entries.0.len() != 3 {
        return Err(malformed(format!(
            "{context} must contain exactly 3 numbers"
        )));
    }
    let mut color = [0.0; 3];
    for (index, entry) in entries.0.iter().enumerate() {
        let number = number(document, entry, context)?;
        if !(0.0..=1.0).contains(&number) {
            return Err(malformed(format!(
                "{context} entries must be within 0.0..=1.0"
            )));
        }
        color[index] = number;
    }
    Ok(color)
}

fn parse_text_array(document: &Document, value: &Value) -> Result<Vec<String>, RenditionError> {
    let resolved = resolve(document, value)?;
    let entries = resolved
        .as_array()
        .ok_or_else(|| malformed("media floating window /TT is not an array"))?;
    let mut texts = Vec::with_capacity(entries.0.len());
    for entry in &entries.0 {
        let entry = resolve(document, entry)?;
        let text = text_value(&entry)
            .ok_or_else(|| malformed("media floating window /TT entries are not text strings"))?;
        texts.push(text);
    }
    Ok(texts)
}

fn ranged_integer(
    document: &Document,
    value: Option<&Value>,
    key: &str,
    low: i64,
    high: i64,
) -> Result<Option<i64>, RenditionError> {
    let number = optional_integer(document, value, &format!("media floating window {key}"))?;
    if number.is_some_and(|number| !(low..=high).contains(&number)) {
        return Err(malformed(format!(
            "media floating window {key} must be within {low}..={high}"
        )));
    }
    Ok(number)
}

fn optional_integer(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<i64>, RenditionError> {
    match value {
        None => Ok(None),
        Some(value) => match resolve(document, value)? {
            Value::Integer(number) => Ok(Some(number)),
            _ => Err(malformed(format!("{context} is not an integer"))),
        },
    }
}

fn optional_boolean(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<bool>, RenditionError> {
    match value {
        None => Ok(None),
        Some(value) => match resolve(document, value)? {
            Value::Bool(flag) => Ok(Some(flag)),
            _ => Err(malformed(format!("{context} is not a boolean"))),
        },
    }
}

fn optional_number(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<f64>, RenditionError> {
    match value {
        None => Ok(None),
        Some(value) => number(document, value, context).map(Some),
    }
}

fn number(document: &Document, value: &Value, context: &str) -> Result<f64, RenditionError> {
    match resolve(document, value)? {
        Value::Integer(number) => Ok(number as f64),
        Value::Float(number) if number.is_finite() => Ok(number),
        Value::Float(_) => Err(malformed(format!("{context} must be finite"))),
        _ => Err(malformed(format!("{context} is not a number"))),
    }
}
